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
        let dylibPath = ProcessInfo.processInfo.environment["TRANSLATOR_ENGINE_DYLIB"]
            ?? "../../../target/debug/libengine_api.dylib"

        guard let handle = dlopen(dylibPath, RTLD_NOW | RTLD_LOCAL) else {
            let message = String(cString: dlerror())
            throw EngineLoaderError.openFailed("dlopen failed for \(dylibPath): \(message)")
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
