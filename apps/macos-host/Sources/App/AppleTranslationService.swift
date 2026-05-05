import Foundation
import Translation

/// Bridge to Apple Translation framework (macOS 15+).
///
/// The Translation framework requires a SwiftUI view context, so the actual
/// `.translationTask` modifier lives in `ContentView`.  This class only
/// holds the mutable state (configuration + source text) that the view
/// reads.
@available(macOS 15.0, *)
final class AppleTranslationService: ObservableObject {
    @Published var configuration: TranslationSession.Configuration?
    @Published var sourceText: String = ""
    @Published var translatedText: String = ""
    @Published var isTranslating: Bool = false

    func setLanguages(source: Locale.Language, target: Locale.Language) {
        configuration = TranslationSession.Configuration(source: source, target: target)
    }

    func translate(_ text: String) {
        sourceText = text
        // Trigger re-translation by invalidating the current configuration.
        configuration?.invalidate()
    }

    func reset() {
        sourceText = ""
        translatedText = ""
        isTranslating = false
    }
}
