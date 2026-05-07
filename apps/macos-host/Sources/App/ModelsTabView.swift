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

struct ModelsTabView: View {
    @EnvironmentObject private var viewModel: AppViewModel
    @State private var sttExpanded = true
    @State private var mtExpanded = true
    @State private var ttsExpanded = true

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                // MARK: - Local STT
                DisclosureGroup(isExpanded: $sttExpanded) {
                    VStack(alignment: .leading, spacing: 12) {
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

                        // VAD model status
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
                    .padding(.top, 8)
                } label: {
                    Label("Local STT", systemImage: "waveform")
                        .font(.headline)
                }

                // MARK: - Local MT
                if viewModel.selectedTranslationProvider == .localCaption {
                    DisclosureGroup(isExpanded: $mtExpanded) {
                        VStack(alignment: .leading, spacing: 12) {
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
                        .padding(.top, 8)
                    } label: {
                        Label("Local MT", systemImage: "arrow.left.arrow.right")
                            .font(.headline)
                    }
                }

                // MARK: - Local TTS
                if viewModel.ttsModeSelection == .local {
                    DisclosureGroup(isExpanded: $ttsExpanded) {
                        VStack(alignment: .leading, spacing: 12) {
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
                        .padding(.top, 8)
                    } label: {
                        Label("Local TTS", systemImage: "speaker.wave.1")
                            .font(.headline)
                    }
                }

                Spacer(minLength: 20)
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
