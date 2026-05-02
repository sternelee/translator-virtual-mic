import Foundation

struct TtsModelFile {
    let relativePath: String
    let url: String
    let sizeBytes: Int64
}

struct TtsModelInfo: Identifiable {
    let id: String
    let description: String
    let sizeDisplay: String
    let totalSizeBytes: Int64
    let files: [TtsModelFile]
}

enum TtsModelRegistry {
    static let allModels: [TtsModelInfo] = [
        TtsModelInfo(
            id: "kokoro-en-v0_19",
            description: "Kokoro English TTS v0.19. High-quality neural TTS, English output only.",
            sizeDisplay: "~310 MB",
            totalSizeBytes: 325_000_000,
            files: [
                TtsModelFile(
                    relativePath: "model.onnx",
                    url: "https://huggingface.co/csukuangfj/kokoro-en-v0_19/resolve/main/model.onnx",
                    sizeBytes: 290_000_000
                ),
                TtsModelFile(
                    relativePath: "voices.bin",
                    url: "https://huggingface.co/csukuangfj/kokoro-en-v0_19/resolve/main/voices.bin",
                    sizeBytes: 18_000_000
                ),
                TtsModelFile(
                    relativePath: "tokens.txt",
                    url: "https://huggingface.co/csukuangfj/kokoro-en-v0_19/resolve/main/tokens.txt",
                    sizeBytes: 5_000
                ),
            ]
        ),
    ]

    static func model(for id: String) -> TtsModelInfo? {
        allModels.first { $0.id == id }
    }
}
