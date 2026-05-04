//! MiniMax TTS V2 client.
//!
//! Posts JSON to `POST {api_host}/v1/t2a_v2` and returns f32 PCM samples.
//!
//! This crate is synchronous (ureq, no tokio) — safe to call from the
//! caption worker thread.

use std::time::Duration;

use thiserror::Error;

use common::MiniMaxTtsConfig;

const DEFAULT_API_HOST: &str = "https://api.minimaxi.com";
const DEFAULT_MODEL: &str = "speech-01-turbo";
const DEFAULT_VOICE_ID: &str = "male-qn-qingse";
const DEFAULT_EMOTION: &str = "happy";
const OUTPUT_SAMPLE_RATE: u32 = 24_000;
const REQUEST_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Error)]
pub enum MiniMaxTtsError {
    #[error("config error: {0}")]
    Config(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("api error: HTTP {status}: {body}")]
    Api { status: u16, body: String },
    #[error("decode error: {0}")]
    Decode(String),
    #[error("api returned error: {0}")]
    ApiError(String),
}

pub type Result<T> = std::result::Result<T, MiniMaxTtsError>;

pub struct MiniMaxTtsClient {
    api_host: String,
    api_key: String,
    model: String,
    voice_id: String,
    emotion: String,
    speed: f32,
    vol: f32,
    pitch: i32,
    sample_rate: u32,
    agent: ureq::Agent,
}

impl MiniMaxTtsClient {
    pub fn new(config: &MiniMaxTtsConfig) -> Result<Self> {
        let api_key = if config.api_key.is_empty() {
            std::env::var("MINIMAX_API_KEY")
                .map_err(|_| MiniMaxTtsError::Config("MINIMAX_API_KEY not set".into()))?
        } else {
            config.api_key.clone()
        };

        let voice_id = if config.voice_id.is_empty() {
            std::env::var("MINIMAX_VOICE_ID").unwrap_or_else(|_| DEFAULT_VOICE_ID.to_string())
        } else {
            config.voice_id.clone()
        };

        let api_host = if config.api_host.is_empty() {
            DEFAULT_API_HOST.to_string()
        } else {
            config.api_host.clone()
        };

        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build();

        Ok(Self {
            api_host,
            api_key,
            model: config.model.clone(),
            voice_id,
            emotion: config.emotion.clone(),
            speed: config.speed,
            vol: config.vol,
            pitch: config.pitch,
            sample_rate: config.sample_rate,
            agent,
        })
    }

    /// Synthesize `text` and return `(samples_f32, sample_rate)`.
    pub fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32)> {
        let url = format!("{}/v1/t2a_v2", self.api_host);

        let body = serde_json::json!({
            "model": if self.model.is_empty() { DEFAULT_MODEL } else { &self.model },
            "text": text,
            "voice_setting": {
                "voice_id": &self.voice_id,
                "speed": self.speed,
                "vol": self.vol,
                "pitch": self.pitch,
                "emotion": if self.emotion.is_empty() { DEFAULT_EMOTION } else { &self.emotion }
            },
            "audio_setting": {
                "sample_rate": if self.sample_rate == 0 { OUTPUT_SAMPLE_RATE } else { self.sample_rate },
                "bitrate": 128000,
                "format": "pcm",
                "channel": 1
            },
            "language_boost": "auto"
        });

        let response = self
            .agent
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(&body)
            .map_err(|e| MiniMaxTtsError::Network(format!("POST {}: {}", url, e)))?;

        let status = response.status();
        let body_text = response
            .into_string()
            .map_err(|e| MiniMaxTtsError::Network(format!("read body: {}", e)))?;

        if status != 200 {
            return Err(MiniMaxTtsError::Api {
                status,
                body: body_text,
            });
        }

        // Parse JSON response
        let json: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| MiniMaxTtsError::Decode(format!("JSON parse: {}", e)))?;

        // Check API error
        if let Some(base_resp) = json.get("base_resp") {
            if let Some(status_code) = base_resp.get("status_code").and_then(|v| v.as_i64()) {
                if status_code != 0 {
                    let msg = base_resp
                        .get("status_msg")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown error");
                    return Err(MiniMaxTtsError::ApiError(format!(
                        "{}: {}",
                        status_code, msg
                    )));
                }
            }
        }

        let audio_hex = json
            .get("data")
            .and_then(|d| d.get("audio"))
            .and_then(|a| a.as_str())
            .ok_or_else(|| MiniMaxTtsError::Decode("missing data.audio in response".into()))?;

        let audio_bytes = hex::decode(audio_hex.trim())
            .map_err(|e| MiniMaxTtsError::Decode(format!("hex decode: {}", e)))?;

        // MiniMax PCM output is 16-bit signed little-endian mono.
        let samples = pcm_i16_le_to_f32(&audio_bytes);
        let sample_rate = if self.sample_rate == 0 {
            OUTPUT_SAMPLE_RATE
        } else {
            self.sample_rate
        };

        Ok((samples, sample_rate))
    }
}

fn pcm_i16_le_to_f32(bytes: &[u8]) -> Vec<f32> {
    let sample_count = bytes.len() / 2;
    let mut samples = Vec::with_capacity(sample_count);
    for i in 0..sample_count {
        let lo = bytes[i * 2] as i16;
        let hi = bytes[i * 2 + 1] as i16;
        let s16 = (hi << 8) | (lo & 0xFF);
        samples.push(s16 as f32 / 32_768.0);
    }
    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pcm_i16_le_to_f32() {
        let bytes: Vec<u8> = vec![0x00, 0x00, 0xFF, 0x7F, 0x00, 0x80];
        let samples = pcm_i16_le_to_f32(&bytes);
        assert!((samples[0]).abs() < 0.0001); // 0
        assert!((samples[1] - 1.0).abs() < 0.0001); // i16::MAX
        assert!((samples[2] + 1.0).abs() < 0.0001); // i16::MIN
    }
}
