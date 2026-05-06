//! Voicebox TTS sidecar HTTP client.
//!
//! POSTs JSON to a locally-running Python FastAPI server (`POST /synthesize`).
//! Returns raw f32 PCM samples at the engine's native sample rate
//! (usually 24000 Hz, 48000 Hz for LuxTTS).
//!
//! This crate is synchronous (ureq, no tokio) — safe to call from the
//! caption worker thread.

use std::io::Read;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:50001";
const REQUEST_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Error)]
pub enum SidecarError {
    #[error("config error: {0}")]
    Config(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("api error: HTTP {status}: {body}")]
    Api { status: u16, body: String },
    #[error("decode error: {0}")]
    Decode(String),
}

pub type Result<T> = std::result::Result<T, SidecarError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizeRequest {
    pub engine: String,
    pub text: String,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_audio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instruct: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_size: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub model_name: String,
    pub display_name: String,
    pub engine: String,
    pub model_size: String,
    pub size_mb: u64,
    pub languages: Vec<String>,
    pub needs_trim: bool,
    pub supports_instruct: bool,
}

#[derive(Debug, Clone)]
pub struct SidecarConfig {
    pub endpoint: String,
    /// Engine to use: "kokoro", "qwen_tts", "chatterbox", "chatterbox_turbo",
    /// "hume", "luxtts", "qwen_tts_mlx", "qwen_custom_voice"
    pub engine: String,
    /// Preset voice ID (e.g. "af_heart" for Kokoro)
    pub voice_name: Option<String>,
    /// Path to reference WAV for zero-shot voice cloning
    pub ref_audio: Option<String>,
    /// Transcript of reference audio
    pub ref_text: Option<String>,
    /// Language code ("en", "ja", "zh", etc.)
    pub language: String,
    /// Model size variant ("0.6B", "1.7B" for Qwen, "default" otherwise)
    pub model_size: Option<String>,
}

impl Default for SidecarConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_string(),
            engine: "kokoro".to_string(),
            voice_name: None,
            ref_audio: None,
            ref_text: None,
            language: "en".to_string(),
            model_size: None,
        }
    }
}

pub struct SidecarClient {
    config: SidecarConfig,
    agent: ureq::Agent,
}

impl SidecarClient {
    pub fn new(config: SidecarConfig) -> Result<Self> {
        if config.endpoint.trim().is_empty() {
            return Err(SidecarError::Config("endpoint is empty".into()));
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build();
        Ok(Self { config, agent })
    }

    /// Synthesize text and return (f32_pcm_samples, native_sample_rate).
    pub fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32)> {
        let text = text.trim();
        if text.is_empty() {
            return Ok((Vec::new(), 24_000));
        }

        let req = SynthesizeRequest {
            engine: self.config.engine.clone(),
            text: text.to_string(),
            language: self.config.language.clone(),
            voice_name: self.config.voice_name.clone(),
            ref_audio: self.config.ref_audio.clone(),
            ref_text: self.config.ref_text.clone(),
            seed: None,
            instruct: None,
            model_size: self.config.model_size.clone(),
        };

        let body = serde_json::to_vec(&req)
            .map_err(|e| SidecarError::Decode(format!("serialize request: {e}")))?;

        let url = format!("{}/synthesize", self.config.endpoint);

        let response = self
            .agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_bytes(&body);

        let response = match response {
            Ok(r) => r,
            Err(ureq::Error::Status(status, body)) => {
                let text = body.into_string().unwrap_or_default();
                return Err(SidecarError::Api { status, body: text });
            }
            Err(e) => return Err(SidecarError::Network(e.to_string())),
        };

        let sample_rate: u32 = response
            .header("X-Sample-Rate")
            .and_then(|v| v.parse().ok())
            .unwrap_or(24_000);

        let mut raw: Vec<u8> = Vec::new();
        response
            .into_reader()
            .take(10 * 1024 * 1024) // 10 MiB max
            .read_to_end(&mut raw)
            .map_err(|e| SidecarError::Decode(format!("read response body: {e}")))?;

        let samples = f32_le_bytes_to_vec(&raw);
        Ok((samples, sample_rate))
    }

    /// List available models from the sidecar.
    pub fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/models", self.config.endpoint);
        let response = self.agent.get(&url).call();

        let response = match response {
            Ok(r) => r,
            Err(ureq::Error::Status(status, body)) => {
                let text = body.into_string().unwrap_or_default();
                return Err(SidecarError::Api { status, body: text });
            }
            Err(e) => return Err(SidecarError::Network(e.to_string())),
        };

        let models: Vec<ModelInfo> = response
            .into_json()
            .map_err(|e| SidecarError::Decode(format!("parse models: {e}")))?;
        Ok(models)
    }

    /// Check if the sidecar server is reachable.
    pub fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.config.endpoint);
        match self.agent.get(&url).call() {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Eagerly load a model (idempotent).
    pub fn load_model(&self, engine: &str, model_size: &str) -> Result<()> {
        let url = format!("{}/models/{engine}/load", self.config.endpoint);
        let body = serde_json::json!({ "model_size": model_size }).to_string();
        let response = self
            .agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_bytes(body.as_bytes());

        match response {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(status, body)) => {
                let text = body.into_string().unwrap_or_default();
                Err(SidecarError::Api { status, body: text })
            }
            Err(e) => Err(SidecarError::Network(e.to_string())),
        }
    }
}

/// Convert raw little-endian f32 bytes to Vec<f32>.
/// Ignores trailing incomplete frame.
fn f32_le_bytes_to_vec(raw: &[u8]) -> Vec<f32> {
    raw.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_endpoint() {
        let cfg = SidecarConfig {
            endpoint: String::new(),
            ..Default::default()
        };
        assert!(matches!(
            SidecarClient::new(cfg),
            Err(SidecarError::Config(_))
        ));
    }

    #[test]
    fn synthesize_returns_empty_for_blank_text() {
        let cfg = SidecarConfig::default();
        let client = SidecarClient::new(cfg).unwrap();
        let (samples, rate) = client.synthesize("   ").unwrap();
        assert!(samples.is_empty());
        assert_eq!(rate, 24_000);
    }

    #[test]
    fn f32_le_zero_is_zero() {
        let bytes = [0u8; 8];
        let samples = f32_le_bytes_to_vec(&bytes);
        assert_eq!(samples, vec![0.0_f32, 0.0_f32]);
    }

    #[test]
    fn f32_le_one_is_one() {
        let bytes = 1.0_f32.to_le_bytes();
        let samples = f32_le_bytes_to_vec(&bytes);
        assert_eq!(samples, vec![1.0_f32]);
    }

    #[test]
    fn f32_le_ignores_trailing_incomplete_frame() {
        let bytes = [0u8; 7]; // 7 bytes — one full f32 + 3 trailing
        let samples = f32_le_bytes_to_vec(&bytes);
        assert_eq!(samples.len(), 1);
    }
}
