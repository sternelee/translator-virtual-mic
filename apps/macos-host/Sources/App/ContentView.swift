import SwiftUI
import Translation

private func langDisplayName(_ code: String) -> String {
    switch code {
    case "zh": return "Chinese"
    case "en": return "English"
    case "ja": return "Japanese"
    case "*": return "Any"
    default: return code.uppercased()
    }
}

struct ContentView: View {
    @EnvironmentObject private var viewModel: AppViewModel

    var body: some View {
        NavigationSplitView {
            List {
                Section("Control") {
                    Text("Status: \(viewModel.statusText)")
                    HStack {
                        Text("Input Level")
                        ProgressView(value: Double(viewModel.inputLevel), total: 1.0)
                    }
                    Picker("Translation Provider", selection: $viewModel.selectedTranslationProvider) {
                        ForEach(TranslationServiceProvider.allCases) { provider in
                            Text(provider.displayName).tag(provider)
                        }
                    }
                    .onChange(of: viewModel.selectedTranslationProvider) { _, newValue in
                        if newValue == .localCaption {
                            viewModel.localMtEnabled = true
                            viewModel.ttsEnabled = true
                        }
                    }
                    Picker("Target Language", selection: $viewModel.targetLanguage) {
                        Text("English").tag("en")
                        Text("Chinese").tag("zh")
                        Text("Japanese").tag("ja")
                    }
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text("Input Gain")
                            Spacer()
                            Text(String(format: "%.1f dB", viewModel.inputGainDB))
                                .foregroundStyle(.secondary)
                        }
                        Slider(value: $viewModel.inputGainDB, in: -6...18, step: 0.5)
                    }
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text("Limiter Threshold")
                            Spacer()
                            Text(String(format: "%.1f dB", viewModel.limiterThresholdDB))
                                .foregroundStyle(.secondary)
                        }
                        Slider(value: $viewModel.limiterThresholdDB, in: -18 ... -1, step: 0.5)
                    }
                    Text("Input Devices")
                        .font(.subheadline.bold())

                    ForEach(viewModel.devices) { device in
                        HStack {
                            Text(device.name)
                            Spacer()
                            if viewModel.selectedDeviceID == device.id {
                                Text("Selected")
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .contentShape(Rectangle())
                        .onTapGesture {
                            viewModel.selectDevice(device)
                        }
                    }

                    Button("Refresh Devices") {
                        viewModel.refreshDevices()
                    }
                    HStack {
                        Button("Start") { viewModel.startEngine() }
                        Button("Stop") { viewModel.stopEngine() }
                    }

                    // Plugin installer
                    HStack(spacing: 8) {
                        Circle()
                            .fill(viewModel.pluginInstalled ? Color.green : Color.red)
                            .frame(width: 8, height: 8)
                        Text(viewModel.pluginInstalled ? "Virtual Mic Driver Installed" : "Virtual Mic Driver Missing")
                            .font(.caption)
                        Spacer()
                        if viewModel.pluginInstallInProgress {
                            ProgressView()
                                .controlSize(.small)
                        } else if viewModel.pluginInstalled {
                            Button("Uninstall") {
                                viewModel.uninstallPlugin()
                            }
                            .buttonStyle(.bordered)
                            .tint(.red)
                        } else {
                            Button("Install Driver") {
                                viewModel.installPlugin()
                            }
                            .buttonStyle(.borderedProminent)
                        }
                    }
                    if !viewModel.pluginInstallError.isEmpty {
                        Text(viewModel.pluginInstallError)
                            .font(.caption)
                            .foregroundStyle(.red)
                    }
                }

                if viewModel.selectedTranslationProvider == .localCaption {
                    Section("Local STT Model") {
                        Picker("Model", selection: $viewModel.selectedLocalSttModelId) {
                            ForEach(ModelRegistry.allModels) { model in
                                let downloaded = viewModel.isModelDownloaded(model.id)
                                Text("\(model.displayName) \(downloaded ? "✓" : "")").tag(model.id)
                            }
                        }
                        if let model = ModelRegistry.model(for: viewModel.selectedLocalSttModelId) {
                            Text(model.description)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Text("Size: \(model.sizeDisplay)")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            HStack {
                                if viewModel.isModelDownloaded(model.id) {
                                    Button("Delete") {
                                        viewModel.deleteModel(model.id)
                                    }
                                    .buttonStyle(.bordered)
                                    .tint(.red)
                                } else {
                                    Button("Download") {
                                        viewModel.downloadModel(model.id)
                                    }
                                    .buttonStyle(.borderedProminent)
                                    .disabled(viewModel.localSttModelDownloadState != .idle)
                                }
                                Spacer()
                            }
                            if case .downloading(let progress) = viewModel.localSttModelDownloadState,
                               progress.modelId == model.id {
                                VStack(alignment: .leading, spacing: 4) {
                                    ProgressView(value: Double(progress.downloadedBytes), total: Double(progress.totalBytes))
                                    Text("Downloading \(progress.fileName) (\(progress.fileIndex + 1)/\(progress.totalFiles))")
                                        .font(.caption2)
                                }
                            }
                        }
                        if !viewModel.isVADModelDownloaded() {
                            HStack {
                                Text("Silero VAD model missing")
                                    .font(.caption)
                                    .foregroundStyle(.red)
                                Spacer()
                                Button("Download VAD") {
                                    viewModel.downloadVADModel()
                                }
                                .buttonStyle(.borderedProminent)
                                .disabled(viewModel.localSttModelDownloadState != .idle)
                            }
                        } else {
                            Text("Silero VAD model ready")
                                .font(.caption)
                                .foregroundStyle(.green)
                        }
                    }
                }

                if viewModel.selectedTranslationProvider == .localCaption {
                    Section("Local MT Model") {
                        if #available(macOS 15.0, *) {
                            Toggle("Use Apple Translation (On-Device)", isOn: $viewModel.appleTranslationEnabled)
                        } else {
                            Text("Apple Translation requires macOS 15+")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }

                        if !viewModel.appleTranslationEnabled {
                            Picker("Model", selection: $viewModel.selectedLocalMtModelId) {
                                ForEach(MtModelRegistry.allModels) { model in
                                    let downloaded = viewModel.isMtModelDownloaded(model.id)
                                    Text("\(model.id) \(downloaded ? "✓" : "")").tag(model.id)
                                }
                            }
                            if let model = MtModelRegistry.model(for: viewModel.selectedLocalMtModelId) {
                                Text(model.description)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                Text("Direction: \(langDisplayName(model.srcLang)) → \(langDisplayName(model.tgtLang))")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                Text("Size: \(model.sizeDisplay)")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                HStack {
                                    if viewModel.isMtModelDownloaded(model.id) {
                                        Button("Delete") {
                                            viewModel.deleteMtModel(model.id)
                                        }
                                        .buttonStyle(.bordered)
                                        .tint(.red)
                                    } else {
                                        Button("Download") {
                                            viewModel.downloadMtModel(model.id)
                                        }
                                        .buttonStyle(.borderedProminent)
                                        .disabled(viewModel.localMtModelDownloadState != .idle)
                                    }
                                    Spacer()
                                }
                                if case .downloading(let progress) = viewModel.localMtModelDownloadState,
                                   progress.modelId == model.id {
                                    VStack(alignment: .leading, spacing: 4) {
                                        ProgressView(value: Double(progress.downloadedBytes), total: Double(progress.totalBytes))
                                        Text("Downloading \(progress.fileName) (\(progress.fileIndex + 1)/\(progress.totalFiles))")
                                            .font(.caption2)
                                    }
                                }
                            }

                            Toggle("Use local MT (replaces remote)", isOn: $viewModel.localMtEnabled)
                                .disabled(!viewModel.isMtModelDownloaded(viewModel.selectedLocalMtModelId))

                            if let model = MtModelRegistry.model(for: viewModel.selectedLocalMtModelId),
                               model.tgtLang != "*",
                               model.tgtLang != viewModel.targetLanguage {
                                Text("Warning: Model translates to \(langDisplayName(model.tgtLang)) but target language is \(langDisplayName(viewModel.targetLanguage))")
                                    .font(.caption)
                                    .foregroundStyle(.orange)
                            }
                        }
                    }
                }

                if viewModel.selectedTranslationProvider == .localCaption {
                    Section("TTS") {
                        Picker("TTS Mode", selection: $viewModel.ttsModeSelection) {
                            ForEach(TtsModeSelection.allCases) { mode in
                                Text(mode.displayName).tag(mode)
                            }
                        }

                        // ── Local TTS ──────────────────────────────────────
                        if viewModel.ttsModeSelection == .local {
                            Picker("Model", selection: $viewModel.selectedTtsModelId) {
                                ForEach(TtsModelRegistry.allModels) { model in
                                    let downloaded = viewModel.isTtsModelDownloaded(model.id)
                                    Text("\(model.id) \(downloaded ? "✓" : "")").tag(model.id)
                                }
                            }
                            if let model = TtsModelRegistry.model(for: viewModel.selectedTtsModelId) {
                                Text(model.description)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                Text("Size: \(model.sizeDisplay)")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                HStack {
                                    if viewModel.isTtsModelDownloaded(model.id) {
                                        Button("Delete") {
                                            viewModel.deleteTtsModel(model.id)
                                        }
                                        .buttonStyle(.bordered)
                                        .tint(.red)
                                    } else {
                                        Button("Download") {
                                            viewModel.downloadTtsModel(model.id)
                                        }
                                        .buttonStyle(.borderedProminent)
                                        .disabled(viewModel.ttsModelDownloadState != .idle)
                                    }
                                    Spacer()
                                }
                                if case .downloading(let progress) = viewModel.ttsModelDownloadState,
                                   progress.modelId == model.id {
                                    VStack(alignment: .leading, spacing: 4) {
                                        ProgressView(value: Double(progress.downloadedBytes), total: Double(progress.totalBytes))
                                        Text("Downloading \(progress.fileName) (\(progress.fileIndex + 1)/\(progress.totalFiles))")
                                            .font(.caption2)
                                    }
                                }
                            }
                            VStack(alignment: .leading, spacing: 4) {
                                HStack {
                                    Text("Speed")
                                    Spacer()
                                    Text(String(format: "%.1fx", viewModel.ttsSpeed))
                                        .foregroundStyle(.secondary)
                                }
                                Slider(value: $viewModel.ttsSpeed, in: 0.5...2.0, step: 0.1)
                            }
                            .disabled(!viewModel.isTtsModelDownloaded(viewModel.selectedTtsModelId))
                        }

                        // ── Kokoro CoreML TTS ──────────────────────────────
                        if viewModel.ttsModeSelection == .coreml {
                            VStack(alignment: .leading, spacing: 8) {
                                HStack {
                                    Text("Models directory")
                                        .font(.caption)
                                    Spacer()
                                    if viewModel.kokoroCoreMLEnabled {
                                        Label("Active", systemImage: "checkmark.circle.fill")
                                            .font(.caption)
                                            .foregroundStyle(.green)
                                    }
                                }
                                TextField(
                                    "~/Library/Application Support/translator-virtual-mic/models/kokoro-coreml",
                                    text: $viewModel.kokoroCoreMLModelDir
                                )
                                .font(.caption)
                                .textFieldStyle(.roundedBorder)

                                HStack {
                                    Text("Voice")
                                        .font(.caption)
                                    Picker("", selection: $viewModel.kokoroCoreMLVoiceId) {
                                        Text("af (American Female)").tag("af")
                                        Text("am (American Male)").tag("am")
                                        Text("bf (British Female)").tag("bf")
                                        Text("bm (British Male)").tag("bm")
                                    }
                                    .pickerStyle(.segmented)
                                    .font(.caption)
                                }

                                VStack(alignment: .leading, spacing: 4) {
                                    HStack {
                                        Text("Speed")
                                        Spacer()
                                        Text(String(format: "%.1fx", viewModel.kokoroCoreMLSpeed))
                                            .foregroundStyle(.secondary)
                                    }
                                    Slider(value: $viewModel.kokoroCoreMLSpeed, in: 0.5...2.0, step: 0.1)
                                }

                                Text("Requires kokoro-coreml CoreML models and Python tokenizer. See docs/apple-silicon-model-research.md")
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                        }

                        // ── ElevenLabs TTS ─────────────────────────────────
                        if viewModel.ttsModeSelection == .elevenlabs {
                            Text("Uses ELEVENLABS_API_KEY and ELEVENLABS_VOICE_ID from environment.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            let hasKey = ProcessInfo.processInfo.environment["ELEVENLABS_API_KEY"] != nil
                            let hasVoice = ProcessInfo.processInfo.environment["ELEVENLABS_VOICE_ID"] != nil
                            Label(
                                hasKey ? "API key found" : "ELEVENLABS_API_KEY not set",
                                systemImage: hasKey ? "checkmark.circle.fill" : "xmark.circle"
                            )
                            .font(.caption)
                            .foregroundStyle(hasKey ? .green : .red)
                            Label(
                                hasVoice ? "Voice ID found" : "ELEVENLABS_VOICE_ID not set",
                                systemImage: hasVoice ? "checkmark.circle.fill" : "xmark.circle"
                            )
                            .font(.caption)
                            .foregroundStyle(hasVoice ? .green : .red)
                        }

                        // ── MiniMax TTS ────────────────────────────────────
                        if viewModel.ttsModeSelection == .minimax {
                            Text("Uses MINIMAX_API_KEY and MINIMAX_VOICE_ID from environment.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            let hasKey = ProcessInfo.processInfo.environment["MINIMAX_API_KEY"] != nil
                            let hasVoice = ProcessInfo.processInfo.environment["MINIMAX_VOICE_ID"] != nil
                            Label(
                                hasKey ? "API key found" : "MINIMAX_API_KEY not set",
                                systemImage: hasKey ? "checkmark.circle.fill" : "xmark.circle"
                            )
                            .font(.caption)
                            .foregroundStyle(hasKey ? .green : .red)
                            Label(
                                hasVoice ? "Voice ID found" : "MINIMAX_VOICE_ID not set",
                                systemImage: hasVoice ? "checkmark.circle.fill" : "xmark.circle"
                            )
                            .font(.caption)
                            .foregroundStyle(hasVoice ? .green : .red)
                        }

                        // ── Sidecar TTS (Voicebox) ─────────────────────
                        if viewModel.ttsModeSelection == .sidecar {
                            Text("Requires Python tts_sidecar_server.py running on port 50001.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            VStack(alignment: .leading, spacing: 6) {
                                TextField("Engine", text: $viewModel.sidecarEngine)
                                    .font(.caption)
                                    .textFieldStyle(.roundedBorder)
                                TextField("Voice Name", text: $viewModel.sidecarVoiceName)
                                    .font(.caption)
                                    .textFieldStyle(.roundedBorder)
                            }
                            Text("Supports: kokoro, qwen_tts, chatterbox, luxtts, hume")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                            Text("Install: ./scripts/install_tts_sidecar_deps.sh (--mlx for Apple Silicon)")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
            .navigationTitle("Translator Virtual Mic")
        } detail: {
            DetailPanel(viewModel: viewModel)
        }
    }
}

// MARK: - Detail Panel

private struct DetailPanel: View {
    @ObservedObject var viewModel: AppViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(viewModel.microphonePermissionGranted ? "Microphone Access Ready" : "Microphone Access Pending")
                .font(.headline)
            Text("Logs")
                .font(.title3)
            if !viewModel.sharedOutputPath.isEmpty {
                Text(viewModel.sharedOutputPath)
                    .font(.system(.footnote, design: .monospaced))
                    .textSelection(.enabled)
            }
            Text(viewModel.sharedBufferStatusText)
                .font(.system(.footnote, design: .monospaced))
                .textSelection(.enabled)

            if viewModel.selectedTranslationProvider == .localCaption && viewModel.appleTranslationEnabled {
                appleTranslationSection
            } else {
                standardCaptionSection
            }

            Text(viewModel.captionStateJSON)
                .font(.system(.footnote, design: .monospaced))
                .textSelection(.enabled)
            Text(viewModel.translationStateJSON)
                .font(.system(.footnote, design: .monospaced))
                .textSelection(.enabled)
            Text(viewModel.metricsJSON)
                .font(.system(.footnote, design: .monospaced))
                .textSelection(.enabled)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    ForEach(Array(viewModel.logLines.enumerated()), id: \.offset) { _, line in
                        Text(line)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
        }
        .padding()
    }

    @ViewBuilder
    private var standardCaptionSection: some View {
        if !viewModel.currentCaption.isEmpty {
            Text(viewModel.currentCaption)
                .font(.title2)
                .textSelection(.enabled)
        }
    }

    @ViewBuilder
    private var appleTranslationSection: some View {
        if #available(macOS 15.0, *) {
            AppleTranslationView(viewModel: viewModel)
        } else {
            Text("Apple Translation requires macOS 15+")
                .font(.caption)
                .foregroundStyle(.red)
        }
    }
}

// MARK: - Apple Translation View (macOS 15+)

@available(macOS 15.0, *)
private struct AppleTranslationView: View {
    @ObservedObject var viewModel: AppViewModel
    @State private var config: TranslationSession.Configuration?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !viewModel.currentCaption.isEmpty {
                Text("Original")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(viewModel.currentCaption)
                    .font(.title3)
                    .textSelection(.enabled)

                if !viewModel.appleTranslatedCaption.isEmpty {
                    Divider()
                    Text("Translated")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(viewModel.appleTranslatedCaption)
                        .font(.title2)
                        .textSelection(.enabled)
                }
            }
        }
        .onAppear {
            let source = Locale.Language(identifier: appleSourceLang)
            let target = Locale.Language(identifier: appleTargetLang)
            config = TranslationSession.Configuration(source: source, target: target)
        }
        .onChange(of: viewModel.currentCaption) { _, newValue in
            viewModel.appleTranslationSource = newValue
            config?.invalidate()
        }
        .translationTask(config) { session in
            guard !viewModel.appleTranslationSource.isEmpty else { return }
            do {
                let response = try await session.translate(viewModel.appleTranslationSource)
                viewModel.appleTranslatedCaption = response.targetText
            } catch {
                viewModel.appleTranslatedCaption = viewModel.appleTranslationSource
            }
        }
    }

    private var appleSourceLang: String {
        switch viewModel.targetLanguage {
        case "zh": return "en-US"
        case "ja": return "en-US"
        default: return "zh-Hans"
        }
    }

    private var appleTargetLang: String {
        switch viewModel.targetLanguage {
        case "zh": return "zh-Hans"
        case "ja": return "ja-JP"
        default: return "en-US"
        }
    }
}
