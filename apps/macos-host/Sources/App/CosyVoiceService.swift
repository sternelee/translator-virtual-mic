import AVFoundation
import Foundation

/// Helper that manages the CosyVoice FastAPI sidecar process, reference-audio
/// recording, and voice-test playback. All public methods must be called from
/// the main actor (AppViewModel is @MainActor and owns this object).
final class CosyVoiceService: NSObject {

    // MARK: - Server

    private var serverProcess: Process?

    func startServer(scriptPath: String, port: Int, onLog: @escaping (String) -> Void) {
        guard !scriptPath.isEmpty else {
            onLog("CosyVoice: server script path is empty")
            return
        }
        stopServer(onLog: { _ in })

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["python3", scriptPath, "--port", String(port)]

        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe

        pipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else { return }
            let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else { return }
            DispatchQueue.main.async { onLog("CosyVoice server: \(trimmed)") }
        }

        process.terminationHandler = { [weak self] proc in
            DispatchQueue.main.async {
                self?.serverProcess = nil
                onLog("CosyVoice server exited (code \(proc.terminationStatus))")
            }
        }

        do {
            try process.run()
            serverProcess = process
            onLog("CosyVoice server started on port \(port)")
        } catch {
            onLog("CosyVoice server launch failed: \(error)")
        }
    }

    func stopServer(onLog: @escaping (String) -> Void) {
        guard let proc = serverProcess else { return }
        proc.terminate()
        serverProcess = nil
        onLog("CosyVoice server stopped")
    }

    var serverIsRunning: Bool {
        serverProcess?.isRunning ?? false
    }

    // MARK: - Reference audio recording

    /// Path where the reference WAV is saved.
    static var refWavPath: String {
        let home = NSHomeDirectory()
        let dir = (home as NSString).appendingPathComponent(".translator_virtual_mic")
        return (dir as NSString).appendingPathComponent("ref_voice.wav")
    }

    var refWavExists: Bool {
        FileManager.default.fileExists(atPath: Self.refWavPath)
    }

    private var audioRecorder: AVAudioRecorder?
    private var recordingTimer: Timer?
    private var autoStopItem: DispatchWorkItem?

    /// Returns elapsed-seconds updates via `onTick` and completion via `onStop`.
    func startRecording(
        onTick: @escaping (Double) -> Void,
        onStop: @escaping (String) -> Void
    ) {
        stopRecording()

        let dir = (Self.refWavPath as NSString).deletingLastPathComponent
        try? FileManager.default.createDirectory(
            atPath: dir,
            withIntermediateDirectories: true,
            attributes: nil
        )

        let url = URL(fileURLWithPath: Self.refWavPath)
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatLinearPCM,
            AVSampleRateKey: 22_050.0,
            AVNumberOfChannelsKey: 1,
            AVLinearPCMBitDepthKey: 16,
            AVLinearPCMIsFloatKey: false,
            AVLinearPCMIsBigEndianKey: false,
        ]

        do {
            let recorder = try AVAudioRecorder(url: url, settings: settings)
            recorder.delegate = self
            recorder.record()
            audioRecorder = recorder
        } catch {
            onStop("Recording failed to start: \(error)")
            return
        }

        var elapsed = 0.0
        recordingTimer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { _ in
            elapsed += 0.1
            DispatchQueue.main.async { onTick(elapsed) }
        }

        let item = DispatchWorkItem { [weak self] in
            self?.stopRecording()
            DispatchQueue.main.async { onStop("Reference recording saved (auto 10s)") }
        }
        autoStopItem = item
        DispatchQueue.main.asyncAfter(deadline: .now() + 10, execute: item)
    }

    func stopRecording() {
        autoStopItem?.cancel()
        autoStopItem = nil
        recordingTimer?.invalidate()
        recordingTimer = nil
        audioRecorder?.stop()
        audioRecorder = nil
    }

    // MARK: - Voice test playback

    private var audioEngine: AVAudioEngine?
    private var playerNode: AVAudioPlayerNode?

    func testVoice(
        port: Int,
        ttsText: String,
        promptText: String,
        onLog: @escaping (String) -> Void,
        completion: @escaping () -> Void
    ) {
        guard refWavExists else {
            onLog("Voice test: no reference audio — record a voice sample first")
            completion()
            return
        }

        onLog("Voice test: synthesising \"\(ttsText)\"...")

        Task {
            do {
                let pcm = try await fetchPCM(port: port, ttsText: ttsText, promptText: promptText)
                await MainActor.run { self.playPCM(data: pcm, sampleRate: 22_050, onLog: onLog) }
            } catch {
                await MainActor.run { onLog("Voice test error: \(error)") }
            }
            await MainActor.run { completion() }
        }
    }

    private func fetchPCM(port: Int, ttsText: String, promptText: String) async throws -> Data {
        let endpoint = URL(string: "http://127.0.0.1:\(port)/inference_zero_shot")!
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.timeoutInterval = 60

        let boundary = "----Boundary\(UUID().uuidString.replacingOccurrences(of: "-", with: ""))"
        request.setValue(
            "multipart/form-data; boundary=\(boundary)",
            forHTTPHeaderField: "Content-Type"
        )

        var body = Data()
        let crlf = "\r\n"

        func appendField(_ name: String, _ value: String) {
            body.append("--\(boundary)\(crlf)".utf8Data)
            body.append("Content-Disposition: form-data; name=\"\(name)\"\(crlf)\(crlf)".utf8Data)
            body.append("\(value)\(crlf)".utf8Data)
        }

        appendField("tts_text", ttsText)
        appendField("prompt_text", promptText)

        let wavData = try Data(contentsOf: URL(fileURLWithPath: Self.refWavPath))
        body.append("--\(boundary)\(crlf)".utf8Data)
        body.append(
            "Content-Disposition: form-data; name=\"prompt_wav\"; filename=\"ref_voice.wav\"\(crlf)".utf8Data
        )
        body.append("Content-Type: audio/wav\(crlf)\(crlf)".utf8Data)
        body.append(wavData)
        body.append(crlf.utf8Data)
        body.append("--\(boundary)--\(crlf)".utf8Data)

        request.httpBody = body

        let (data, _) = try await URLSession.shared.data(for: request)
        return data
    }

    @MainActor
    private func playPCM(data: Data, sampleRate: Double, onLog: (String) -> Void) {
        audioEngine?.stop()
        audioEngine = nil
        playerNode = nil

        guard !data.isEmpty else {
            onLog("Voice test: empty response from CosyVoice server")
            return
        }

        let sampleCount = data.count / 2 // int16 = 2 bytes
        let format = AVAudioFormat(
            commonFormat: .pcmFormatFloat32,
            sampleRate: sampleRate,
            channels: 1,
            interleaved: false
        )!

        guard let buffer = AVAudioPCMBuffer(
            pcmFormat: format,
            frameCapacity: AVAudioFrameCount(sampleCount)
        ) else {
            onLog("Voice test: failed to create PCM buffer")
            return
        }
        buffer.frameLength = AVAudioFrameCount(sampleCount)

        // Convert int16 LE → float32 in [-1, 1]
        let floatPtr = buffer.floatChannelData![0]
        data.withUnsafeBytes { rawPtr in
            let int16Ptr = rawPtr.bindMemory(to: Int16.self)
            for i in 0..<sampleCount {
                floatPtr[i] = Float(int16Ptr[i]) / 32_768.0
            }
        }

        let engine = AVAudioEngine()
        let player = AVAudioPlayerNode()
        engine.attach(player)
        engine.connect(player, to: engine.mainMixerNode, format: format)

        do {
            try engine.start()
            player.scheduleBuffer(buffer, completionHandler: nil)
            player.play()
            audioEngine = engine
            playerNode = player
            onLog("Voice test: playing \(sampleCount) samples @ \(Int(sampleRate)) Hz")
        } catch {
            onLog("Voice test: playback error: \(error)")
        }
    }
}

// MARK: - AVAudioRecorderDelegate

extension CosyVoiceService: AVAudioRecorderDelegate {
    func audioRecorderEncodeErrorDidOccur(_ recorder: AVAudioRecorder, error: Error?) {
        if let error {
            DispatchQueue.main.async {
                fputs("[CosyVoiceService] Recorder encode error: \(error)\n", stderr)
            }
        }
    }
}

// MARK: - Private helpers

private extension String {
    var utf8Data: Data { Data(utf8) }
}
