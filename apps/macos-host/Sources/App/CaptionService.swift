import Foundation
import Combine

/// Polls the Rust engine for caption events produced by the local-STT pipeline.
final class CaptionService: ObservableObject {
    @Published var currentCaption: String = ""
    @Published var captionStateJSON: String = "{}"

    private weak var engine: EngineBox?
    private var cancellable: AnyCancellable?

    func start(engine: EngineBox) {
        stop()
        self.engine = engine
        cancellable = Timer.publish(every: 0.05, on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                guard let self, let engine = self.engine else { return }
                if let eventJSON = engine.takeNextCaptionEvent() {
                    let text = Self.extractText(from: eventJSON) ?? eventJSON
                    self.currentCaption = text
                    self.captionStateJSON = engine.captionStateJSON()
                    fputs("[Caption] \(text)\n", stderr)
                }
            }
    }

    func stop() {
        cancellable?.cancel()
        cancellable = nil
        engine = nil
        currentCaption = ""
        captionStateJSON = "{}"
    }

    private static func extractText(from json: String) -> String? {
        guard let data = json.data(using: .utf8),
              let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }
        // Prefer translation if present; fall back to original text.
        if let translation = dict["translation"] as? String, !translation.isEmpty {
            return translation
        }
        return dict["text"] as? String
    }
}
