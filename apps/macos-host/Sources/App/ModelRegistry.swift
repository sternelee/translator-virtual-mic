import Foundation

struct ModelFile: Identifiable {
    let id = UUID()
    let relativePath: String
    let url: String
    let sizeBytes: Int64
}

struct SttModel: Identifiable {
    let id: String
    let displayName: String
    let description: String
    let sizeDisplay: String
    let totalSizeBytes: Int64
    let files: [ModelFile]
    let bestForLanguages: [String]
}

enum ModelRegistry {
    static let vadModel = SttModel(
        id: "silero-vad",
        displayName: "Silero VAD",
        description: "Voice Activity Detection model required by all local STT pipelines.",
        sizeDisplay: "1.1 MB",
        totalSizeBytes: 1_150_000,
        files: [
            ModelFile(
                relativePath: "silero_vad.onnx",
                url: "https://huggingface.co/csukuangfj/silero-vad/resolve/main/silero_vad.onnx",
                sizeBytes: 1_150_000
            )
        ],
        bestForLanguages: []
    )

    static let allModels: [SttModel] = [
        SttModel(
            id: "paraformer-zh",
            displayName: "Paraformer Chinese",
            description: "Alibaba Paraformer, non-autoregressive, RTF<0.07. Chinese-only, fast.",
            sizeDisplay: "217 MB",
            totalSizeBytes: 227_405_559,
            files: [
                ModelFile(relativePath: "model.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-zh-2024-03-09/resolve/main/model.int8.onnx", sizeBytes: 227_330_205),
                ModelFile(relativePath: "tokens.txt", url: "https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-zh-2024-03-09/resolve/main/tokens.txt", sizeBytes: 75_354),
            ],
            bestForLanguages: ["zh"]
        ),
        SttModel(
            id: "paraformer-trilingual",
            displayName: "Paraformer ZH/EN/Cantonese",
            description: "Alibaba Paraformer trilingual. Handles zh + en + yue mix.",
            sizeDisplay: "234 MB",
            totalSizeBytes: 245_119_000,
            files: [
                ModelFile(relativePath: "model.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-trilingual-zh-cantonese-en/resolve/main/model.int8.onnx", sizeBytes: 245_000_000),
                ModelFile(relativePath: "tokens.txt", url: "https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-trilingual-zh-cantonese-en/resolve/main/tokens.txt", sizeBytes: 119_000),
            ],
            bestForLanguages: ["zh", "en", "yue"]
        ),
        SttModel(
            id: "fire-red-asr-v1",
            displayName: "FireRedASR Large v1",
            description: "Xiaohongshu FireRedASR, AED architecture, very high Chinese accuracy. 1.74 GB.",
            sizeDisplay: "1.74 GB",
            totalSizeBytes: 1_735_071_400,
            files: [
                ModelFile(relativePath: "encoder.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-fire-red-asr-large-zh_en-2025-02-16/resolve/main/encoder.int8.onnx", sizeBytes: 1_290_000_000),
                ModelFile(relativePath: "decoder.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-fire-red-asr-large-zh_en-2025-02-16/resolve/main/decoder.int8.onnx", sizeBytes: 445_000_000),
                ModelFile(relativePath: "tokens.txt", url: "https://huggingface.co/csukuangfj/sherpa-onnx-fire-red-asr-large-zh_en-2025-02-16/resolve/main/tokens.txt", sizeBytes: 71_400),
            ],
            bestForLanguages: ["zh"]
        ),
        SttModel(
            id: "moonshine-base-en",
            displayName: "Moonshine Base (EN)",
            description: "Realtime-tuned English ASR, ~5x faster than Whisper, RTF<0.05.",
            sizeDisplay: "274 MB",
            totalSizeBytes: 286_929_760,
            files: [
                ModelFile(relativePath: "preprocess.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-base-en-int8/resolve/main/preprocess.onnx", sizeBytes: 14_077_290),
                ModelFile(relativePath: "encode.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-base-en-int8/resolve/main/encode.int8.onnx", sizeBytes: 50_311_494),
                ModelFile(relativePath: "uncached_decode.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-base-en-int8/resolve/main/uncached_decode.int8.onnx", sizeBytes: 122_120_451),
                ModelFile(relativePath: "cached_decode.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-base-en-int8/resolve/main/cached_decode.int8.onnx", sizeBytes: 99_983_837),
                ModelFile(relativePath: "tokens.txt", url: "https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-base-en-int8/resolve/main/tokens.txt", sizeBytes: 436_688),
            ],
            bestForLanguages: ["en"]
        ),
        SttModel(
            id: "zipformer-ctc-zh",
            displayName: "Zipformer Chinese CTC",
            description: "Next-gen Kaldi Zipformer CTC, Chinese offline. Lightweight backup.",
            sizeDisplay: "350 MB",
            totalSizeBytes: 367_013_400,
            files: [
                ModelFile(relativePath: "model.int8.onnx", url: "https://huggingface.co/csukuangfj/sherpa-onnx-zipformer-ctc-zh-int8-2025-07-03/resolve/main/model.int8.onnx", sizeBytes: 367_000_000),
                ModelFile(relativePath: "tokens.txt", url: "https://huggingface.co/csukuangfj/sherpa-onnx-zipformer-ctc-zh-int8-2025-07-03/resolve/main/tokens.txt", sizeBytes: 13_400),
            ],
            bestForLanguages: ["zh"]
        ),
    ]

    static func model(for id: String) -> SttModel? {
        if id == vadModel.id { return vadModel }
        return allModels.first { $0.id == id }
    }
}
