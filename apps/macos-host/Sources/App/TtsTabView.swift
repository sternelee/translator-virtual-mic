import SwiftUI

struct TtsTabView: View {
    @EnvironmentObject private var viewModel: AppViewModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                // MARK: - TTS Mode
                GroupBox("TTS Mode") {
                    Picker("Mode", selection: $viewModel.ttsModeSelection) {
                        ForEach(TtsModeSelection.allCases) { mode in
                            Text(mode.displayName).tag(mode)
                        }
                    }
                    .pickerStyle(.radioGroup)
                    .padding(8)
                }

                // MARK: - Local TTS
                if viewModel.ttsModeSelection == .local {
                    GroupBox("Local TTS Configuration") {
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
                        .padding(8)
                    }
                }

                // MARK: - Kokoro CoreML TTS
                if viewModel.ttsModeSelection == .coreml {
                    GroupBox("Kokoro CoreML Configuration") {
                        VStack(alignment: .leading, spacing: 12) {
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
                        .padding(8)
                    }
                }

                // MARK: - ElevenLabs TTS
                if viewModel.ttsModeSelection == .elevenlabs {
                    GroupBox("ElevenLabs Configuration") {
                        VStack(alignment: .leading, spacing: 12) {
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
                        .padding(8)
                    }
                }

                // MARK: - MiniMax TTS
                if viewModel.ttsModeSelection == .minimax {
                    GroupBox("MiniMax Configuration") {
                        VStack(alignment: .leading, spacing: 12) {
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
                        .padding(8)
                    }
                }

                // MARK: - Sidecar TTS
                if viewModel.ttsModeSelection == .sidecar {
                    GroupBox("Sidecar (Voicebox) Configuration") {
                        VStack(alignment: .leading, spacing: 12) {
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
                        .padding(8)
                    }
                }

                Spacer(minLength: 20)
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
