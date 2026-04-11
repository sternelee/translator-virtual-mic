use std::collections::VecDeque;

use common::{AzureVoiceLiveConfig, EngineConfig, EngineError, TranslationProvider};

pub type Result<T> = std::result::Result<T, EngineError>;

#[derive(Clone, Debug, Default)]
pub struct AzureVoiceLiveRuntimeState {
    pub audio_delta_count: u64,
    pub audio_done_count: u64,
    pub transcript_delta_count: u64,
    pub translated_audio_samples: u64,
    pub last_response_id: String,
    pub last_item_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AzureVoiceLiveServerEvent {
    ResponseAudioDelta {
        response_id: String,
        item_id: String,
        audio_bytes: Vec<u8>,
    },
    ResponseAudioDone {
        response_id: String,
        item_id: String,
    },
    ResponseTranscriptDelta {
        response_id: String,
        item_id: String,
        delta: String,
    },
    Error {
        code: String,
        message: String,
    },
    Unknown {
        event_type: String,
    },
}

#[derive(Clone, Debug)]
pub struct AzureVoiceLiveBridge {
    plan: AzureVoiceLivePlan,
    pending_events: VecDeque<String>,
    runtime_state: AzureVoiceLiveRuntimeState,
}

#[derive(Clone, Debug)]
pub struct AzureVoiceLivePlan {
    pub websocket_url: String,
    pub auth_headers: Vec<(String, String)>,
    pub session_update_event: String,
    pub response_create_event: String,
    pub input_audio_commit_event: String,
}

impl AzureVoiceLivePlan {
    pub fn from_config(config: &EngineConfig) -> Result<Self> {
        if config.translation_provider != TranslationProvider::AzureVoiceLive {
            return Err(EngineError::new("translation provider is not azure_voice_live"));
        }

        let azure = config
            .azure_voice_live
            .as_ref()
            .ok_or_else(|| EngineError::new("azure voice live config is missing"))?;

        Ok(Self {
            websocket_url: build_websocket_url(azure),
            auth_headers: build_auth_headers(azure)?,
            session_update_event: build_session_update_event(azure),
            response_create_event: build_response_create_event(),
            input_audio_commit_event: "{\"type\":\"input_audio_buffer.commit\"}".to_string(),
        })
    }
}

impl AzureVoiceLiveBridge {
    pub fn from_config(config: &EngineConfig) -> Result<Self> {
        let plan = AzureVoiceLivePlan::from_config(config)?;
        let mut pending_events = VecDeque::new();
        pending_events.push_back(plan.session_update_event.clone());
        pending_events.push_back(plan.response_create_event.clone());
        Ok(Self {
            plan,
            pending_events,
            runtime_state: AzureVoiceLiveRuntimeState::default(),
        })
    }

    pub fn queue_input_audio_f32(&mut self, samples: &[f32], input_sample_rate: u32) {
        let samples_24k = if input_sample_rate == 24_000 || input_sample_rate == 0 {
            samples.to_vec()
        } else {
            resample_mono_linear(samples, input_sample_rate, 24_000)
        };
        self.pending_events
            .push_back(build_input_audio_append_event_from_f32(&samples_24k));
    }

    pub fn take_next_event(&mut self) -> Option<String> {
        self.pending_events.pop_front()
    }

    pub fn ingest_server_event(&mut self, raw: &str) -> Result<Option<Vec<f32>>> {
        let event = parse_server_event(raw)?;
        Ok(apply_server_event(&mut self.runtime_state, &event))
    }

    pub fn state(&self) -> &AzureVoiceLiveRuntimeState {
        &self.runtime_state
    }

    pub fn plan(&self) -> &AzureVoiceLivePlan {
        &self.plan
    }
}

pub fn build_websocket_url(config: &AzureVoiceLiveConfig) -> String {
    let endpoint = config.endpoint.trim_end_matches('/');
    format!(
        "{endpoint}/voice-live/realtime?api-version={}&model={}",
        percent_encode(&config.api_version),
        percent_encode(&config.model),
    )
}

pub fn build_session_update_event(config: &AzureVoiceLiveConfig) -> String {
    let turn_detection = if config.enable_server_vad {
        r#""turn_detection":{"type":"azure_semantic_vad"},"#
    } else {
        ""
    };

    format!(
        "{{\"type\":\"session.update\",\"session\":{{\"modalities\":[\"audio\"],\"instructions\":\"{}\",\"voice\":\"{}\",\"input_audio_format\":\"pcm16\",\"output_audio_format\":\"pcm16\",\"input_audio_sampling_rate\":24000,{}\"input_audio_noise_reduction\":{{\"type\":\"azure_deep_noise_suppression\"}}}}}}",
        json_escape(&interpreter_instructions(config)),
        json_escape(&config.voice_name),
        turn_detection
    )
}

pub fn build_response_create_event() -> String {
    r#"{"type":"response.create","response":{"modalities":["audio"]}}"#.to_string()
}

pub fn build_input_audio_append_event_from_f32(samples: &[f32]) -> String {
    let pcm_bytes = pcm_f32_to_pcm16le_bytes(samples);
    let audio_base64 = base64_encode(&pcm_bytes);
    format!(
        r#"{{"type":"input_audio_buffer.append","audio":"{}"}}"#,
        audio_base64
    )
}

pub fn parse_server_event(raw: &str) -> Result<AzureVoiceLiveServerEvent> {
    let event_type = extract_json_string(raw, "type").unwrap_or_default();
    match event_type.as_str() {
        "response.audio.delta" => {
            let delta = extract_json_string(raw, "delta").ok_or_else(|| EngineError::new("missing audio delta"))?;
            Ok(AzureVoiceLiveServerEvent::ResponseAudioDelta {
                response_id: extract_json_string(raw, "response_id").unwrap_or_default(),
                item_id: extract_json_string(raw, "item_id").unwrap_or_default(),
                audio_bytes: base64_decode(&delta)?,
            })
        }
        "response.audio.done" => Ok(AzureVoiceLiveServerEvent::ResponseAudioDone {
            response_id: extract_json_string(raw, "response_id").unwrap_or_default(),
            item_id: extract_json_string(raw, "item_id").unwrap_or_default(),
        }),
        "response.audio_transcript.delta" | "response.text.delta" => Ok(AzureVoiceLiveServerEvent::ResponseTranscriptDelta {
            response_id: extract_json_string(raw, "response_id").unwrap_or_default(),
            item_id: extract_json_string(raw, "item_id").unwrap_or_default(),
            delta: extract_json_string(raw, "delta").unwrap_or_default(),
        }),
        "error" => Ok(AzureVoiceLiveServerEvent::Error {
            code: extract_nested_json_string(raw, "error", "code").unwrap_or_default(),
            message: extract_nested_json_string(raw, "error", "message").unwrap_or_else(|| raw.to_string()),
        }),
        _ => Ok(AzureVoiceLiveServerEvent::Unknown { event_type }),
    }
}

pub fn apply_server_event(state: &mut AzureVoiceLiveRuntimeState, event: &AzureVoiceLiveServerEvent) -> Option<Vec<f32>> {
    match event {
        AzureVoiceLiveServerEvent::ResponseAudioDelta {
            response_id,
            item_id,
            audio_bytes,
        } => {
            state.audio_delta_count += 1;
            state.last_response_id = response_id.clone();
            state.last_item_id = item_id.clone();
            let samples = pcm16le_bytes_to_f32(audio_bytes);
            state.translated_audio_samples += samples.len() as u64;
            Some(samples)
        }
        AzureVoiceLiveServerEvent::ResponseAudioDone { response_id, item_id } => {
            state.audio_done_count += 1;
            state.last_response_id = response_id.clone();
            state.last_item_id = item_id.clone();
            None
        }
        AzureVoiceLiveServerEvent::ResponseTranscriptDelta { response_id, item_id, .. } => {
            state.transcript_delta_count += 1;
            state.last_response_id = response_id.clone();
            state.last_item_id = item_id.clone();
            None
        }
        AzureVoiceLiveServerEvent::Error { .. } | AzureVoiceLiveServerEvent::Unknown { .. } => None,
    }
}

pub fn pcm_f32_to_pcm16le_bytes(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len().saturating_mul(2));
    for sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let pcm = (clamped * i16::MAX as f32).round() as i16;
        bytes.extend_from_slice(&pcm.to_le_bytes());
    }
    bytes
}

pub fn pcm16le_bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / i16::MAX as f32)
        .collect()
}

fn interpreter_instructions(config: &AzureVoiceLiveConfig) -> String {
    if config.source_locale == "auto" {
        format!(
            "You are a live interpreter. Continuously detect the speaker language and translate it into {}. Respond only with translated speech in a natural voice. Do not add commentary.",
            config.target_locale
        )
    } else {
        format!(
            "You are a live interpreter. Translate spoken {} into {}. Respond only with translated speech in a natural voice. Do not add commentary.",
            config.source_locale,
            config.target_locale
        )
    }
}

fn build_auth_headers(config: &AzureVoiceLiveConfig) -> Result<Vec<(String, String)>> {
    let api_key = resolve_api_key(config)?;
    Ok(vec![
        ("x-ms-client-request-id".to_string(), "translator-virtual-mic".to_string()),
        ("api-key".to_string(), api_key),
    ])
}

fn resolve_api_key(config: &AzureVoiceLiveConfig) -> Result<String> {
    if !config.api_key.is_empty() {
        return Ok(config.api_key.clone());
    }
    std::env::var(&config.api_key_env).map_err(|_| {
        EngineError::new(format!(
            "azure voice live api key is missing; set {} or provide azure_voice_live_api_key",
            config.api_key_env
        ))
    })
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => vec![byte as char],
            _ => format!("%{byte:02X}").chars().collect::<Vec<_>>(),
        })
        .collect()
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut index = 0usize;

    while index < bytes.len() {
        let b0 = bytes[index];
        let b1 = if index + 1 < bytes.len() { bytes[index + 1] } else { 0 };
        let b2 = if index + 2 < bytes.len() { bytes[index + 2] } else { 0 };

        encoded.push(TABLE[(b0 >> 2) as usize] as char);
        encoded.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);

        if index + 1 < bytes.len() {
            encoded.push(TABLE[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            encoded.push('=');
        }

        if index + 2 < bytes.len() {
            encoded.push(TABLE[(b2 & 0x3F) as usize] as char);
        } else {
            encoded.push('=');
        }

        index += 3;
    }

    encoded
}

fn base64_decode(value: &str) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity((value.len() / 4) * 3);
    let mut chunk = [0u8; 4];
    let mut chunk_len = 0usize;

    for ch in value.bytes() {
        if matches!(ch, b'\r' | b'\n' | b' ') {
            continue;
        }
        chunk[chunk_len] = decode_base64_char(ch)?;
        chunk_len += 1;
        if chunk_len == 4 {
            output.push((chunk[0] << 2) | (chunk[1] >> 4));
            if chunk[2] != 64 {
                output.push((chunk[1] << 4) | (chunk[2] >> 2));
            }
            if chunk[3] != 64 {
                output.push((chunk[2] << 6) | chunk[3]);
            }
            chunk_len = 0;
        }
    }

    if chunk_len != 0 {
        return Err(EngineError::new("invalid base64 payload length"));
    }

    Ok(output)
}

fn decode_base64_char(byte: u8) -> Result<u8> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        b'=' => Ok(64),
        _ => Err(EngineError::new("invalid base64 character")),
    }
}

fn extract_json_string(raw: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\":");
    let start = raw.find(&pattern)? + pattern.len();
    let slice = &raw[start..];
    let quote_start = slice.find('"')?;
    let remainder = &slice[quote_start + 1..];
    let quote_end = remainder.find('"')?;
    Some(remainder[..quote_end].to_string())
}

fn extract_nested_json_string(raw: &str, parent: &str, key: &str) -> Option<String> {
    let parent_pattern = format!("\"{parent}\":");
    let parent_start = raw.find(&parent_pattern)? + parent_pattern.len();
    let parent_slice = &raw[parent_start..];
    let object_start = parent_slice.find('{')?;
    let object_slice = &parent_slice[object_start..];
    extract_json_string(object_slice, key)
}

fn resample_mono_linear(samples: &[f32], input_sample_rate: u32, output_sample_rate: u32) -> Vec<f32> {
    if samples.is_empty() || input_sample_rate == 0 || output_sample_rate == 0 || input_sample_rate == output_sample_rate {
        return samples.to_vec();
    }

    let output_frame_count = ((samples.len() as u64 * output_sample_rate as u64) / input_sample_rate as u64)
        .max(1) as usize;
    let mut output = vec![0.0f32; output_frame_count];
    for output_frame_index in 0..output_frame_count {
        let source_position = (output_frame_index as f64) * (input_sample_rate as f64) / (output_sample_rate as f64);
        let left_index = source_position.floor() as usize;
        let right_index = left_index.min(samples.len().saturating_sub(1)).saturating_add(1).min(samples.len().saturating_sub(1));
        let fraction = (source_position - left_index as f64) as f32;
        let left = samples[left_index];
        let right = samples[right_index];
        output[output_frame_index] = left + ((right - left) * fraction);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{EngineConfig, EngineMode, TranslationProvider};

    fn azure_config() -> EngineConfig {
        EngineConfig::from_json_lossy(
            r#"{
                "translation_provider":"azure_voice_live",
                "azure_voice_live_endpoint":"https://example-resource.cognitiveservices.azure.com",
                "azure_voice_live_api_version":"2025-10-01",
                "azure_voice_live_model":"gpt-realtime",
                "azure_voice_live_api_key":"test-key",
                "azure_voice_live_voice_name":"en-US-Ava:DragonHDLatestNeural",
                "azure_voice_live_source_locale":"auto",
                "azure_voice_live_target_locale":"ja-JP"
            }"#,
        )
    }

    #[test]
    fn builds_websocket_plan() {
        let mut config = azure_config();
        config.mode = EngineMode::Translate;
        config.translation_provider = TranslationProvider::AzureVoiceLive;
        let plan = AzureVoiceLivePlan::from_config(&config).expect("plan");
        assert!(plan.websocket_url.contains("voice-live/realtime"));
        assert!(plan.websocket_url.contains("api-version=2025-10-01"));
        assert!(plan.websocket_url.contains("model=gpt-realtime"));
        assert!(plan.auth_headers.iter().any(|(key, _)| key == "api-key"));
        assert!(plan.session_update_event.contains("\"type\":\"session.update\""));
        assert!(plan.session_update_event.contains("azure_semantic_vad"));
        assert!(plan.session_update_event.contains("ja-JP"));
    }

    #[test]
    fn audio_append_event_contains_base64_audio() {
        let event = build_input_audio_append_event_from_f32(&[0.0, 0.5, -0.5]);
        assert!(event.contains("\"type\":\"input_audio_buffer.append\""));
        assert!(event.contains("\"audio\":\""));
    }

    #[test]
    fn pcm_round_trip_is_reasonable() {
        let source = [0.0f32, 0.25, -0.25, 0.75];
        let bytes = pcm_f32_to_pcm16le_bytes(&source);
        let decoded = pcm16le_bytes_to_f32(&bytes);
        assert_eq!(decoded.len(), source.len());
        assert!(decoded[1] > 0.2);
        assert!(decoded[2] < -0.2);
    }

    #[test]
    fn parses_audio_delta_event() {
        let event = parse_server_event(
            r#"{
                "type":"response.audio.delta",
                "response_id":"resp_1",
                "item_id":"item_1",
                "delta":"AAABAA=="
            }"#,
        )
        .expect("parse event");

        match event {
            AzureVoiceLiveServerEvent::ResponseAudioDelta {
                response_id,
                item_id,
                audio_bytes,
            } => {
                assert_eq!(response_id, "resp_1");
                assert_eq!(item_id, "item_1");
                assert_eq!(audio_bytes.len(), 4);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn applies_audio_delta_to_runtime_state() {
        let event = AzureVoiceLiveServerEvent::ResponseAudioDelta {
            response_id: "resp".to_string(),
            item_id: "item".to_string(),
            audio_bytes: pcm_f32_to_pcm16le_bytes(&[0.0, 0.5]),
        };
        let mut state = AzureVoiceLiveRuntimeState::default();
        let samples = apply_server_event(&mut state, &event).expect("audio samples");
        assert_eq!(state.audio_delta_count, 1);
        assert_eq!(state.last_response_id, "resp");
        assert_eq!(state.last_item_id, "item");
        assert_eq!(samples.len(), 2);
    }

    #[test]
    fn bridge_queues_bootstrap_and_audio_events() {
        let config = azure_config();
        let mut bridge = AzureVoiceLiveBridge::from_config(&config).expect("bridge");
        assert!(bridge.take_next_event().unwrap().contains("\"type\":\"session.update\""));
        assert!(bridge.take_next_event().unwrap().contains("\"type\":\"response.create\""));
        bridge.queue_input_audio_f32(&[0.0, 0.5, -0.5], 48_000);
        assert!(bridge.take_next_event().unwrap().contains("\"type\":\"input_audio_buffer.append\""));
    }
}
