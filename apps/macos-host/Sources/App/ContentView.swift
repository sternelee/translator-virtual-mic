import SwiftUI

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
                    Button("Refresh Devices") {
                        viewModel.refreshDevices()
                    }
                    HStack {
                        Button("Start") { viewModel.startEngine() }
                        Button("Stop") { viewModel.stopEngine() }
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

                Section("Input Devices") {
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
                }
            }
            .navigationTitle("Translator Virtual Mic")
        } detail: {
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
                if !viewModel.currentCaption.isEmpty {
                    Text(viewModel.currentCaption)
                        .font(.title2)
                        .textSelection(.enabled)
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
    }
}
