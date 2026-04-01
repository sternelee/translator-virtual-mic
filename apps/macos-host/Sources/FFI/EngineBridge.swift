import Foundation

final class EngineBox {
    private let runtime: EngineRuntime?
    private let runtimeError: String?
    private var handle: EngineHandleRef?

    init(configJSON: String) {
        do {
            let runtime = try EngineRuntime.load()
            self.runtime = runtime
            self.runtimeError = nil
            self.handle = configJSON.withCString { runtime.create($0) }
        } catch {
            self.runtime = nil
            self.runtimeError = String(describing: error)
            self.handle = nil
        }
    }

    deinit {
        guard let runtime, let handle else { return }
        runtime.destroy(handle)
    }

    func start() -> Int32 {
        guard let runtime, let handle else { return -1 }
        return runtime.start(handle)
    }

    func stop() -> Int32 {
        guard let runtime, let handle else { return -1 }
        return runtime.stop(handle)
    }

    func setTargetLanguage(_ language: String) -> Int32 {
        guard let runtime, let handle else { return -1 }
        return language.withCString { runtime.setTargetLanguage(handle, $0) }
    }

    func enableSharedOutput(capacityFrames: Int32, channels: Int32, sampleRate: Int32) -> Int32 {
        guard let runtime, let handle else { return -1 }
        return runtime.enableSharedOutput(handle, capacityFrames, channels, sampleRate)
    }

    func pushInputPCM(samples: [Float], frameCount: Int32, channels: Int32, sampleRate: Int32, timestampNs: UInt64) -> Int32 {
        guard let runtime, let handle else { return -1 }
        return samples.withUnsafeBufferPointer { buffer in
            runtime.pushInputPcm(handle, buffer.baseAddress, frameCount, channels, sampleRate, timestampNs)
        }
    }

    func metricsJSON() -> String {
        guard let runtime, let handle, let raw = runtime.metricsJson(handle) else {
            return "{}"
        }
        return String(cString: raw)
    }

    func sharedOutputPath() -> String {
        guard let runtime, let handle, let raw = runtime.sharedOutputPath(handle) else {
            return ""
        }
        return String(cString: raw)
    }

    func lastError() -> String {
        if let runtimeError {
            return runtimeError
        }
        guard let runtime, let handle, let raw = runtime.lastError(handle) else {
            return "unknown"
        }
        return String(cString: raw)
    }
}
