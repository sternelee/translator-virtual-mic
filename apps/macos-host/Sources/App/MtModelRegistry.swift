import Foundation

enum MtModelRegistry {
    static let allModels: [MtModelInfo] = [
        MtModelInfo(
            id: "opus-mt-zh-en",
            family: .opusMt,
            srcLang: "zh",
            tgtLang: "en",
            description: "Helsinki-NLP OPUS-MT Chinese to English. Fast, high-quality bilingual model.",
            sizeDisplay: "300 MB",
            totalSizeBytes: 300_000_000,
            files: [
                MtModelFile(relativePath: "encoder_model.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-zh-en/resolve/main/onnx/encoder_model.onnx", sizeBytes: 150_000_000),
                MtModelFile(relativePath: "decoder_model_merged.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-zh-en/resolve/main/onnx/decoder_model_merged.onnx", sizeBytes: 140_000_000),
                MtModelFile(relativePath: "decoder_model.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-zh-en/resolve/main/onnx/decoder_model.onnx", sizeBytes: 140_000_000),
                MtModelFile(relativePath: "tokenizer.json", url: "https://huggingface.co/Helsinki-NLP/opus-mt-zh-en/resolve/main/tokenizer.json", sizeBytes: 2_000_000),
            ],
            hfRepo: "Helsinki-NLP/opus-mt-zh-en"
        ),
        MtModelInfo(
            id: "opus-mt-tc-big-zh-en",
            family: .opusMt,
            srcLang: "zh",
            tgtLang: "en",
            description: "OPUS-MT Chinese to English (large). Higher quality, slower than standard.",
            sizeDisplay: "600 MB",
            totalSizeBytes: 600_000_000,
            files: [
                MtModelFile(relativePath: "encoder_model.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-tc-big-zh-en/resolve/main/onnx/encoder_model.onnx", sizeBytes: 300_000_000),
                MtModelFile(relativePath: "decoder_model_merged.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-tc-big-zh-en/resolve/main/onnx/decoder_model_merged.onnx", sizeBytes: 290_000_000),
                MtModelFile(relativePath: "decoder_model.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-tc-big-zh-en/resolve/main/onnx/decoder_model.onnx", sizeBytes: 290_000_000),
                MtModelFile(relativePath: "tokenizer.json", url: "https://huggingface.co/Helsinki-NLP/opus-mt-tc-big-zh-en/resolve/main/tokenizer.json", sizeBytes: 2_000_000),
            ],
            hfRepo: "Helsinki-NLP/opus-mt-tc-big-zh-en"
        ),
        MtModelInfo(
            id: "opus-mt-en-zh",
            family: .opusMt,
            srcLang: "en",
            tgtLang: "zh",
            description: "Helsinki-NLP OPUS-MT English to Chinese. Fast, high-quality bilingual model.",
            sizeDisplay: "300 MB",
            totalSizeBytes: 300_000_000,
            files: [
                MtModelFile(relativePath: "encoder_model.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-en-zh/resolve/main/onnx/encoder_model.onnx", sizeBytes: 150_000_000),
                MtModelFile(relativePath: "decoder_model_merged.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-en-zh/resolve/main/onnx/decoder_model_merged.onnx", sizeBytes: 140_000_000),
                MtModelFile(relativePath: "decoder_model.onnx", url: "https://huggingface.co/Helsinki-NLP/opus-mt-en-zh/resolve/main/onnx/decoder_model.onnx", sizeBytes: 140_000_000),
                MtModelFile(relativePath: "tokenizer.json", url: "https://huggingface.co/Helsinki-NLP/opus-mt-en-zh/resolve/main/tokenizer.json", sizeBytes: 2_000_000),
            ],
            hfRepo: "Helsinki-NLP/opus-mt-en-zh"
        ),
        MtModelInfo(
            id: "nllb-200-distilled-600M",
            family: .nllb200,
            srcLang: "*",
            tgtLang: "*",
            description: "Meta NLLB-200 distilled 600M. Multilingual model supporting 200+ languages.",
            sizeDisplay: "1.2 GB",
            totalSizeBytes: 1_200_000_000,
            files: [
                MtModelFile(relativePath: "encoder_model.onnx", url: "https://huggingface.co/facebook/nllb-200-distilled-600M/resolve/main/onnx/encoder_model.onnx", sizeBytes: 600_000_000),
                MtModelFile(relativePath: "decoder_model_merged.onnx", url: "https://huggingface.co/facebook/nllb-200-distilled-600M/resolve/main/onnx/decoder_model_merged.onnx", sizeBytes: 580_000_000),
                MtModelFile(relativePath: "decoder_model.onnx", url: "https://huggingface.co/facebook/nllb-200-distilled-600M/resolve/main/onnx/decoder_model.onnx", sizeBytes: 580_000_000),
                MtModelFile(relativePath: "tokenizer.json", url: "https://huggingface.co/facebook/nllb-200-distilled-600M/resolve/main/tokenizer.json", sizeBytes: 5_000_000),
            ],
            hfRepo: "facebook/nllb-200-distilled-600M"
        ),
        MtModelInfo(
            id: "nllb-200-distilled-1.3B",
            family: .nllb200,
            srcLang: "*",
            tgtLang: "*",
            description: "Meta NLLB-200 distilled 1.3B. Higher quality multilingual model.",
            sizeDisplay: "2.5 GB",
            totalSizeBytes: 2_500_000_000,
            files: [
                MtModelFile(relativePath: "encoder_model.onnx", url: "https://huggingface.co/facebook/nllb-200-distilled-1.3B/resolve/main/onnx/encoder_model.onnx", sizeBytes: 1_250_000_000),
                MtModelFile(relativePath: "decoder_model_merged.onnx", url: "https://huggingface.co/facebook/nllb-200-distilled-1.3B/resolve/main/onnx/decoder_model_merged.onnx", sizeBytes: 1_200_000_000),
                MtModelFile(relativePath: "decoder_model.onnx", url: "https://huggingface.co/facebook/nllb-200-distilled-1.3B/resolve/main/onnx/decoder_model.onnx", sizeBytes: 1_200_000_000),
                MtModelFile(relativePath: "tokenizer.json", url: "https://huggingface.co/facebook/nllb-200-distilled-1.3B/resolve/main/tokenizer.json", sizeBytes: 5_000_000),
            ],
            hfRepo: "facebook/nllb-200-distilled-1.3B"
        ),
    ]

    static func model(for id: String) -> MtModelInfo? {
        allModels.first { $0.id == id }
    }
}
