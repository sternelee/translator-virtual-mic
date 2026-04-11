import Darwin
import Foundation

public enum EngineMode: Int32 {
    case bypass = 0
    case translate = 1
    case captionOnly = 2
    case muteOnFailure = 3
    case fallbackToBypass = 4
}

typealias EngineHandleRef = OpaquePointer

enum EngineLoaderError: Error {
    case openFailed(String)
    case symbolMissing(String)
}

final class EngineRuntime {
    typealias CreateFn = @convention(c) (UnsafePointer<CChar>?) -> EngineHandleRef?
    typealias DestroyFn = @convention(c) (EngineHandleRef?) -> Void
    typealias StartFn = @convention(c) (EngineHandleRef?) -> Int32
    typealias StopFn = @convention(c) (EngineHandleRef?) -> Int32
    typealias SetTargetLanguageFn = @convention(c) (EngineHandleRef?, UnsafePointer<CChar>?) -> Int32
    typealias EnableSharedOutputFn = @convention(c) (EngineHandleRef?, Int32, Int32, Int32) -> Int32
    typealias PushInputPcmFn = @convention(c) (EngineHandleRef?, UnsafePointer<Float>?, Int32, Int32, Int32, UInt64) -> Int32
    typealias LastErrorFn = @convention(c) (EngineHandleRef?) -> UnsafePointer<CChar>?
    typealias MetricsJsonFn = @convention(c) (EngineHandleRef?) -> UnsafePointer<CChar>?
    typealias SharedOutputPathFn = @convention(c) (EngineHandleRef?) -> UnsafePointer<CChar>?

    let create: CreateFn
    let destroy: DestroyFn
    let start: StartFn
    let stop: StopFn
    let setTargetLanguage: SetTargetLanguageFn
    let enableSharedOutput: EnableSharedOutputFn
    let pushInputPcm: PushInputPcmFn
    let lastError: LastErrorFn
    let metricsJson: MetricsJsonFn
    let sharedOutputPath: SharedOutputPathFn

    private let dylibHandle: UnsafeMutableRawPointer

    static func load() throws -> EngineRuntime {
        let candidates = dylibCandidates()
        var errors: [String] = []
        var loadedHandle: UnsafeMutableRawPointer?

        for path in candidates {
            guard FileManager.default.fileExists(atPath: path) else {
                errors.append("missing: \(path)")
                continue
            }
            if let handle = dlopen(path, RTLD_NOW | RTLD_LOCAL) {
                loadedHandle = handle
                break
            }
            let message = dlerror().map { String(cString: $0) } ?? "unknown dlopen error"
            errors.append("dlopen failed for \(path): \(message)")
        }

        guard let handle = loadedHandle else {
            throw EngineLoaderError.openFailed("unable to load libengine_api.dylib. Tried:\n\(errors.joined(separator: "\n"))")
        }

        func loadSymbol<T>(_ name: String, as type: T.Type) throws -> T {
            guard let symbol = dlsym(handle, name) else {
                throw EngineLoaderError.symbolMissing("missing symbol: \(name)")
            }
            return unsafeBitCast(symbol, to: type)
        }

        return try EngineRuntime(
            dylibHandle: handle,
            create: loadSymbol("engine_create", as: CreateFn.self),
            destroy: loadSymbol("engine_destroy", as: DestroyFn.self),
            start: loadSymbol("engine_start", as: StartFn.self),
            stop: loadSymbol("engine_stop", as: StopFn.self),
            setTargetLanguage: loadSymbol("engine_set_target_language", as: SetTargetLanguageFn.self),
            enableSharedOutput: loadSymbol("engine_enable_shared_output", as: EnableSharedOutputFn.self),
            pushInputPcm: loadSymbol("engine_push_input_pcm", as: PushInputPcmFn.self),
            lastError: loadSymbol("engine_get_last_error", as: LastErrorFn.self),
            metricsJson: loadSymbol("engine_get_metrics_json", as: MetricsJsonFn.self),
            sharedOutputPath: loadSymbol("engine_get_shared_output_path", as: SharedOutputPathFn.self)
        )
    }

    private static func dylibCandidates() -> [String] {
        var candidates: [String] = []
        let fm = FileManager.default

        if let envPath = ProcessInfo.processInfo.environment["TRANSLATOR_ENGINE_DYLIB"], !envPath.isEmpty {
            candidates.append(envPath)
        }

        let cwd = fm.currentDirectoryPath
        candidates.append(URL(fileURLWithPath: cwd).appendingPathComponent("target/debug/libengine_api.dylib").path)
        candidates.append(URL(fileURLWithPath: cwd).appendingPathComponent("../../../target/debug/libengine_api.dylib").path)

        if let executableURL = Bundle.main.executableURL {
            let executableDir = executableURL.deletingLastPathComponent()
            candidates.append(executableDir.appendingPathComponent("libengine_api.dylib").path)
            candidates.append(executableDir.appendingPathComponent("../libengine_api.dylib").standardized.path)
            candidates.append(executableDir.appendingPathComponent("../../../libengine_api.dylib").standardized.path)
            candidates.append(executableDir.appendingPathComponent("../../../../../target/debug/libengine_api.dylib").standardized.path)
        }

        let sourceFileURL = URL(fileURLWithPath: #filePath)
        let repoRoot = sourceFileURL
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        candidates.append(repoRoot.appendingPathComponent("target/debug/libengine_api.dylib").path)

        return Array(NSOrderedSet(array: candidates)) as? [String] ?? candidates
    }

    private init(
        dylibHandle: UnsafeMutableRawPointer,
        create: CreateFn,
        destroy: DestroyFn,
        start: StartFn,
        stop: StopFn,
        setTargetLanguage: SetTargetLanguageFn,
        enableSharedOutput: EnableSharedOutputFn,
        pushInputPcm: PushInputPcmFn,
        lastError: LastErrorFn,
        metricsJson: MetricsJsonFn,
        sharedOutputPath: SharedOutputPathFn
    ) {
        self.dylibHandle = dylibHandle
        self.create = create
        self.destroy = destroy
        self.start = start
        self.stop = stop
        self.setTargetLanguage = setTargetLanguage
        self.enableSharedOutput = enableSharedOutput
        self.pushInputPcm = pushInputPcm
        self.lastError = lastError
        self.metricsJson = metricsJson
        self.sharedOutputPath = sharedOutputPath
    }

    deinit {
        dlclose(dylibHandle)
    }
}
