//! Well-known model metadata for offline MT.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtModelFamily {
    /// Helsinki-NLP MarianMT (opus-mt-* on HuggingFace)
    OpusMt,
    /// Meta NLLB-200 (multilingual, single model)
    Nllb200,
    /// Google MadLad-400 (multilingual, T5-based)
    Madlad400,
}

#[derive(Debug, Clone)]
pub struct MtModelInfo {
    pub id: &'static str,
    pub family: MtModelFamily,
    pub src_lang: &'static str,
    pub tgt_lang: &'static str,
    pub description: &'static str,
}

pub static ALL_MODELS: &[MtModelInfo] = &[
    MtModelInfo {
        id: "opus-mt-zh-en",
        family: MtModelFamily::OpusMt,
        src_lang: "zh",
        tgt_lang: "en",
        description: "Helsinki-NLP opus-mt-zh-en (Chinese → English)",
    },
    MtModelInfo {
        id: "opus-mt-ja-en",
        family: MtModelFamily::OpusMt,
        src_lang: "ja",
        tgt_lang: "en",
        description: "Helsinki-NLP opus-mt-ja-en (Japanese → English)",
    },
    MtModelInfo {
        id: "opus-mt-en-zh",
        family: MtModelFamily::OpusMt,
        src_lang: "en",
        tgt_lang: "zh",
        description: "Helsinki-NLP opus-mt-en-zh (English → Chinese)",
    },
    MtModelInfo {
        id: "opus-mt-tc-big-zh-en",
        family: MtModelFamily::OpusMt,
        src_lang: "zh",
        tgt_lang: "en",
        description: "Helsinki-NLP opus-mt-tc-big-zh-en (larger zh→en model)",
    },
    MtModelInfo {
        id: "nllb-200-distilled-600M",
        family: MtModelFamily::Nllb200,
        src_lang: "multilingual",
        tgt_lang: "multilingual",
        description: "Meta NLLB-200 distilled 600M (multilingual)",
    },
    MtModelInfo {
        id: "nllb-200-distilled-1.3B",
        family: MtModelFamily::Nllb200,
        src_lang: "multilingual",
        tgt_lang: "multilingual",
        description: "Meta NLLB-200 distilled 1.3B (multilingual)",
    },
    MtModelInfo {
        id: "google/madlad400-3b-mt",
        family: MtModelFamily::Madlad400,
        src_lang: "multilingual",
        tgt_lang: "multilingual",
        description: "Google MadLad-400 3B-MT (400+ languages, mlx-lm/transformers)",
    },
];

pub fn get_model(id: &str) -> Option<&'static MtModelInfo> {
    ALL_MODELS.iter().find(|m| m.id == id)
}

/// Map ISO 639-1 code to NLLB BCP-47 tag (best effort).
pub fn iso_to_nllb(lang: &str) -> String {
    match lang {
        "zh" | "zh-CN" | "zho_Hans" => "zho_Hans",
        "zh-TW" | "zho_Hant" => "zho_Hant",
        "en" | "eng_Latn" => "eng_Latn",
        "ja" | "jpn_Jpan" => "jpn_Jpan",
        "ko" | "kor_Hang" => "kor_Hang",
        "fr" | "fra_Latn" => "fra_Latn",
        "de" | "deu_Latn" => "deu_Latn",
        "es" | "spa_Latn" => "spa_Latn",
        "ru" | "rus_Cyrl" => "rus_Cyrl",
        "ar" | "arb_Arab" => "arb_Arab",
        other => other,
    }
    .to_string()
}
