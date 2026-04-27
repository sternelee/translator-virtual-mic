import Foundation

// Errors thrown inside the pipeline (logged, never surfaced to UI).
private enum PipelineError: Error {
    case httpError(Int, String)
    case missingApiKey(String)
    case missingVoiceId
}

/// Three-step pipeline: Whisper ASR → GPT-4o MT → ElevenLabs TTS.
///
/// Thread safety: `onAudioChunk` is called from the AVCapture background thread.
/// All mutable state is protected by `lock`.
final class ElevenLabsPipelineService {

    // MARK: - Configuration

    /// RMS level below which a frame is considered silence (≈ –38 dBFS).
    private let silenceThresholdRMS: Float = 0.012
    /// Number of consecutive silent frames required to trigger pipeline (500 ms @ 48 kHz).
    private let silenceWindowFrames: Int = 24_000
    /// Minimum accumulated frames before an utterance is eligible (800 ms @ 48 kHz).
    private let minUtteranceFrames: Int = 38_400

    // MARK: - State (protected by `lock`)

    private let lock = NSLock()
    private var accumulatedSamples: [Float] = []
    private var accumulatedSampleRate: Int = 48_000
    private var silenceFrameCount: Int = 0
    private var isPipelineRunning: Bool = false
    private var isStopped: Bool = false

    // MARK: - Dependencies (set via configure)

    private weak var engine: EngineBox?
    private var targetLocale: String = "en-US"

    // MARK: - Public API

    func configure(engine: EngineBox, targetLocale: String) {
        self.engine = engine
        self.targetLocale = targetLocale
    }

    func stop() {
        lock.lock()
        defer { lock.unlock() }
        isStopped = true
        accumulatedSamples = []
        silenceFrameCount = 0
        isPipelineRunning = false
    }

    func reset() {
        lock.lock()
        defer { lock.unlock() }
        isStopped = false
        accumulatedSamples = []
        silenceFrameCount = 0
        isPipelineRunning = false
    }

    /// Called from the AVCapture background thread on every audio chunk.
    func onAudioChunk(_ chunk: MicrophoneCaptureService.PCMChunk) {
        lock.lock()
        defer { lock.unlock() }

        guard !isStopped else { return }

        accumulatedSamples.append(contentsOf: chunk.samples)
        accumulatedSampleRate = chunk.sampleRate

        if chunk.rmsLevel < silenceThresholdRMS {
            silenceFrameCount += chunk.frameCount
        } else {
            silenceFrameCount = 0
        }

        let shouldProcess = !isPipelineRunning
            && silenceFrameCount >= silenceWindowFrames
            && accumulatedSamples.count >= minUtteranceFrames

        guard shouldProcess else { return }

        let utterance = accumulatedSamples
        let sampleRate = accumulatedSampleRate
        accumulatedSamples = []
        silenceFrameCount = 0
        isPipelineRunning = true

        Task {
            await self.runPipeline(utterance: utterance, sampleRate: sampleRate)
            self.lock.lock()
            self.isPipelineRunning = false
            self.lock.unlock()
        }
    }

    // MARK: - Pipeline

    private func runPipeline(utterance: [Float], sampleRate: Int) async {
        do {
            let transcript = try await transcribe(samples: utterance, sampleRate: sampleRate)
            guard !transcript.isEmpty else {
                NSLog("[ElevenLabsPipeline] empty transcript, skipping")
                return
            }

            let translation = try await translate(text: transcript, targetLocale: targetLocale)
            guard !translation.isEmpty else {
                NSLog("[ElevenLabsPipeline] empty translation, skipping")
                return
            }

            NSLog("[ElevenLabsPipeline] transcript='\(transcript)' → translation='\(translation)'")

            let pcmSamples = try await synthesize(text: translation)
            guard !pcmSamples.isEmpty else { return }

            pushToEngine(samples: pcmSamples)
        } catch {
            NSLog("[ElevenLabsPipeline] pipeline error: \(error)")
        }
    }

    // MARK: - Step 1: Whisper ASR

    private func transcribe(samples: [Float], sampleRate: Int) async throws -> String {
        let apiKey = ProcessInfo.processInfo.environment["OPENAI_API_KEY"] ?? ""
        guard !apiKey.isEmpty else { throw PipelineError.missingApiKey("OPENAI_API_KEY") }

        let wavData = buildWAV(samples: samples, inputSampleRate: UInt32(sampleRate))

        var request = URLRequest(url: URL(string: "https://api.openai.com/v1/audio/transcriptions")!)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")

        let boundary = "Boundary-\(UUID().uuidString)"
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")

        var body = Data()
        // file part
        body.append("--\(boundary)\r\n".data(using: .utf8)!)
        body.append("Content-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\n".data(using: .utf8)!)
        body.append("Content-Type: audio/wav\r\n\r\n".data(using: .utf8)!)
        body.append(wavData)
        body.append("\r\n".data(using: .utf8)!)
        // model part
        body.append("--\(boundary)\r\n".data(using: .utf8)!)
        body.append("Content-Disposition: form-data; name=\"model\"\r\n\r\n".data(using: .utf8)!)
        body.append("whisper-1\r\n".data(using: .utf8)!)
        body.append("--\(boundary)--\r\n".data(using: .utf8)!)
        request.httpBody = body

        let (data, response) = try await URLSession.shared.data(for: request)
        if let http = response as? HTTPURLResponse, http.statusCode != 200 {
            let bodyStr = String(data: data, encoding: .utf8) ?? ""
            throw PipelineError.httpError(http.statusCode, bodyStr)
        }

        let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        return (json?["text"] as? String) ?? ""
    }

    // MARK: - Step 2: GPT-4o Translation

    private func translate(text: String, targetLocale: String) async throws -> String {
        let apiKey = ProcessInfo.processInfo.environment["OPENAI_API_KEY"] ?? ""
        guard !apiKey.isEmpty else { throw PipelineError.missingApiKey("OPENAI_API_KEY") }

        var request = URLRequest(url: URL(string: "https://api.openai.com/v1/chat/completions")!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")

        let body: [String: Any] = [
            "model": "gpt-4o",
            "messages": [
                [
                    "role": "system",
                    "content": "You are a professional translator. Translate the following text to \(targetLocale). Output ONLY the translated text with no explanation, preamble, or quotation marks.",
                ],
                ["role": "user", "content": text],
            ],
            "max_tokens": 1024,
        ]
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        let (data, response) = try await URLSession.shared.data(for: request)
        if let http = response as? HTTPURLResponse, http.statusCode != 200 {
            let bodyStr = String(data: data, encoding: .utf8) ?? ""
            throw PipelineError.httpError(http.statusCode, bodyStr)
        }

        let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let choices = json?["choices"] as? [[String: Any]]
        let message = choices?.first?["message"] as? [String: Any]
        return (message?["content"] as? String) ?? ""
    }

    // MARK: - Step 3: ElevenLabs TTS

    private func synthesize(text: String) async throws -> [Float] {
        let apiKey = ProcessInfo.processInfo.environment["ELEVENLABS_API_KEY"] ?? ""
        guard !apiKey.isEmpty else { throw PipelineError.missingApiKey("ELEVENLABS_API_KEY") }
        let voiceId = ProcessInfo.processInfo.environment["ELEVENLABS_VOICE_ID"] ?? ""
        guard !voiceId.isEmpty else { throw PipelineError.missingVoiceId }

        let modelId = ProcessInfo.processInfo.environment["ELEVENLABS_MODEL_ID"] ?? "eleven_multilingual_v2"
        let urlString = "https://api.elevenlabs.io/v1/text-to-speech/\(voiceId)/stream?output_format=pcm_24000"
        var request = URLRequest(url: URL(string: urlString)!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue(apiKey, forHTTPHeaderField: "xi-api-key")

        let body: [String: Any] = [
            "text": text,
            "model_id": modelId,
            "voice_settings": ["stability": 0.5, "similarity_boost": 0.75],
        ]
        request.httpBody = try JSONSerialization.data(withJSONObject: body)

        let (data, response) = try await URLSession.shared.data(for: request)
        if let http = response as? HTTPURLResponse, http.statusCode != 200 {
            let bodyStr = String(data: data, encoding: .utf8) ?? ""
            throw PipelineError.httpError(http.statusCode, bodyStr)
        }

        // Response is raw Int16 LE PCM at 24 kHz mono.
        let sampleCount = data.count / 2
        guard sampleCount > 0 else { return [] }
        var samples = [Float](repeating: 0, count: sampleCount)
        data.withUnsafeBytes { rawBuffer in
            for i in 0..<sampleCount {
                let int16 = rawBuffer.load(fromByteOffset: i * 2, as: Int16.self)
                samples[i] = Float(Int16(littleEndian: int16)) / 32_767.0
            }
        }
        return samples
    }

    // MARK: - Push to engine

    private func pushToEngine(samples: [Float]) {
        guard let engine else { return }
        let frameCount = Int32(samples.count)
        _ = engine.pushTranslatedPCM(
            samples: samples,
            frameCount: frameCount,
            channels: 1,
            sampleRate: 24_000,
            timestampNs: UInt64(Date().timeIntervalSince1970 * 1_000_000_000)
        )
    }

    // MARK: - WAV builder

    /// Build a 16-bit PCM WAV from f32 samples, resampling to 16 kHz for Whisper.
    private func buildWAV(samples: [Float], inputSampleRate: UInt32) -> Data {
        let resampled = inputSampleRate == 16_000
            ? samples
            : resampleLinear(samples, fromRate: inputSampleRate, toRate: 16_000)

        let int16Samples = resampled.map { sample -> Int16 in
            let clamped = max(-1.0, min(1.0, sample))
            return Int16(clamped * 32_767)
        }

        let pcmData: Data = int16Samples.withUnsafeBufferPointer { Data(buffer: $0) }
        let dataSize = UInt32(pcmData.count)
        let numChannels: UInt16 = 1
        let sampleRate: UInt32 = 16_000
        let bitsPerSample: UInt16 = 16
        let byteRate = sampleRate * UInt32(numChannels) * UInt32(bitsPerSample) / 8
        let blockAlign = numChannels * bitsPerSample / 8

        var header = Data()
        header.append(contentsOf: "RIFF".utf8)
        appendLE(UInt32(36 + dataSize), to: &header)
        header.append(contentsOf: "WAVE".utf8)
        header.append(contentsOf: "fmt ".utf8)
        appendLE(UInt32(16), to: &header)      // subchunk1Size = 16 for PCM
        appendLE(UInt16(1), to: &header)        // audioFormat = 1 (PCM)
        appendLE(numChannels, to: &header)
        appendLE(sampleRate, to: &header)
        appendLE(byteRate, to: &header)
        appendLE(blockAlign, to: &header)
        appendLE(bitsPerSample, to: &header)
        header.append(contentsOf: "data".utf8)
        appendLE(dataSize, to: &header)
        return header + pcmData
    }

    private func resampleLinear(_ samples: [Float], fromRate: UInt32, toRate: UInt32) -> [Float] {
        guard fromRate != toRate, !samples.isEmpty else { return samples }
        let ratio = Double(fromRate) / Double(toRate)
        let outputCount = max(1, Int(Double(samples.count) / ratio))
        var output = [Float](repeating: 0, count: outputCount)
        for i in 0..<outputCount {
            let srcPos = Double(i) * ratio
            let lo = Int(srcPos)
            let hi = min(lo + 1, samples.count - 1)
            let frac = Float(srcPos - Double(lo))
            output[i] = samples[lo] * (1 - frac) + samples[hi] * frac
        }
        return output
    }

    private func appendLE<T: FixedWidthInteger>(_ value: T, to data: inout Data) {
        var le = value.littleEndian
        withUnsafeBytes(of: &le) { data.append(contentsOf: $0) }
    }
}
