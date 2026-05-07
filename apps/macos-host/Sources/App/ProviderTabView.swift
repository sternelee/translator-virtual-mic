import SwiftUI

struct ProviderTabView: View {
    @EnvironmentObject private var viewModel: AppViewModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                // MARK: - Provider Cards
                ForEach(TranslationServiceProvider.allCases) { provider in
                    ProviderCard(
                        provider: provider,
                        isSelected: viewModel.selectedTranslationProvider == provider,
                        isConfigured: isProviderConfigured(provider)
                    )
                    .onTapGesture {
                        withAnimation(.easeInOut(duration: 0.15)) {
                            viewModel.selectedTranslationProvider = provider
                            if provider == .localCaption {
                                viewModel.localMtEnabled = true
                                viewModel.ttsEnabled = true
                            }
                        }
                    }
                }

                // MARK: - Target Language
                if viewModel.selectedTranslationProvider.needsTargetLanguage {
                    GroupBox("Target Language") {
                        Picker("Language", selection: $viewModel.targetLanguage) {
                            Text("English").tag("en")
                            Text("Chinese").tag("zh")
                            Text("Japanese").tag("ja")
                        }
                        .pickerStyle(.segmented)
                        .padding(8)
                    }
                }

                Spacer(minLength: 20)
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func isProviderConfigured(_ provider: TranslationServiceProvider) -> Bool {
        let env = ProcessInfo.processInfo.environment
        switch provider {
        case .none:
            return true
        case .openAIRealtime:
            return !(env["OPENAI_API_KEY"] ?? "").isEmpty
        case .azureVoiceLive:
            return !(env["AZURE_VOICELIVE_API_KEY"] ?? "").isEmpty
        case .elevenLabs:
            let hasKey = !(env["ELEVENLABS_API_KEY"] ?? "").isEmpty
            let hasVoice = !(env["ELEVENLABS_VOICE_ID"] ?? "").isEmpty
            let mtKeyEnv = env["MT_API_KEY_ENV"] ?? "OPENAI_API_KEY"
            let hasMtKey = !(env[mtKeyEnv] ?? "").isEmpty
            return hasKey && hasVoice && hasMtKey
        case .localCaption:
            return viewModel.isModelDownloaded(viewModel.selectedLocalSttModelId)
                && viewModel.isVADModelDownloaded()
        }
    }
}

// MARK: - Provider Card

private struct ProviderCard: View {
    let provider: TranslationServiceProvider
    let isSelected: Bool
    let isConfigured: Bool

    var body: some View {
        HStack(spacing: 12) {
            Text(providerIcon)
                .font(.title2)

            VStack(alignment: .leading, spacing: 4) {
                Text(provider.displayName)
                    .font(.headline)
                Text(providerDescription)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                HStack(spacing: 4) {
                    Circle()
                        .fill(isConfigured ? Color.green : Color.red)
                        .frame(width: 6, height: 6)
                    Text(isConfigured ? "Configured" : providerRequirement)
                        .font(.caption2)
                        .foregroundStyle(isConfigured ? .green : .red)
                }
            }

            Spacer()

            if isSelected {
                Image(systemName: "checkmark.circle.fill")
                    .foregroundStyle(Color.accentColor)
                    .font(.title3)
            }
        }
        .padding()
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(isSelected ? Color.accentColor.opacity(0.08) : Color.secondary.opacity(0.05))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(isSelected ? Color.accentColor : Color.clear, lineWidth: 2)
        )
        .contentShape(Rectangle())
    }

    private var providerIcon: String {
        switch provider {
        case .none: return "🔇"
        case .openAIRealtime: return "🔄"
        case .azureVoiceLive: return "☁️"
        case .elevenLabs: return "🔊"
        case .localCaption: return "📝"
        }
    }

    private var providerDescription: String {
        switch provider {
        case .none:
            return "No translation, raw mic to virtual device"
        case .openAIRealtime:
            return "Real-time speech-to-speech via OpenAI API"
        case .azureVoiceLive:
            return "Azure speech-to-speech translation"
        case .elevenLabs:
            return "ElevenLabs pipeline: Scribe STT + MT + TTS"
        case .localCaption:
            return "VAD → STT → MT → TTS (fully local). Works offline."
        }
    }

    private var providerRequirement: String {
        switch provider {
        case .none:
            return "No requirements"
        case .openAIRealtime:
            return "Requires: OPENAI_API_KEY"
        case .azureVoiceLive:
            return "Requires: AZURE_VOICELIVE_API_KEY"
        case .elevenLabs:
            return "Requires: ELEVENLABS_API_KEY, VOICE_ID, MT key"
        case .localCaption:
            return "Models needed"
        }
    }
}

// MARK: - TranslationServiceProvider Helpers

private extension TranslationServiceProvider {
    var needsTargetLanguage: Bool {
        switch self {
        case .none: return false
        default: return true
        }
    }
}
