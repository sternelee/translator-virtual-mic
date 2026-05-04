import Foundation

/// Polls the Rust engine for caption events produced by the local-STT pipeline.
/// Uses a dedicated background queue with a short (5 ms) sleep loop so that
/// events appear on the SwiftUI side with minimal latency, without burning
/// CPU on empty polls thanks to the cheap `hasPendingCaptionEvents` FFI check.
final class CaptionService: ObservableObject {
    @Published var currentCaption: String = ""
    @Published var captionStateJSON: String = "{}"

    private weak var engine: EngineBox?
    private var pollTask: Task<Void, Never>?

    func start(engine: EngineBox) {
        stop()
        self.engine = engine
        pollTask = Task.detached(priority: .userInitiated) { [weak self] in
            while !Task.isCancelled {
                guard let self, let engine = self.engine else { break }

                // Cheap FFI check: no mutex or allocation on the Rust side.
                if engine.hasPendingCaptionEvents() {
                    while let eventJSON = engine.takeNextCaptionEvent(),
                          !Task.isCancelled {
                        let text = Self.extractText(from: eventJSON) ?? eventJSON
                        let state = engine.captionStateJSON()
                        await MainActor.run {
                            self.currentCaption = text
                            self.captionStateJSON = state
                        }
                        fputs("[Caption] \(text)\n", stderr)
                    }
                }

                // 5 ms sleep gives ~200 Hz polling: low enough to avoid
                // spinning, fast enough to keep caption latency < 10 ms.
                try? await Task.sleep(nanoseconds: 5_000_000)
            }
        }
    }

    func stop() {
        pollTask?.cancel()
        pollTask = nil
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
