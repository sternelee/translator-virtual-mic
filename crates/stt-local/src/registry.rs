/// Identifies which inference backend a model uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Paraformer,
    Moonshine,
    FireRedAsr,
    ZipformerCtc,
}

#[derive(Debug, Clone)]
pub struct ModelFile {
    pub relative_path: &'static str,
    pub url: &'static str,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub backend: BackendKind,
    pub total_size_bytes: u64,
    pub size_display: &'static str,
    pub files: &'static [ModelFile],
    pub best_for_languages: &'static [&'static str],
    pub recommendation_reason: &'static str,
}

const PARAFORMER_ZH_FILES: &[ModelFile] = &[
    ModelFile {
        relative_path: "model.int8.onnx",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-zh-2024-03-09/resolve/main/model.int8.onnx",
        size_bytes: 227_330_205,
    },
    ModelFile {
        relative_path: "tokens.txt",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-zh-2024-03-09/resolve/main/tokens.txt",
        size_bytes: 75_354,
    },
];

const PARAFORMER_TRILINGUAL_FILES: &[ModelFile] = &[
    ModelFile {
        relative_path: "model.int8.onnx",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-trilingual-zh-cantonese-en/resolve/main/model.int8.onnx",
        size_bytes: 245_000_000,
    },
    ModelFile {
        relative_path: "tokens.txt",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-trilingual-zh-cantonese-en/resolve/main/tokens.txt",
        size_bytes: 119_000,
    },
];

const FIRE_RED_ASR_V1_FILES: &[ModelFile] = &[
    ModelFile {
        relative_path: "encoder.int8.onnx",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-fire-red-asr-large-zh_en-2025-02-16/resolve/main/encoder.int8.onnx",
        size_bytes: 1_290_000_000,
    },
    ModelFile {
        relative_path: "decoder.int8.onnx",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-fire-red-asr-large-zh_en-2025-02-16/resolve/main/decoder.int8.onnx",
        size_bytes: 445_000_000,
    },
    ModelFile {
        relative_path: "tokens.txt",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-fire-red-asr-large-zh_en-2025-02-16/resolve/main/tokens.txt",
        size_bytes: 71_400,
    },
];

const MOONSHINE_BASE_EN_FILES: &[ModelFile] = &[
    ModelFile {
        relative_path: "preprocess.onnx",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-base-en-int8/resolve/main/preprocess.onnx",
        size_bytes: 14_077_290,
    },
    ModelFile {
        relative_path: "encode.int8.onnx",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-base-en-int8/resolve/main/encode.int8.onnx",
        size_bytes: 50_311_494,
    },
    ModelFile {
        relative_path: "uncached_decode.int8.onnx",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-base-en-int8/resolve/main/uncached_decode.int8.onnx",
        size_bytes: 122_120_451,
    },
    ModelFile {
        relative_path: "cached_decode.int8.onnx",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-base-en-int8/resolve/main/cached_decode.int8.onnx",
        size_bytes: 99_983_837,
    },
    ModelFile {
        relative_path: "tokens.txt",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-base-en-int8/resolve/main/tokens.txt",
        size_bytes: 436_688,
    },
];

const ZIPFORMER_CTC_ZH_FILES: &[ModelFile] = &[
    ModelFile {
        relative_path: "model.int8.onnx",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-zipformer-ctc-zh-int8-2025-07-03/resolve/main/model.int8.onnx",
        size_bytes: 367_000_000,
    },
    ModelFile {
        relative_path: "tokens.txt",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-zipformer-ctc-zh-int8-2025-07-03/resolve/main/tokens.txt",
        size_bytes: 13_400,
    },
];

pub static ALL_MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "paraformer-zh",
        display_name: "Paraformer Chinese",
        description: "Alibaba Paraformer, non-autoregressive, RTF<0.07. Chinese-only, fast.",
        backend: BackendKind::Paraformer,
        total_size_bytes: 227_405_559,
        size_display: "217 MB",
        files: PARAFORMER_ZH_FILES,
        best_for_languages: &["zh"],
        recommendation_reason: "Chinese-only, very fast (RTF<0.07), high accuracy",
    },
    ModelInfo {
        id: "paraformer-trilingual",
        display_name: "Paraformer ZH/EN/Cantonese",
        description: "Alibaba Paraformer trilingual. Handles zh + en + yue mix.",
        backend: BackendKind::Paraformer,
        total_size_bytes: 245_119_000,
        size_display: "234 MB",
        files: PARAFORMER_TRILINGUAL_FILES,
        best_for_languages: &["zh", "en", "yue"],
        recommendation_reason: "Trilingual zh/en/yue; only Cantonese option available",
    },
    ModelInfo {
        id: "fire-red-asr-v1",
        display_name: "FireRedASR Large v1",
        description:
            "Xiaohongshu FireRedASR, AED architecture, very high Chinese accuracy. 1.74 GB.",
        backend: BackendKind::FireRedAsr,
        total_size_bytes: 1_735_071_400,
        size_display: "1.74 GB",
        files: FIRE_RED_ASR_V1_FILES,
        best_for_languages: &["zh"],
        recommendation_reason: "Chinese ASR SOTA (CER ~2%), best accuracy, large size",
    },
    ModelInfo {
        id: "moonshine-base-en",
        display_name: "Moonshine Base (EN)",
        description: "Realtime-tuned English ASR, ~5x faster than Whisper, RTF<0.05.",
        backend: BackendKind::Moonshine,
        total_size_bytes: 286_929_760,
        size_display: "274 MB",
        files: MOONSHINE_BASE_EN_FILES,
        best_for_languages: &["en"],
        recommendation_reason: "English-only, ~5x faster than Whisper",
    },
    ModelInfo {
        id: "zipformer-ctc-zh",
        display_name: "Zipformer Chinese CTC",
        description: "Next-gen Kaldi Zipformer CTC, Chinese offline. Lightweight backup.",
        backend: BackendKind::ZipformerCtc,
        total_size_bytes: 367_013_400,
        size_display: "350 MB",
        files: ZIPFORMER_CTC_ZH_FILES,
        best_for_languages: &["zh"],
        recommendation_reason: "Chinese offline CTC, modest size, fast",
    },
];

pub fn get_model(id: &str) -> Option<&'static ModelInfo> {
    ALL_MODELS.iter().find(|m| m.id == id)
}
