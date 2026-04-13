import Foundation

@MainActor
final class OpenAIRealtimeService: NSObject {
    private var webSocketTask: URLSessionWebSocketTask?
    private var urlSession: URLSession?
    private var isRunning = false
    private var sendLoopTask: Task<Void, Never>?
    private var receiveLoopTask: Task<Void, Never>?
    private weak var engine: EngineBox?
    private var onLog: ((String) -> Void)?

    func start(engine: EngineBox, endpoint: String, model: String, apiKey: String, onLog: @escaping (String) -> Void) {
        stop()
        self.engine = engine
        self.onLog = onLog

        guard let encodedModel = model.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) else {
            onLog("OpenAI Realtime invalid endpoint: \(endpoint)")
            return
        }
        let separator = endpoint.contains("?") ? "&" : "?"
        guard let url = URL(string: "\(endpoint)\(separator)model=\(encodedModel)") else {
            onLog("OpenAI Realtime invalid endpoint: \(endpoint)")
            return
        }

        var request = URLRequest(url: url)
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")

        let session = URLSession(configuration: .default, delegate: nil, delegateQueue: nil)
        let task = session.webSocketTask(with: request)
        self.urlSession = session
        self.webSocketTask = task
        self.isRunning = true
        task.resume()

        onLog("OpenAI Realtime connecting: \(url.absoluteString)")

        sendLoopTask = Task { [weak self] in
            await self?.sendLoop()
        }
        receiveLoopTask = Task { [weak self] in
            await self?.receiveLoop()
        }
    }

    func stop() {
        isRunning = false
        sendLoopTask?.cancel()
        receiveLoopTask?.cancel()
        sendLoopTask = nil
        receiveLoopTask = nil
        webSocketTask?.cancel(with: .goingAway, reason: nil)
        webSocketTask = nil
        urlSession?.invalidateAndCancel()
        urlSession = nil
        engine = nil
        onLog = nil
    }

    private func sendLoop() async {
        while isRunning, !Task.isCancelled {
            guard let engine else { return }
            if let event = engine.takeNextTranslationEvent() {
                do {
                    try await webSocketTask?.send(.string(event))
                } catch {
                    onLog?("OpenAI Realtime send failed: \(error)")
                    break
                }
            } else {
                try? await Task.sleep(for: .milliseconds(20))
            }
        }
    }

    private func receiveLoop() async {
        while isRunning, !Task.isCancelled {
            guard let task = webSocketTask, let engine else { return }
            do {
                let message = try await task.receive()
                switch message {
                case .string(let text):
                    let frames = engine.ingestTranslationEvent(text)
                    if frames < 0 {
                        onLog?("OpenAI Realtime ingest failed: \(engine.lastError())")
                    }
                case .data(let data):
                    if let text = String(data: data, encoding: .utf8) {
                        let frames = engine.ingestTranslationEvent(text)
                        if frames < 0 {
                            onLog?("OpenAI Realtime ingest failed: \(engine.lastError())")
                        }
                    }
                @unknown default:
                    onLog?("OpenAI Realtime received unknown WebSocket message")
                }
            } catch {
                onLog?("OpenAI Realtime receive failed: \(error)")
                break
            }
        }
    }
}
