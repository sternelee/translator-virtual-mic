import Foundation

@MainActor
final class AzureVoiceLiveService: NSObject {
    private var webSocketTask: URLSessionWebSocketTask?
    private var urlSession: URLSession?
    private var isRunning = false
    private var sendLoopTask: Task<Void, Never>?
    private var receiveLoopTask: Task<Void, Never>?
    private weak var engine: EngineBox?
    private var onLog: ((String) -> Void)?

    func start(engine: EngineBox, endpoint: String, apiKey: String, onLog: @escaping (String) -> Void) {
        stop()
        self.engine = engine
        self.onLog = onLog

        guard let url = URL(string: endpoint) else {
            onLog("Azure Voice Live invalid endpoint: \(endpoint)")
            return
        }

        var request = URLRequest(url: url)
        request.setValue(apiKey, forHTTPHeaderField: "api-key")
        request.setValue("translator-virtual-mic", forHTTPHeaderField: "x-ms-client-request-id")

        let session = URLSession(configuration: .default, delegate: nil, delegateQueue: nil)
        let task = session.webSocketTask(with: request)
        self.urlSession = session
        self.webSocketTask = task
        self.isRunning = true
        task.resume()

        onLog("Azure Voice Live connecting: \(endpoint)")

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
                    onLog?("Azure Voice Live send failed: \(error)")
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
                        onLog?("Azure Voice Live ingest failed: \(engine.lastError())")
                    }
                case .data(let data):
                    if let text = String(data: data, encoding: .utf8) {
                        let frames = engine.ingestTranslationEvent(text)
                        if frames < 0 {
                            onLog?("Azure Voice Live ingest failed: \(engine.lastError())")
                        }
                    }
                @unknown default:
                    onLog?("Azure Voice Live received unknown WebSocket message")
                }
            } catch {
                onLog?("Azure Voice Live receive failed: \(error)")
                break
            }
        }
    }
}
