import Foundation

#if canImport(KokoroPipeline)
import KokoroPipeline

/// Generates TTS audio using Kokoro CoreML models on Apple Neural Engine.
///
/// Architecture:
/// 1. Python tokenizer subprocess (`scripts/kokoro_coreml_tokenize.py`) converts
///    text → phonemes → input_ids, attention_mask, ref_s.
/// 2. Swift KokoroPipeline runs CoreML inference (duration → f0 → decoder → vocoder).
/// 3. Returns `[Float]` at 24 kHz, which the caller pushes to the Rust engine via
///    `pushTranslatedPCM` (Rust resamples to 48 kHz automatically).
///
/// Setup:
///   1. Clone https://github.com/mattmireles/kokoro-coreml
///   2. Install Python package: `pip install -e /path/to/kokoro-coreml`
///   3. Export CoreML models: follow kokoro-coreml README
///   4. Add `.package(path: "../third_party/kokoro-coreml/swift")` to Package.swift
///   5. Build with KokoroPipeline linked.
final class KokoroCoreMLService {

    struct Config {
        var modelsDirectory: URL
        var tokenizerScriptPath: String
        var voiceId: String
        var speed: Float
        var buckets: [Int]
        var linearWeights: [Float]
        var linearBias: Float

        static let `default` = Config(
            modelsDirectory: FileManager.default
                .urls(for: .applicationSupportDirectory, in: .userDomainMask)
                .first!
                .appendingPathComponent("translator-virtual-mic/models/kokoro-coreml"),
            tokenizerScriptPath: "scripts/kokoro_coreml_tokenize.py",
            voiceId: "af",
            speed: 1.0,
            buckets: [3, 7, 10, 15, 30],
            linearWeights: [],
            linearBias: 0.0
        )
    }

    enum ServiceError: Error, LocalizedError {
        case pipelineNotLoaded
        case tokenizerNotRunning
        case tokenizerError(String)
        case invalidResponse
        case synthesisFailed(String)

        var errorDescription: String? {
            switch self {
            case .pipelineNotLoaded: return "KokoroPipeline not loaded. Check CoreML models."
            case .tokenizerNotRunning: return "Python tokenizer subprocess is not running."
            case .tokenizerError(let msg): return "Tokenizer error: \(msg)"
            case .invalidResponse: return "Invalid response from tokenizer subprocess."
            case .synthesisFailed(let msg): return "CoreML synthesis failed: \(msg)"
            }
        }
    }

    private var config: Config
    private var pipeline: KokoroPipeline?
    private var tokenizerProcess: Process?
    private var tokenizerStdin: FileHandle?
    private var responseQueue: [CheckedContinuation<Data, Error>] = []
    private let queueLock = NSLock()

    init(config: Config = .default) {
        self.config = config
    }

    // MARK: - Lifecycle

    /// Load CoreML models and start the Python tokenizer subprocess.
    func start() throws {
        try loadPipeline()
        try startTokenizerSubprocess()
    }

    func stop() {
        if let proc = tokenizerProcess, proc.isRunning {
            proc.terminate()
        }
        tokenizerProcess = nil
        tokenizerStdin = nil
        pipeline = nil

        // Fail any pending continuations
        queueLock.lock()
        let pending = responseQueue
        responseQueue.removeAll()
        queueLock.unlock()
        for continuation in pending {
            continuation.resume(throwing: ServiceError.tokenizerNotRunning)
        }
    }

    var isRunning: Bool {
        pipeline != nil && tokenizerProcess?.isRunning == true
    }

    // MARK: - Synthesis

    /// Synthesize text to audio samples (24 kHz, mono, Float).
    func synthesize(text: String) async throws -> [Float] {
        guard pipeline != nil else {
            throw ServiceError.pipelineNotLoaded
        }
        guard tokenizerProcess?.isRunning == true else {
            throw ServiceError.tokenizerNotRunning
        }

        // 1. Tokenize via Python subprocess
        let tokenData = try await tokenizeViaPython(text: text)
        let tokenJSON = try JSONSerialization.jsonObject(with: tokenData) as? [String: Any]

        if let error = tokenJSON?["error"] as? String {
            throw ServiceError.tokenizerError(error)
        }

        guard let inputIds = tokenJSON?["input_ids"] as? [Int32],
              let attentionMask = tokenJSON?["attention_mask"] as? [Int32],
              let refS = tokenJSON?["ref_s"] as? [Float] else {
            throw ServiceError.invalidResponse
        }

        // 2. Run CoreML inference
        let result = try pipeline!.synthesize(
            inputIds: inputIds,
            attentionMask: attentionMask,
            refS: refS,
            speed: config.speed
        )

        return result.audio
    }

    // MARK: - Private

    private func loadPipeline() throws {
        let modelsDir = config.modelsDirectory
        guard FileManager.default.fileExists(atPath: modelsDir.path) else {
            throw ServiceError.pipelineNotLoaded
        }

        let contents = (try? FileManager.default.contentsOfDirectory(at: modelsDir, includingPropertiesForKeys: nil)) ?? []
        let hasPackages = contents.contains { $0.pathExtension == "mlpackage" }
        guard hasPackages else {
            throw ServiceError.pipelineNotLoaded
        }

        var weights = config.linearWeights
        var bias = config.linearBias
        let weightsURL = modelsDir.appendingPathComponent("hnsf_weights.json")
        if let data = try? Data(contentsOf: weightsURL),
           let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            if let w = json["linear_weights"] as? [Float] {
                weights = w
            }
            if let b = json["linear_bias"] as? Float {
                bias = b
            }
        }

        if weights.isEmpty {
            weights = [Float](repeating: 0.0, count: 128)
        }

        pipeline = try KokoroPipeline(
            modelsDirectory: modelsDir,
            buckets: config.buckets,
            linearWeights: weights,
            linearBias: bias
        )
    }

    private func startTokenizerSubprocess() throws {
        let scriptPath = resolveScriptPath()
        guard FileManager.default.fileExists(atPath: scriptPath) else {
            throw ServiceError.tokenizerError(
                "kokoro_coreml_tokenize.py not found at \(scriptPath)"
            )
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["python3", scriptPath]
        process.environment = ProcessInfo.processInfo.environment

        let stdinPipe = Pipe()
        let stdoutPipe = Pipe()
        process.standardInput = stdinPipe
        process.standardOutput = stdoutPipe
        process.standardError = stdoutPipe

        try process.run()
        tokenizerProcess = process
        tokenizerStdin = stdinPipe.fileHandleForWriting

        stdoutPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else { return }
            for line in text.components(separatedBy: .newlines) where !line.isEmpty {
                self?.handleTokenizerLine(line)
            }
        }
    }

    private func tokenizeViaPython(text: String) async throws -> Data {
        let request: [String: Any] = [
            "text": text,
            "voice_id": config.voiceId,
            "speed": config.speed,
        ]
        let data = try JSONSerialization.data(withJSONObject: request)
        let line = data + Data([0x0A])

        return try await withCheckedThrowingContinuation { continuation in
            queueLock.lock()
            responseQueue.append(continuation)
            queueLock.unlock()

            do {
                try tokenizerStdin?.write(contentsOf: line)
            } catch {
                queueLock.lock()
                if let idx = responseQueue.firstIndex(where: { $0 === continuation }) {
                    responseQueue.remove(at: idx)
                }
                queueLock.unlock()
                continuation.resume(throwing: error)
            }
        }
    }

    private func handleTokenizerLine(_ line: String) {
        guard let data = line.data(using: .utf8) else { return }

        queueLock.lock()
        guard let continuation = responseQueue.first else {
            queueLock.unlock()
            // Startup/status lines without matching request
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let status = json["status"] as? String {
                fputs("[KokoroCoreML] tokenizer status: \(status)\n", stderr)
            }
            if let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let error = json["error"] as? String {
                fputs("[KokoroCoreML] tokenizer error: \(error)\n", stderr)
            }
            return
        }
        responseQueue.removeFirst()
        queueLock.unlock()

        continuation.resume(returning: data)
    }

    private func resolveScriptPath() -> String {
        let candidates = [
            config.tokenizerScriptPath,
            "../scripts/kokoro_coreml_tokenize.py",
            "../../scripts/kokoro_coreml_tokenize.py",
        ]
        for path in candidates {
            if FileManager.default.fileExists(atPath: path) {
                return path
            }
        }
        return config.tokenizerScriptPath
    }
}

#else

// MARK: - Stub when KokoroPipeline is not linked

final class KokoroCoreMLService {
    struct Config {
        var modelsDirectory: URL
        var tokenizerScriptPath: String
        var voiceId: String
        var speed: Float

        static let `default` = Config(
            modelsDirectory: FileManager.default
                .urls(for: .applicationSupportDirectory, in: .userDomainMask)
                .first!
                .appendingPathComponent("translator-virtual-mic/models/kokoro-coreml"),
            tokenizerScriptPath: "scripts/kokoro_coreml_tokenize.py",
            voiceId: "af",
            speed: 1.0
        )
    }

    init(config: Config = .default) {}

    func start() throws {
        throw NSError(
            domain: "KokoroCoreML",
            code: -1,
            userInfo: [
                NSLocalizedDescriptionKey:
                    "KokoroPipeline not available. "
                    + "Add kokoro-coreml Swift package to Package.swift and rebuild. "
                    + "See docs/apple-silicon-model-research.md for setup instructions."
            ]
        )
    }

    func stop() {}
    var isRunning: Bool { false }

    func synthesize(text: String) async throws -> [Float] {
        throw NSError(
            domain: "KokoroCoreML",
            code: -1,
            userInfo: [
                NSLocalizedDescriptionKey:
                    "KokoroPipeline not linked. Build with kokoro-coreml package."
            ]
        )
    }
}

#endif
