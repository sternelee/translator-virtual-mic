import SwiftUI
import Translation

struct DebugTabView: View {
    @EnvironmentObject private var viewModel: AppViewModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                // MARK: - Caption Display
                GroupBox("Caption") {
                    VStack(alignment: .leading, spacing: 8) {
                        if viewModel.selectedTranslationProvider == .localCaption && viewModel.appleTranslationEnabled {
                            if #available(macOS 15.0, *) {
                                AppleTranslationView(viewModel: viewModel)
                            } else {
                                Text("Apple Translation requires macOS 15+")
                                    .font(.caption)
                                    .foregroundStyle(.red)
                            }
                        } else {
                            if !viewModel.currentCaption.isEmpty {
                                Text(viewModel.currentCaption)
                                    .font(.title2)
                                    .textSelection(.enabled)
                            } else {
                                Text("No caption yet")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                    .padding(8)
                }

                // MARK: - Shared Buffer
                GroupBox("Shared Buffer") {
                    VStack(alignment: .leading, spacing: 4) {
                        if !viewModel.sharedOutputPath.isEmpty {
                            Text(viewModel.sharedOutputPath)
                                .font(.system(.footnote, design: .monospaced))
                                .textSelection(.enabled)
                        }
                        Text(viewModel.sharedBufferStatusText)
                            .font(.system(.footnote, design: .monospaced))
                            .textSelection(.enabled)
                    }
                    .padding(8)
                }

                // MARK: - Metrics
                GroupBox("Metrics JSON") {
                    Text(viewModel.metricsJSON)
                        .font(.system(.footnote, design: .monospaced))
                        .textSelection(.enabled)
                        .padding(8)
                }

                // MARK: - Translation State
                GroupBox("Translation State JSON") {
                    Text(viewModel.translationStateJSON)
                        .font(.system(.footnote, design: .monospaced))
                        .textSelection(.enabled)
                        .padding(8)
                }

                // MARK: - Caption State
                GroupBox("Caption State JSON") {
                    Text(viewModel.captionStateJSON)
                        .font(.system(.footnote, design: .monospaced))
                        .textSelection(.enabled)
                        .padding(8)
                }

                // MARK: - Logs
                GroupBox("Logs (last 200 lines)") {
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 4) {
                            ForEach(Array(viewModel.logLines.enumerated()), id: \.offset) { _, line in
                                Text(line)
                                    .font(.system(.caption, design: .monospaced))
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .textSelection(.enabled)
                            }
                        }
                        .padding(8)
                    }
                    .frame(minHeight: 200, maxHeight: 400)
                }

                Spacer(minLength: 20)
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
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
