//! CosyVoice 2 TTS client.
//!
//! POSTs multipart/form-data to a locally-running CosyVoice FastAPI server
//! (`POST /inference_zero_shot`). Returns raw f32 PCM samples at 22 050 Hz.
//!
//! This crate is synchronous (ureq, no tokio) — safe to call from the
//! caption worker thread.

use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:50000";
const REQUEST_TIMEOUT_SECS: u64 = 30;
/// CosyVoice server output: 22 050 Hz int16 mono PCM.
pub const OUTPUT_SAMPLE_RATE: u32 = 22_050;

#[derive(Debug, Error)]
pub enum CosyVoiceError {
    #[error("config error: {0}")]
    Config(String),
    #[error("io error reading reference wav: {0}")]
    Io(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("api error: HTTP {status}: {body}")]
    Api { status: u16, body: String },
    #[error("decode error: {0}")]
    Decode(String),
}

pub type Result<T> = std::result::Result<T, CosyVoiceError>;

#[derive(Debug, Clone)]
pub struct CosyVoiceConfig {
    /// Base URL of the CosyVoice FastAPI server, e.g. "http://127.0.0.1:50000".
    pub endpoint: String,
    /// Path to reference voice WAV file.
    pub prompt_wav_path: PathBuf,
    /// Transcript of the reference WAV (may be empty).
    pub prompt_text: String,
}

impl Default for CosyVoiceConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_string(),
            prompt_wav_path: PathBuf::new(),
            prompt_text: String::new(),
        }
    }
}

pub struct CosyVoiceClient {
    config: CosyVoiceConfig,
    agent: ureq::Agent,
}

impl CosyVoiceClient {
    /// Create client. Validates config but does NOT contact the server.
    /// Returns `Err` if the endpoint or reference WAV path is empty.
    pub fn new(config: CosyVoiceConfig) -> Result<Self> {
        if config.endpoint.trim().is_empty() {
            return Err(CosyVoiceError::Config("endpoint is empty".into()));
        }
        if config.prompt_wav_path.as_os_str().is_empty() {
            return Err(CosyVoiceError::Config("prompt_wav_path is empty".into()));
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build();
        Ok(Self { config, agent })
    }

    /// Synthesize `text` using zero-shot voice cloning.
    ///
    /// Reads the reference WAV from disk, POSTs to `/inference_zero_shot`,
    /// and returns f32 PCM samples at [`OUTPUT_SAMPLE_RATE`] (22 050 Hz).
    pub fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32)> {
        let text = text.trim();
        if text.is_empty() {
            return Ok((Vec::new(), OUTPUT_SAMPLE_RATE));
        }

        let wav_bytes = std::fs::read(&self.config.prompt_wav_path).map_err(|e| {
            CosyVoiceError::Io(format!("read {:?}: {e}", self.config.prompt_wav_path))
        })?;

        let boundary = "----CosyVoiceBoundary7Ma4YWxkTrZu0gW";
        let body = build_multipart(boundary, text, &self.config.prompt_text, &wav_bytes);
        let content_type = format!("multipart/form-data; boundary={boundary}");
        let url = format!("{}/inference_zero_shot", self.config.endpoint);

        let response = self
            .agent
            .post(&url)
            .set("Content-Type", &content_type)
            .send_bytes(&body);

        let response = match response {
            Ok(r) => r,
            Err(ureq::Error::Status(status, body)) => {
                let text = body.into_string().unwrap_or_default();
                return Err(CosyVoiceError::Api { status, body: text });
            }
            Err(e) => return Err(CosyVoiceError::Network(e.to_string())),
        };

        let mut raw: Vec<u8> = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut raw)
            .map_err(|e| CosyVoiceError::Decode(format!("read response body: {e}")))?;

        Ok((pcm_i16_le_to_f32(&raw), OUTPUT_SAMPLE_RATE))
    }
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

/// Build a multipart/form-data body with fields: tts_text, prompt_text, prompt_wav.
fn build_multipart(boundary: &str, tts_text: &str, prompt_text: &str, wav_bytes: &[u8]) -> Vec<u8> {
    let mut body: Vec<u8> = Vec::new();
    append_text_field(&mut body, boundary, "tts_text", tts_text);
    append_text_field(&mut body, boundary, "prompt_text", prompt_text);
    append_file_field(
        &mut body,
        boundary,
        "prompt_wav",
        "ref_voice.wav",
        "audio/wav",
        wav_bytes,
    );
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

fn append_text_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        )
        .as_bytes(),
    );
}

fn append_file_field(
    body: &mut Vec<u8>,
    boundary: &str,
    name: &str,
    filename: &str,
    content_type: &str,
    data: &[u8],
) {
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(b"\r\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_endpoint() {
        let cfg = CosyVoiceConfig {
            endpoint: String::new(),
            prompt_wav_path: PathBuf::from("/tmp/ref.wav"),
            prompt_text: String::new(),
        };
        assert!(matches!(
            CosyVoiceClient::new(cfg),
            Err(CosyVoiceError::Config(_))
        ));
    }

    #[test]
    fn rejects_empty_wav_path() {
        let cfg = CosyVoiceConfig {
            endpoint: "http://127.0.0.1:50000".into(),
            prompt_wav_path: PathBuf::new(),
            prompt_text: String::new(),
        };
        assert!(matches!(
            CosyVoiceClient::new(cfg),
            Err(CosyVoiceError::Config(_))
        ));
    }

    #[test]
    fn pcm_i16_le_to_f32_zero_is_zero() {
        let bytes = [0u8, 0u8, 0u8, 0u8];
        let samples = pcm_i16_le_to_f32(&bytes);
        assert_eq!(samples.len(), 2);
        assert!(samples[0].abs() < 1e-6);
    }

    #[test]
    fn pcm_i16_le_to_f32_max_is_one() {
        let max = i16::MAX;
        let bytes = max.to_le_bytes();
        let samples = pcm_i16_le_to_f32(&bytes);
        assert_eq!(samples.len(), 1);
        assert!((samples[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn pcm_i16_le_to_f32_odd_bytes_ignored() {
        let bytes = [0u8, 0u8, 0xFF];
        let samples = pcm_i16_le_to_f32(&bytes);
        assert_eq!(samples.len(), 1);
    }

    #[test]
    fn multipart_body_contains_fields() {
        let body = build_multipart("BOUNDARY", "hello world", "reference text", b"WAVDATA");
        let s = String::from_utf8_lossy(&body);
        assert!(s.contains("--BOUNDARY"));
        assert!(s.contains("name=\"tts_text\""));
        assert!(s.contains("hello world"));
        assert!(s.contains("name=\"prompt_text\""));
        assert!(s.contains("reference text"));
        assert!(s.contains("name=\"prompt_wav\""));
        assert!(s.contains("filename=\"ref_voice.wav\""));
        assert!(s.contains("--BOUNDARY--"));
    }

    #[test]
    fn multipart_body_binary_wav_preserved() {
        let wav = vec![0xAB_u8, 0xFF, 0x00];
        let body = build_multipart("B", "txt", "ptxt", &wav);
        let header = b"Content-Type: audio/wav\r\n\r\n";
        let pos = body
            .windows(header.len())
            .position(|w| w == header)
            .expect("header present");
        let after_header = &body[pos + header.len()..];
        assert!(after_header.starts_with(&wav));
    }

    #[test]
    fn synthesize_returns_empty_for_blank_text() {
        let cfg = CosyVoiceConfig {
            endpoint: "http://127.0.0.1:50000".into(),
            prompt_wav_path: PathBuf::from("/tmp/any.wav"),
            prompt_text: String::new(),
        };
        let client = CosyVoiceClient::new(cfg).unwrap();
        let (samples, rate) = client.synthesize("   ").unwrap();
        assert!(samples.is_empty());
        assert_eq!(rate, OUTPUT_SAMPLE_RATE);
    }
}
