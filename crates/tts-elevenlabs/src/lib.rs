//! ElevenLabs TTS client for the local caption pipeline.
//!
//! POSTs JSON to `POST https://api.elevenlabs.io/v1/text-to-speech/{voice_id}/stream`
//! with `output_format=pcm_24000`. Returns raw int16 LE PCM at 24 000 Hz, which
//! this crate converts to f32 samples.
//!
//! This crate is synchronous (ureq, no tokio) — safe to call from the
//! caption worker thread.

use std::io::Read;
use std::time::Duration;

use thiserror::Error;

const API_BASE: &str = "https://api.elevenlabs.io/v1";
const REQUEST_TIMEOUT_SECS: u64 = 30;
/// ElevenLabs `pcm_24000` output: 24 000 Hz int16 mono PCM.
pub const OUTPUT_SAMPLE_RATE: u32 = 24_000;

#[derive(Debug, Error)]
pub enum ElevenLabsTtsError {
    #[error("config error: {0}")]
    Config(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("api error: HTTP {status}: {body}")]
    Api { status: u16, body: String },
    #[error("decode error: {0}")]
    Decode(String),
}

pub type Result<T> = std::result::Result<T, ElevenLabsTtsError>;

#[derive(Debug, Clone)]
pub struct ElevenLabsTtsConfig {
    pub enabled: bool,
    /// ElevenLabs API key (xi-api-key header).
    pub api_key: String,
    /// Voice ID to use for synthesis.
    pub voice_id: String,
    /// Model ID, e.g. "eleven_multilingual_v2" or "eleven_turbo_v2_5".
    pub model_id: String,
}

impl Default for ElevenLabsTtsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: String::new(),
            voice_id: String::new(),
            model_id: "eleven_multilingual_v2".to_string(),
        }
    }
}

pub struct ElevenLabsTtsClient {
    config: ElevenLabsTtsConfig,
    agent: ureq::Agent,
}

impl ElevenLabsTtsClient {
    /// Create client. Returns `Err` if api_key or voice_id is empty.
    pub fn new(config: ElevenLabsTtsConfig) -> Result<Self> {
        if config.api_key.trim().is_empty() {
            return Err(ElevenLabsTtsError::Config("api_key is empty".into()));
        }
        if config.voice_id.trim().is_empty() {
            return Err(ElevenLabsTtsError::Config("voice_id is empty".into()));
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build();
        Ok(Self { config, agent })
    }

    /// Synthesise `text` via ElevenLabs streaming TTS.
    ///
    /// Returns f32 PCM samples at [`OUTPUT_SAMPLE_RATE`] (24 000 Hz).
    pub fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32)> {
        let text = text.trim();
        if text.is_empty() {
            return Ok((Vec::new(), OUTPUT_SAMPLE_RATE));
        }

        let url = format!(
            "{API_BASE}/text-to-speech/{}/stream?output_format=pcm_24000",
            self.config.voice_id
        );

        let body = format!(
            r#"{{"text":{},"model_id":{}}}"#,
            serde_json_str(text),
            serde_json_str(&self.config.model_id),
        );

        let response = self
            .agent
            .post(&url)
            .set("xi-api-key", &self.config.api_key)
            .set("Content-Type", "application/json")
            .send_string(&body);

        let response = match response {
            Ok(r) => r,
            Err(ureq::Error::Status(status, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                return Err(ElevenLabsTtsError::Api { status, body });
            }
            Err(e) => return Err(ElevenLabsTtsError::Network(e.to_string())),
        };

        let mut raw: Vec<u8> = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut raw)
            .map_err(|e| ElevenLabsTtsError::Decode(format!("read response body: {e}")))?;

        Ok((pcm_i16_le_to_f32(&raw), OUTPUT_SAMPLE_RATE))
    }
}

/// Minimal JSON string escaping (no external dep).
fn serde_json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Convert raw little-endian int16 bytes to normalised f32 samples.
fn pcm_i16_le_to_f32(raw: &[u8]) -> Vec<f32> {
    raw.chunks_exact(2)
        .map(|b| {
            let s = i16::from_le_bytes([b[0], b[1]]);
            f32::from(s) / f32::from(i16::MAX)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_api_key() {
        let cfg = ElevenLabsTtsConfig {
            api_key: String::new(),
            voice_id: "abc".into(),
            ..Default::default()
        };
        assert!(matches!(
            ElevenLabsTtsClient::new(cfg),
            Err(ElevenLabsTtsError::Config(_))
        ));
    }

    #[test]
    fn rejects_empty_voice_id() {
        let cfg = ElevenLabsTtsConfig {
            api_key: "key".into(),
            voice_id: String::new(),
            ..Default::default()
        };
        assert!(matches!(
            ElevenLabsTtsClient::new(cfg),
            Err(ElevenLabsTtsError::Config(_))
        ));
    }

    #[test]
    fn synthesize_returns_empty_for_blank_text() {
        let cfg = ElevenLabsTtsConfig {
            enabled: true,
            api_key: "key".into(),
            voice_id: "vid".into(),
            model_id: "eleven_multilingual_v2".into(),
        };
        let client = ElevenLabsTtsClient::new(cfg).unwrap();
        let (samples, rate) = client.synthesize("   ").unwrap();
        assert!(samples.is_empty());
        assert_eq!(rate, OUTPUT_SAMPLE_RATE);
    }

    #[test]
    fn pcm_conversion_zero() {
        let bytes = [0u8, 0u8];
        let samples = pcm_i16_le_to_f32(&bytes);
        assert_eq!(samples.len(), 1);
        assert!(samples[0].abs() < 1e-6);
    }

    #[test]
    fn pcm_conversion_max() {
        let bytes = i16::MAX.to_le_bytes();
        let samples = pcm_i16_le_to_f32(&bytes);
        assert!((samples[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn json_escaping() {
        let result = serde_json_str("hello \"world\"\nnew");
        assert_eq!(result, r#""hello \"world\"\nnew""#);
    }
}
