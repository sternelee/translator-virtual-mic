//! Minimal OpenAI-compatible chat-completions client used to optionally
//! post-process / translate caption text emitted by the local STT pipeline.
//!
//! The crate is fully synchronous and uses `ureq` instead of `reqwest+tokio`
//! to keep the rest of the workspace tokio-free. One short blocking HTTP call
//! per caption — the caption pipeline runs this off the audio thread, so
//! blocking here is fine.
//!
//! Endpoint shape: any OpenAI-compatible `/chat/completions` server
//! (api.openai.com, Azure, ollama, vLLM, …).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
const REQUEST_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Error)]
pub enum MtError {
    #[error("network error: {0}")]
    Network(String),
    #[error("api error: {0}")]
    Api(String),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    #[error("config error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, MtError>;

#[derive(Debug, Clone)]
pub struct MtClientConfig {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
}

impl MtClientConfig {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_string(),
            api_key: api_key.into(),
            model: DEFAULT_MODEL.to_string(),
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: String,
}

#[derive(Deserialize)]
struct ChatResponseChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Option<Vec<ChatResponseChoice>>,
}

pub struct MtClient {
    config: MtClientConfig,
    agent: ureq::Agent,
}

impl MtClient {
    pub fn new(config: MtClientConfig) -> Result<Self> {
        if config.api_key.trim().is_empty() {
            return Err(MtError::Config("api_key is empty".to_string()));
        }
        if config.endpoint.trim().is_empty() {
            return Err(MtError::Config("endpoint is empty".to_string()));
        }
        if config.model.trim().is_empty() {
            return Err(MtError::Config("model is empty".to_string()));
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build();
        Ok(Self { config, agent })
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Translate `original` into `target_lang`. Returns the translated text,
    /// trimmed. No history, no vocabulary — caption-translation only.
    pub fn translate(&self, original: &str, target_lang: &str) -> Result<String> {
        let original = original.trim();
        if original.is_empty() {
            return Ok(String::new());
        }
        let system = build_system_prompt(target_lang);
        let request = ChatRequest {
            model: &self.config.model,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: system,
                },
                ChatMessage {
                    role: "user",
                    content: original.to_string(),
                },
            ],
            temperature: 0.2,
        };

        let resp = self
            .agent
            .post(&self.config.endpoint)
            .set("Authorization", &format!("Bearer {}", self.config.api_key))
            .set("Content-Type", "application/json")
            .send_json(serde_json::to_value(&request).map_err(|e| {
                MtError::InvalidResponse(format!("serialize request: {e}"))
            })?);

        let resp = match resp {
            Ok(r) => r,
            Err(ureq::Error::Status(status, body)) => {
                let text = body.into_string().unwrap_or_default();
                return Err(MtError::Api(format!("HTTP {status}: {text}")));
            }
            Err(e) => return Err(MtError::Network(e.to_string())),
        };

        let parsed: ChatResponse = resp
            .into_json()
            .map_err(|e| MtError::InvalidResponse(format!("decode body: {e}")))?;

        let choices = parsed
            .choices
            .ok_or_else(|| MtError::InvalidResponse("missing 'choices'".into()))?;
        let first = choices
            .into_iter()
            .next()
            .ok_or_else(|| MtError::InvalidResponse("empty 'choices'".into()))?;
        Ok(first.message.content.trim().to_string())
    }
}

fn build_system_prompt(target_lang: &str) -> String {
    format!(
        "You are a translator. Translate the user's text into {target_lang}. \
         Output ONLY the translated text. No quotes, no explanations, no \
         extra punctuation. Preserve proper nouns and code/technical terms."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_sensible() {
        let c = MtClientConfig::new("sk-test");
        assert_eq!(c.endpoint, DEFAULT_ENDPOINT);
        assert_eq!(c.model, DEFAULT_MODEL);
    }

    #[test]
    fn config_overrides_apply() {
        let c = MtClientConfig::new("sk-test")
            .with_endpoint("http://localhost:8080/v1/chat/completions")
            .with_model("qwen2.5-7b");
        assert_eq!(c.endpoint, "http://localhost:8080/v1/chat/completions");
        assert_eq!(c.model, "qwen2.5-7b");
    }

    #[test]
    fn rejects_empty_api_key() {
        let res = MtClient::new(MtClientConfig::new(""));
        assert!(matches!(res, Err(MtError::Config(_))));
    }

    #[test]
    fn translate_skips_empty_input() {
        let client = MtClient::new(MtClientConfig::new("sk-test")).unwrap();
        assert_eq!(client.translate("   ", "zh").unwrap(), "");
    }

    #[test]
    fn system_prompt_includes_target_lang() {
        let p = build_system_prompt("Japanese");
        assert!(p.contains("Japanese"));
    }
}
