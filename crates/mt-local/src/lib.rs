//! Offline neural machine translation via ONNX Runtime.
//!
//! Supported model families:
//!
//! ## OPUS-MT (Helsinki-NLP MarianMT)
//! Files under `model_dir/<model_id>/`:
//! ```text
//! encoder_model.onnx
//! decoder_model_merged.onnx   (merged with past-key-values for efficient decode)
//! tokenizer.json              (HuggingFace fast-tokenizer format)
//! ```
//! Download via `optimum-cli export onnx --model Helsinki-NLP/opus-mt-zh-en ./opus-mt-zh-en`
//!
//! ## NLLB-200 (Meta, multilingual)
//! Same file layout; model_id starts with `"nllb-"`.
//! Source/target lang codes use NLLB BCP-47 (e.g. `"zho_Hans"`, `"eng_Latn"`).

pub mod marian;
pub mod registry;

use std::path::Path;
use thiserror::Error;

pub use marian::MarianBackend;

#[derive(Debug, Error)]
pub enum MtLocalError {
    #[error("model error: {0}")]
    Model(String),
    #[error("tokenizer error: {0}")]
    Tokenizer(String),
    #[error("inference error: {0}")]
    Inference(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, MtLocalError>;

/// Common interface for all local MT backends.
pub trait LocalMtBackend: Send + Sync {
    /// Translate `text` to `target_lang`. `target_lang` is an ISO 639-1 code
    /// (e.g. `"en"`, `"zh"`, `"ja"`) or NLLB BCP-47 for NLLB models.
    fn translate(&self, text: &str, target_lang: &str) -> Result<String>;

    /// Human-readable model identifier.
    fn model_id(&self) -> &str;
}

/// Load a local MT backend from files under `model_dir/<model_id>/`.
///
/// `src_lang` is an ISO 639-1 code (e.g. `"zh"`, `"en"`) used to build the
/// NLLB source-language prefix token.  For OPUS-MT models it is ignored.
pub fn load_backend(
    model_id: &str,
    model_dir: &Path,
    src_lang: &str,
) -> Result<Box<dyn LocalMtBackend>> {
    eprintln!(
        "[mt-local] load_backend: model_id={model_id} model_dir={model_dir:?} src_lang={src_lang}"
    );
    let dir = model_dir.join(model_id);
    let backend = marian::MarianBackend::new(model_id, &dir, src_lang)?;
    Ok(Box::new(backend))
}
