import Foundation

struct TtsModelInfo: Identifiable {
    let id: String
    let description: String
    let sizeDisplay: String
    let tarballUrl: String
    let tarballSizeBytes: Int64
}

enum TtsModelRegistry {
    static let allModels: [TtsModelInfo] = [
        TtsModelInfo(
            id: "kokoro-en-v0_19",
            description: "Kokoro English TTS v0.19 — 11 speakers, high-quality neural TTS.",
            sizeDisplay: "~336 MB",
            tarballUrl: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-en-v0_19.tar.bz2",
            tarballSizeBytes: 336_000_000
        ),
    ]

    static func model(for id: String) -> TtsModelInfo? {
        allModels.first { $0.id == id }
    }
}
