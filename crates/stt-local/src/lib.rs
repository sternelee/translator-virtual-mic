pub mod audio;
pub mod fire_red_asr;
pub mod manager;
pub mod moonshine;
pub mod paraformer;
pub mod registry;
pub mod tts;
pub mod vad;
pub mod zipformer_ctc;

use std::path::Path;

use thiserror::Error;

pub use registry::{BackendKind, ModelInfo, ALL_MODELS};
pub use tts::TtsBackend;

#[derive(Debug, Error)]
pub enum SttError {
    #[error("model error: {0}")]
    Model(String),
    #[error("audio error: {0}")]
    Audio(String),
    #[error("backend error: {0}")]
    Backend(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SttError>;

pub trait TranscriberBackend: Send + Sync {
    fn transcribe(&self, audio: &[f32], language: &str) -> Result<String>;
    fn backend_kind(&self) -> BackendKind;
    fn model_id(&self) -> &str;
}

pub struct ManagedTranscriber {
    inner: Option<Box<dyn TranscriberBackend>>,
}

impl ManagedTranscriber {
    pub fn new() -> Self {
        Self { inner: None }
    }

    pub fn load(&mut self, backend: Box<dyn TranscriberBackend>) {
        self.inner = Some(backend);
    }

    pub fn unload(&mut self) {
        self.inner = None;
    }

    pub fn is_loaded(&self) -> bool {
        self.inner.is_some()
    }

    pub fn transcribe(&self, audio: &[f32], language: &str) -> Result<String> {
        let backend = self
            .inner
            .as_ref()
            .ok_or_else(|| SttError::Backend("no backend loaded".to_string()))?;
        backend.transcribe(audio, language)
    }

    pub fn model_id(&self) -> Option<&str> {
        self.inner.as_ref().map(|b| b.model_id())
    }

    pub fn backend_kind(&self) -> Option<BackendKind> {
        self.inner.as_ref().map(|b| b.backend_kind())
    }
}

impl Default for ManagedTranscriber {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a backend instance for the given model id, expecting model files to
/// already exist under `models_root/<model_id>/`.
pub fn load_backend(model_id: &str, models_root: &Path) -> Result<Box<dyn TranscriberBackend>> {
    eprintln!(
        "[stt-local] load_backend: model_id={} models_root={:?}",
        model_id, models_root
    );
    let info = registry::get_model(model_id)
        .ok_or_else(|| SttError::Model(format!("unknown model id: {model_id}")))?;
    let model_dir = models_root.join(model_id);
    eprintln!("[stt-local] load_backend: model_dir={:?}", model_dir);

    match info.backend {
        BackendKind::Paraformer => {
            let (onnx, tokens) = manager::paraformer_model_paths(&model_dir);
            eprintln!(
                "[stt-local] paraformer paths: onnx={:?} tokens={:?}",
                onnx, tokens
            );
            let backend = paraformer::ParaformerBackend::new(&onnx, &tokens, model_id)?;
            eprintln!("[stt-local] paraformer backend created");
            Ok(Box::new(backend))
        }
        BackendKind::Moonshine => {
            let (preprocessor, encoder, uncached_decoder, cached_decoder, tokens) =
                manager::moonshine_model_paths(&model_dir);
            eprintln!(
                "[stt-local] moonshine paths: preprocess={:?} encode={:?} tokens={:?}",
                preprocessor, encoder, tokens
            );
            let backend = moonshine::MoonshineBackend::new(
                &preprocessor,
                &encoder,
                &uncached_decoder,
                &cached_decoder,
                &tokens,
                model_id,
            )?;
            eprintln!("[stt-local] moonshine backend created");
            Ok(Box::new(backend))
        }
        BackendKind::FireRedAsr => {
            let (encoder, decoder, tokens) = manager::fire_red_asr_model_paths(&model_dir);
            eprintln!(
                "[stt-local] fire_red_asr paths: encoder={:?} decoder={:?} tokens={:?}",
                encoder, decoder, tokens
            );
            let backend =
                fire_red_asr::FireRedAsrBackend::new(&encoder, &decoder, &tokens, model_id)?;
            eprintln!("[stt-local] fire_red_asr backend created");
            Ok(Box::new(backend))
        }
        BackendKind::ZipformerCtc => {
            let (onnx, tokens) = manager::zipformer_ctc_model_paths(&model_dir);
            eprintln!(
                "[stt-local] zipformer_ctc paths: onnx={:?} tokens={:?}",
                onnx, tokens
            );
            let backend = zipformer_ctc::ZipformerCtcBackend::new(&onnx, &tokens, model_id)?;
            eprintln!("[stt-local] zipformer_ctc backend created");
            Ok(Box::new(backend))
        }
    }
}

/// Load a TTS backend for `model_id`, expecting files under `models_root/<model_id>/`.
pub fn load_tts_backend(
    model_id: &str,
    models_root: &Path,
    speaker_id: i32,
    speed: f32,
) -> Result<TtsBackend> {
    TtsBackend::new(model_id, models_root, speaker_id, speed)
}
