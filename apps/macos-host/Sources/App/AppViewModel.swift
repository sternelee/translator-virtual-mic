import AVFoundation
import Combine
import CoreAudio
import Foundation

struct SharedBufferMonitorSnapshot {
    let fileExists: Bool
    let sampleRate: UInt32
    let channelCount: UInt32
    let capacityFrames: UInt32
    let writeIndexFrames: UInt64
    let readIndexFrames: UInt64
    let lastTimestampNs: UInt64

    static let missing = SharedBufferMonitorSnapshot(
        fileExists: false,
        sampleRate: 0,
        channelCount: 0,
        capacityFrames: 0,
        writeIndexFrames: 0,
        readIndexFrames: 0,
        lastTimestampNs: 0
    )
}

enum TranslationServiceProvider: String, CaseIterable, Identifiable {
    case none = "none"
    case openAIRealtime = "openai_realtime"
    case azureVoiceLive = "azure_voice_live"
    case elevenLabs = "eleven_labs"
    case localCaption = "local_caption"

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .none:
            "Off"
        case .openAIRealtime:
            "OpenAI Realtime"
        case .azureVoiceLive:
            "Azure Voice Live"
        case .elevenLabs:
            "ElevenLabs"
        case .localCaption:
            "Local Caption"
        }
    }
}

@MainActor
final class AppViewModel: ObservableObject {
    @Published var devices: [AudioDevice] = []
    @Published var selectedDeviceID: AudioDeviceID?
    @Published var selectedDeviceUID: String?
    @Published var statusText: String = "Idle"
    @Published var logLines: [String] = []
    @Published var selectedTranslationProvider: TranslationServiceProvider = .openAIRealtime
    @Published var targetLanguage: String = "en"
    @Published var inputGainDB: Double = 6.0
    @Published var limiterThresholdDB: Double = -6.0
    @Published var microphonePermissionGranted: Bool = false
    @Published var inputLevel: Float = 0
    @Published var metricsJSON: String = "{}"
    @Published var sharedOutputPath: String = ""
    @Published var sharedBufferStatusText: String = "Shared buffer idle"
    @Published var translationStateJSON: String = "{}"
    @Published var currentCaption: String = ""
    @Published var captionStateJSON: String = "{}"
    @Published var selectedLocalSttModelId: String = "paraformer-zh"
    @Published var localSttModelDownloadState: DownloadState = .idle
    @Published var selectedLocalMtModelId: String = "opus-mt-zh-en"
    @Published var localMtEnabled: Bool = false
    @Published var localMtModelDownloadState: DownloadState = .idle
    @Published var ttsEnabled: Bool = false
    @Published var selectedTtsModelId: String = "kokoro-en-v0_19"
    @Published var ttsSpeed: Double = 1.0
    @Published var ttsModelDownloadState: DownloadState = .idle

    private let captureService = MicrophoneCaptureService()
    private let azureVoiceLiveService = AzureVoiceLiveService()
    private let openAIRealtimeService = OpenAIRealtimeService()
    private let elevenLabsPipelineService = ElevenLabsPipelineService()
    private let captionService = CaptionService()
    private let modelDownloadService = ModelDownloadService()
    private var engine: EngineBox?
    private var downloadCancellable: AnyCancellable?
    private var mtDownloadCancellable: AnyCancellable?
    private var ttsDownloadCancellable: AnyCancellable?
    private var sharedBufferMonitorTask: Task<Void, Never>?
    private var lastSharedBufferSnapshot: SharedBufferMonitorSnapshot = .missing

    init() {
        refreshDevices()
        requestMicrophonePermission()
    }

    func requestMicrophonePermission() {
        AVCaptureDevice.requestAccess(for: .audio) { granted in
            Task { @MainActor in
                self.microphonePermissionGranted = granted
                self.appendLog(granted ? "Microphone permission granted" : "Microphone permission denied")
            }
        }
    }

    func refreshDevices() {
        devices = AudioDevice.enumerateInputDevices()
        // 排除虚拟麦克风，选择真正的物理麦克风
        let physicalDevices = devices.filter { !$0.uid.contains("translator.virtual.mic") }

        // 如果当前选中的是虚拟麦克风，重新选择物理麦克风
        if let currentUID = selectedDeviceUID, currentUID.contains("translator.virtual.mic") {
            selectedDeviceID = nil
            selectedDeviceUID = nil
        }

        if selectedDeviceID == nil {
            if let firstPhysical = physicalDevices.first {
                selectedDeviceID = firstPhysical.id
                selectedDeviceUID = firstPhysical.uid
                appendLog("Auto-selected physical device: \(firstPhysical.name)")
            } else if let first = devices.first {
                selectedDeviceID = first.id
                selectedDeviceUID = first.uid
            }
        } else if let selectedDeviceID,
                  let device = devices.first(where: { $0.id == selectedDeviceID }) {
            selectedDeviceUID = device.uid
        }
        appendLog("Enumerated \(devices.count) input device(s)")
    }

    func selectDevice(_ device: AudioDevice) {
        selectedDeviceID = device.id
        selectedDeviceUID = device.uid
        appendLog("Selected input device: \(device.name)")
    }

    func startEngine() {
        stopEngine()
        appendLog("Starting engine with device UID: \(selectedDeviceUID ?? "nil")")
        let configJSON = buildEngineConfigJSON()
        appendLog("Engine config: \(configJSON)")
        let engine = EngineBox(configJSON: configJSON)
        guard engine.start() == 0 else {
            statusText = "Failed"
            appendLog("engine_start failed (or dylib failed to load): \(engine.lastError())")
            return
        }

        let engineMode: EngineMode = switch selectedTranslationProvider {
        case .none: .bypass
        case .localCaption: .captionOnly
        default: .translate
        }
        _ = engine.setTargetLanguage(targetLanguage)
        _ = engine.setMode(engineMode)
        let sharedResult = engine.enableSharedOutput(capacityFrames: 14_400, channels: 1, sampleRate: 48_000)
        appendLog("enableSharedOutput result: \(sharedResult)")
        if sharedResult != 0 {
            _ = engine.stop()
            statusText = "Failed"
            appendLog("enableSharedOutput failed: \(engine.lastError())")
            return
        }
        sharedOutputPath = engine.sharedOutputPath()
        appendLog("sharedOutputPath: \(sharedOutputPath)")
        if sharedOutputPath.isEmpty {
            _ = engine.stop()
            statusText = "Failed"
            appendLog("sharedOutputPath is empty after enabling shared output")
            return
        }
        refreshSharedBufferStatus(logOnChange: true)

        do {
            var chunkCount = 0
            try captureService.start(deviceUID: selectedDeviceUID) { [weak self] chunk in
                guard let self else { return }
                chunkCount += 1
                let result = engine.pushInputPCM(
                    samples: chunk.samples,
                    frameCount: Int32(chunk.frameCount),
                    channels: Int32(chunk.channels),
                    sampleRate: Int32(chunk.sampleRate),
                    timestampNs: chunk.timestampNs
                )

                // Route to ElevenLabs pipeline when active (no-op if stopped).
                self.elevenLabsPipelineService.onAudioChunk(chunk)

                Task { @MainActor in
                    self.inputLevel = chunk.rmsLevel
                    self.metricsJSON = engine.metricsJSON()
                    if result != 0 {
                        self.statusText = "Degraded"
                        self.appendLog("engine_push_input_pcm failed: \(engine.lastError())")
                    }
                    // Log first few chunks for debugging
                    if chunkCount <= 3 {
                        self.appendLog("chunk#\(chunkCount): frames=\(chunk.frameCount) rms=\(String(format: "%.6f", chunk.rmsLevel)) samples=[\(chunk.samples.prefix(3).map { String(format: "%.4f", $0) }.joined(separator: ","))...]")
                    }
                }
            }
        } catch {
            _ = engine.stop()
            statusText = "Failed"
            appendLog("Microphone capture start failed: \(error)")
            return
        }

        self.engine = engine
        statusText = "Listening"
        metricsJSON = engine.metricsJSON()
        translationStateJSON = engine.translationStateJSON()
        currentCaption = ""
        captionStateJSON = "{}"
        appendLog("Engine started")
        if !sharedOutputPath.isEmpty {
            appendLog("Shared output file: \(sharedOutputPath)")
        }
        if selectedTranslationProvider == .localCaption {
            captionService.start(engine: engine)
            appendLog("Local caption service started")
        } else {
            startTranslationService(using: engine)
        }
        startSharedBufferMonitor()
    }

    func stopEngine() {
        captureService.stop()
        azureVoiceLiveService.stop()
        openAIRealtimeService.stop()
        elevenLabsPipelineService.stop()
        captionService.stop()
        stopSharedBufferMonitor()
        guard let engine else { return }
        _ = engine.stop()
        self.engine = nil
        statusText = "Idle"
        inputLevel = 0
        sharedOutputPath = ""
        sharedBufferStatusText = "Shared buffer idle"
        translationStateJSON = "{}"
        currentCaption = ""
        captionStateJSON = "{}"
        lastSharedBufferSnapshot = .missing
        appendLog("Engine stopped")
    }

    private func buildEngineConfigJSON() -> String {
        let sourceLocale = "auto"
        let targetLocale: String = switch targetLanguage {
        case "zh":
            "zh-CN"
        case "ja":
            "ja-JP"
        default:
            "en-US"
        }

        let mode: String = switch selectedTranslationProvider {
        case .none: "bypass"
        case .localCaption: "caption_only"
        default: "translate"
        }

        let azureEndpoint = ProcessInfo.processInfo.environment["AZURE_VOICELIVE_ENDPOINT"] ?? ""
        let azureModel = ProcessInfo.processInfo.environment["AZURE_VOICELIVE_MODEL"] ?? "gpt-realtime"
        let azureVoiceName = ProcessInfo.processInfo.environment["AZURE_VOICELIVE_VOICE_NAME"]
            ?? (targetLanguage == "zh" ? "zh-CN-XiaoxiaoNeural" : targetLanguage == "ja" ? "ja-JP-NanamiNeural" : "en-US-Ava:DragonHDLatestNeural")
        let openAIEndpoint = ProcessInfo.processInfo.environment["OPENAI_REALTIME_ENDPOINT"] ?? "wss://api.openai.com/v1/realtime"
        let openAIModel = ProcessInfo.processInfo.environment["OPENAI_REALTIME_MODEL"] ?? "gpt-realtime"
        let openAIVoiceName = ProcessInfo.processInfo.environment["OPENAI_REALTIME_VOICE_NAME"] ?? "marin"

        var base = String(
            format: #"{"target":"%@","mode":"%@","translation_provider":"%@","azure_voice_live_endpoint":"%@","azure_voice_live_model":"%@","azure_voice_live_api_key_env":"AZURE_VOICELIVE_API_KEY","azure_voice_live_voice_name":"%@","azure_voice_live_source_locale":"%@","azure_voice_live_target_locale":"%@","openai_realtime_endpoint":"%@","openai_realtime_model":"%@","openai_realtime_api_key_env":"OPENAI_API_KEY","openai_realtime_voice_name":"%@","openai_realtime_source_locale":"%@","openai_realtime_target_locale":"%@","input_gain_db":%.2f,"limiter_threshold_db":%.2f}"#,
            targetLanguage,
            mode,
            selectedTranslationProvider.rawValue,
            azureEndpoint,
            azureModel,
            azureVoiceName,
            sourceLocale,
            targetLocale,
            openAIEndpoint,
            openAIModel,
            openAIVoiceName,
            sourceLocale,
            targetLocale,
            inputGainDB,
            limiterThresholdDB
        )

        if selectedTranslationProvider == .localCaption {
            let env = ProcessInfo.processInfo.environment
            let modelId = env["LOCAL_STT_MODEL_ID"] ?? selectedLocalSttModelId
            let defaultModelDir = FileManager.default
                .urls(for: .applicationSupportDirectory, in: .userDomainMask)
                .first?
                .appendingPathComponent("translator-virtual-mic/models")
                .path ?? ""
            let modelDir = env["LOCAL_STT_MODEL_DIR"] ?? defaultModelDir
            let vadPath = env["LOCAL_STT_VAD_MODEL_PATH"] ?? (modelDir as NSString).appendingPathComponent("silero_vad.onnx")
            let vadThreshold = env["LOCAL_STT_VAD_THRESHOLD"] ?? "0.5"
            let sttLanguage = env["LOCAL_STT_LANGUAGE"] ?? "zh"
            let mtEnabled = env["MT_ENABLED"] ?? "false"
            let mtEndpoint = env["MT_ENDPOINT"] ?? "https://api.openai.com/v1/chat/completions"
            let mtApiKey = env["MT_API_KEY"] ?? ""
            let mtApiKeyEnv = env["MT_API_KEY_ENV"] ?? "OPENAI_API_KEY"
            let mtModel = env["MT_MODEL"] ?? "gpt-4o-mini"
            let mtTarget = targetLanguage

            // Local MT config
            let localMtEnabledStr = localMtEnabled ? "true" : "false"
            let localMtModelId = env["LOCAL_MT_MODEL_ID"] ?? selectedLocalMtModelId
            let localMtModelDir = env["LOCAL_MT_MODEL_DIR"] ?? modelDir
            let localMtSourceLang = env["LOCAL_MT_SOURCE_LANG"] ?? "zh"

            // TTS config
            let ttsEnabledStr = (ttsEnabled && isTtsModelDownloaded(selectedTtsModelId)) ? "true" : "false"
            let ttsModelDir = FileManager.default
                .urls(for: .applicationSupportDirectory, in: .userDomainMask)
                .first?
                .appendingPathComponent("translator-virtual-mic/models/tts")
                .path ?? ""
            let ttsSpeedStr = String(format: "%.2f", ttsSpeed)

            // Append local_stt, mt, local_mt, and tts keys by slicing before the final "}"
            let extra = #","local_stt_enabled":true,"local_stt_model_id":"\#(modelId)","local_stt_model_dir":"\#(modelDir)","local_stt_vad_model_path":"\#(vadPath)","local_stt_vad_threshold":\#(vadThreshold),"local_stt_language":"\#(sttLanguage)","mt_enabled":\#(mtEnabled),"mt_endpoint":"\#(mtEndpoint)","mt_api_key":"\#(mtApiKey)","mt_api_key_env":"\#(mtApiKeyEnv)","mt_model":"\#(mtModel)","mt_target_language":"\#(mtTarget)","local_mt_enabled":\#(localMtEnabledStr),"local_mt_model_id":"\#(localMtModelId)","local_mt_model_dir":"\#(localMtModelDir)","local_mt_source_lang":"\#(localMtSourceLang)","tts_enabled":\#(ttsEnabledStr),"tts_model_id":"\#(selectedTtsModelId)","tts_model_dir":"\#(ttsModelDir)","tts_speaker_id":0,"tts_speed":\#(ttsSpeedStr)}"#
            if let idx = base.lastIndex(of: "}") {
                base.replaceSubrange(idx...base.index(before: base.endIndex), with: extra)
            }
        }

        return base
    }

    private func startTranslationService(using engine: EngineBox) {
        switch selectedTranslationProvider {
        case .none, .localCaption:
            return
        case .azureVoiceLive:
            startAzureVoiceLive(using: engine)
        case .openAIRealtime:
            startOpenAIRealtime(using: engine)
        case .elevenLabs:
            startElevenLabs(using: engine)
        }
    }

    private func startAzureVoiceLive(using engine: EngineBox) {
        let endpoint = ProcessInfo.processInfo.environment["AZURE_VOICELIVE_ENDPOINT"] ?? ""
        let apiKey = ProcessInfo.processInfo.environment["AZURE_VOICELIVE_API_KEY"] ?? ""
        guard !endpoint.isEmpty else {
            appendLog("Azure Voice Live disabled: AZURE_VOICELIVE_ENDPOINT is missing")
            return
        }
        guard !apiKey.isEmpty else {
            appendLog("Azure Voice Live disabled: AZURE_VOICELIVE_API_KEY is missing")
            return
        }

        azureVoiceLiveService.start(engine: engine, endpoint: endpoint, apiKey: apiKey) { [weak self] message in
            Task { @MainActor in
                self?.appendLog(message)
                self?.translationStateJSON = engine.translationStateJSON()
            }
        }
        appendLog("Azure Voice Live started")
    }

    private func startOpenAIRealtime(using engine: EngineBox) {
        let endpoint = ProcessInfo.processInfo.environment["OPENAI_REALTIME_ENDPOINT"] ?? "wss://api.openai.com/v1/realtime"
        let model = ProcessInfo.processInfo.environment["OPENAI_REALTIME_MODEL"] ?? "gpt-realtime"
        let apiKey = ProcessInfo.processInfo.environment["OPENAI_API_KEY"] ?? ""
        guard !apiKey.isEmpty else {
            appendLog("OpenAI Realtime disabled: OPENAI_API_KEY is missing")
            return
        }

        openAIRealtimeService.start(engine: engine, endpoint: endpoint, model: model, apiKey: apiKey) { [weak self] message in
            Task { @MainActor in
                self?.appendLog(message)
                self?.translationStateJSON = engine.translationStateJSON()
            }
        }
        appendLog("OpenAI Realtime started")
    }

    private func startElevenLabs(using engine: EngineBox) {
        let env = ProcessInfo.processInfo.environment
        guard !env["ELEVENLABS_API_KEY", default: ""].isEmpty else {
            appendLog("ElevenLabs disabled: ELEVENLABS_API_KEY is missing (used for Scribe STT and TTS)")
            return
        }
        guard !env["ELEVENLABS_VOICE_ID", default: ""].isEmpty else {
            appendLog("ElevenLabs disabled: ELEVENLABS_VOICE_ID is missing")
            return
        }
        let mtKeyEnv = env["MT_API_KEY_ENV"] ?? "OPENAI_API_KEY"
        guard !env[mtKeyEnv, default: ""].isEmpty else {
            appendLog("ElevenLabs disabled: MT API key env var '\(mtKeyEnv)' is missing")
            return
        }

        let targetLocale: String = switch targetLanguage {
        case "zh": "zh-CN"
        case "ja": "ja-JP"
        default: "en-US"
        }

        elevenLabsPipelineService.configure(engine: engine, targetLocale: targetLocale)
        elevenLabsPipelineService.reset()
        appendLog("ElevenLabs pipeline started (target=\(targetLocale))")
    }

    private func startSharedBufferMonitor() {
        stopSharedBufferMonitor()
        sharedBufferMonitorTask = Task { [weak self] in
            while !Task.isCancelled {
                await MainActor.run {
                    self?.refreshSharedBufferStatus(logOnChange: false)
                }
                try? await Task.sleep(for: .milliseconds(500))
            }
        }
    }

    private func stopSharedBufferMonitor() {
        sharedBufferMonitorTask?.cancel()
        sharedBufferMonitorTask = nil
    }

    private func refreshSharedBufferStatus(logOnChange: Bool) {
        let snapshot = Self.readSharedBufferSnapshot(path: sharedOutputPath)
        sharedBufferStatusText = Self.describeSharedBufferSnapshot(snapshot)

        guard snapshot.fileExists != lastSharedBufferSnapshot.fileExists ||
                snapshot.writeIndexFrames != lastSharedBufferSnapshot.writeIndexFrames ||
                snapshot.readIndexFrames != lastSharedBufferSnapshot.readIndexFrames ||
                snapshot.lastTimestampNs != lastSharedBufferSnapshot.lastTimestampNs
        else {
            return
        }

        if logOnChange || snapshot.fileExists || lastSharedBufferSnapshot.fileExists {
            appendLog("Shared buffer: \(sharedBufferStatusText)")
        }
        lastSharedBufferSnapshot = snapshot
    }

    private static func readSharedBufferSnapshot(path: String) -> SharedBufferMonitorSnapshot {
        guard !path.isEmpty else {
            return .missing
        }
        let url = URL(fileURLWithPath: path)
        guard let data = try? Data(contentsOf: url), data.count >= 48 else {
            return .missing
        }

        return SharedBufferMonitorSnapshot(
            fileExists: true,
            sampleRate: readUInt32LE(from: data, offset: 12),
            channelCount: readUInt32LE(from: data, offset: 8),
            capacityFrames: readUInt32LE(from: data, offset: 16),
            writeIndexFrames: readUInt64LE(from: data, offset: 24),
            readIndexFrames: readUInt64LE(from: data, offset: 32),
            lastTimestampNs: readUInt64LE(from: data, offset: 40)
        )
    }

    private static func describeSharedBufferSnapshot(_ snapshot: SharedBufferMonitorSnapshot) -> String {
        guard snapshot.fileExists else {
            return "Shared buffer missing"
        }

        let unreadFrames = snapshot.writeIndexFrames >= snapshot.readIndexFrames
            ? snapshot.writeIndexFrames - snapshot.readIndexFrames
            : 0
        return "Shared buffer sr=\(snapshot.sampleRate) ch=\(snapshot.channelCount) cap=\(snapshot.capacityFrames) write=\(snapshot.writeIndexFrames) read=\(snapshot.readIndexFrames) unread=\(unreadFrames) ts=\(snapshot.lastTimestampNs)"
    }

    private static func readUInt32LE(from data: Data, offset: Int) -> UInt32 {
        let range = offset..<(offset + 4)
        return data[range].withUnsafeBytes { rawBuffer in
            UInt32(littleEndian: rawBuffer.load(as: UInt32.self))
        }
    }

    private static func readUInt64LE(from data: Data, offset: Int) -> UInt64 {
        let range = offset..<(offset + 8)
        return data[range].withUnsafeBytes { rawBuffer in
            UInt64(littleEndian: rawBuffer.load(as: UInt64.self))
        }
    }

    // MARK: - Local STT Model Management

    func isModelDownloaded(_ modelId: String) -> Bool {
        guard let model = ModelRegistry.model(for: modelId) else { return false }
        return modelDownloadService.isModelDownloaded(model)
    }

    func isVADModelDownloaded() -> Bool {
        return modelDownloadService.isModelDownloaded(ModelRegistry.vadModel)
    }

    func downloadModel(_ modelId: String) {
        guard let model = ModelRegistry.model(for: modelId) else { return }
        localSttModelDownloadState = .idle
        downloadCancellable = modelDownloadService.$state
            .receive(on: DispatchQueue.main)
            .sink { [weak self] state in
                self?.localSttModelDownloadState = state
                if case .completed = state {
                    self?.appendLog("Model \(modelId) downloaded")
                } else if case .failed(let msg) = state {
                    self?.appendLog("Model download failed: \(msg)")
                }
            }
        modelDownloadService.startDownload(model)
    }

    func downloadVADModel() {
        downloadModel(ModelRegistry.vadModel.id)
    }

    func cancelModelDownload() {
        modelDownloadService.cancel()
        downloadCancellable?.cancel()
        downloadCancellable = nil
        localSttModelDownloadState = .idle
    }

    func deleteModel(_ modelId: String) {
        guard let model = ModelRegistry.model(for: modelId) else { return }
        modelDownloadService.deleteModel(model)
        appendLog("Deleted model \(modelId)")
    }

    // MARK: - Local MT Model Management

    func isMtModelDownloaded(_ modelId: String) -> Bool {
        guard let model = MtModelRegistry.model(for: modelId) else { return false }
        return modelDownloadService.isMtModelDownloaded(model)
    }

    func downloadMtModel(_ modelId: String) {
        guard let model = MtModelRegistry.model(for: modelId) else { return }
        localMtModelDownloadState = .idle
        mtDownloadCancellable = modelDownloadService.$mtState
            .receive(on: DispatchQueue.main)
            .sink { [weak self] state in
                self?.localMtModelDownloadState = state
                if case .completed = state {
                    self?.appendLog("MT model \(modelId) downloaded")
                } else if case .failed(let msg) = state {
                    self?.appendLog("MT model download failed: \(msg)")
                }
            }
        modelDownloadService.startMtDownload(model)
    }

    func cancelMtModelDownload() {
        modelDownloadService.cancelMtDownload()
        mtDownloadCancellable?.cancel()
        mtDownloadCancellable = nil
        localMtModelDownloadState = .idle
    }

    func deleteMtModel(_ modelId: String) {
        guard let model = MtModelRegistry.model(for: modelId) else { return }
        modelDownloadService.deleteMtModel(model)
        appendLog("Deleted MT model \(modelId)")
    }

    // MARK: - Local TTS Model Management

    func isTtsModelDownloaded(_ modelId: String) -> Bool {
        guard let model = TtsModelRegistry.model(for: modelId) else { return false }
        return modelDownloadService.isTtsModelDownloaded(model)
    }

    func downloadTtsModel(_ modelId: String) {
        guard let model = TtsModelRegistry.model(for: modelId) else { return }
        ttsModelDownloadState = .idle
        ttsDownloadCancellable = modelDownloadService.$ttsState
            .receive(on: DispatchQueue.main)
            .sink { [weak self] state in
                self?.ttsModelDownloadState = state
                if case .completed = state {
                    self?.appendLog("TTS model \(modelId) downloaded")
                } else if case .failed(let msg) = state {
                    self?.appendLog("TTS model download failed: \(msg)")
                }
            }
        modelDownloadService.startTtsDownload(model)
    }

    func cancelTtsModelDownload() {
        modelDownloadService.cancelTtsDownload()
        ttsDownloadCancellable?.cancel()
        ttsDownloadCancellable = nil
        ttsModelDownloadState = .idle
    }

    func deleteTtsModel(_ modelId: String) {
        guard let model = TtsModelRegistry.model(for: modelId) else { return }
        modelDownloadService.deleteTtsModel(model)
        appendLog("Deleted TTS model \(modelId)")
    }

    private func appendLog(_ message: String) {
        fputs("[TranslatorVirtualMicHost] \(message)\n", stderr)
        logLines.append(message)
        if logLines.count > 200 {
            logLines.removeFirst(logLines.count - 200)
        }
    }
}
