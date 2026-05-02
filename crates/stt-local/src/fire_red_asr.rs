use std::path::Path;

use sherpa_onnx::{OfflineFireRedAsrModelConfig, OfflineRecognizer, OfflineRecognizerConfig};

use crate::registry::BackendKind;
use crate::{Result, SttError, TranscriberBackend};

pub struct FireRedAsrBackend {
    recognizer: OfflineRecognizer,
    model_id: String,
}

unsafe impl Send for FireRedAsrBackend {}
unsafe impl Sync for FireRedAsrBackend {}

impl FireRedAsrBackend {
    pub fn new(
        encoder_path: &Path,
        decoder_path: &Path,
        tokens_path: &Path,
        model_id: &str,
    ) -> Result<Self> {
        for (label, path) in [
            ("encoder", encoder_path),
            ("decoder", decoder_path),
            ("tokens", tokens_path),
        ] {
            if !path.exists() {
                return Err(SttError::Model(format!(
                    "FireRedASR {} file not found: {}",
                    label,
                    path.display()
                )));
            }
        }

        let mut config = OfflineRecognizerConfig::default();
        config.model_config.fire_red_asr = OfflineFireRedAsrModelConfig {
            encoder: Some(encoder_path.to_string_lossy().into_owned()),
            decoder: Some(decoder_path.to_string_lossy().into_owned()),
        };
        config.model_config.tokens = Some(tokens_path.to_string_lossy().into_owned());
        config.model_config.num_threads = 4;

        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| {
            SttError::Backend("Failed to create FireRedASR recognizer".to_string())
        })?;

        Ok(Self {
            recognizer,
            model_id: model_id.to_string(),
        })
    }
}

impl TranscriberBackend for FireRedAsrBackend {
    fn transcribe(&self, audio: &[f32], _language: &str) -> Result<String> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        let stream = self.recognizer.create_stream();
        stream.accept_waveform(16000, audio);
        self.recognizer.decode(&stream);

        let result = stream
            .get_result()
            .ok_or_else(|| SttError::Backend("Failed to get FireRedASR result".to_string()))?;

        Ok(result.text.trim().to_string())
    }

    fn backend_kind(&self) -> BackendKind {
        BackendKind::FireRedAsr
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}
