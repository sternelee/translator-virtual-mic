use std::collections::VecDeque;

use common::{EngineConfig, EngineError, OpenAIRealtimeConfig, TranslationProvider};

pub type Result<T> = std::result::Result<T, EngineError>;

#[derive(Clone, Debug, Default)]
pub struct OpenAIRealtimeRuntimeState {
    pub audio_delta_count: u64,
    pub audio_done_count: u64,
    pub transcript_delta_count: u64,
    pub translated_audio_samples: u64,
    pub last_response_id: String,
    pub last_item_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OpenAIRealtimeServerEvent {
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
pub struct OpenAIRealtimeBridge {
    plan: OpenAIRealtimePlan,
    pending_events: VecDeque<String>,
    runtime_state: OpenAIRealtimeRuntimeState,
}

#[derive(Clone, Debug)]
pub struct OpenAIRealtimePlan {
    pub websocket_url: String,
    pub auth_headers: Vec<(String, String)>,
    pub session_update_event: String,
}

impl OpenAIRealtimePlan {
    pub fn from_config(config: &EngineConfig) -> Result<Self> {
        if config.translation_provider != TranslationProvider::OpenAIRealtime {
            return Err(EngineError::new(
                "translation provider is not openai_realtime",
            ));
        }

        let openai = config
            .openai_realtime
            .as_ref()
            .ok_or_else(|| EngineError::new("openai realtime config is missing"))?;

        Ok(Self {
            websocket_url: build_websocket_url(openai),
            auth_headers: build_auth_headers(openai)?,
            session_update_event: build_session_update_event(openai),
        })
    }
}

impl OpenAIRealtimeBridge {
    pub fn from_config(config: &EngineConfig) -> Result<Self> {
        let plan = OpenAIRealtimePlan::from_config(config)?;
        let mut pending_events = VecDeque::new();
        pending_events.push_back(plan.session_update_event.clone());
        Ok(Self {
            plan,
            pending_events,
            runtime_state: OpenAIRealtimeRuntimeState::default(),
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

    pub fn state(&self) -> &OpenAIRealtimeRuntimeState {
        &self.runtime_state
    }

    pub fn plan(&self) -> &OpenAIRealtimePlan {
        &self.plan
    }
}

pub fn build_websocket_url(config: &OpenAIRealtimeConfig) -> String {
    let endpoint = config.endpoint.trim_end_matches('/');
    format!("{endpoint}?model={}", percent_encode(&config.model),)
}

pub fn build_session_update_event(config: &OpenAIRealtimeConfig) -> String {
    let turn_detection = if config.enable_server_vad {
        r#","turn_detection":{"type":"semantic_vad"}"#
    } else {
        r#","turn_detection":null"#
    };

    format!(
        "{{\"type\":\"session.update\",\"session\":{{\"type\":\"realtime\",\"model\":\"{}\",\"output_modalities\":[\"audio\"],\"audio\":{{\"input\":{{\"format\":{{\"type\":\"audio/pcm\",\"rate\":24000}}{}}},\"output\":{{\"format\":{{\"type\":\"audio/pcm\"}},\"voice\":\"{}\"}}}},\"instructions\":\"{}\"}}}}",
        json_escape(&config.model),
        turn_detection,
        json_escape(&config.voice_name),
        json_escape(&interpreter_instructions(config)),
    )
}

pub fn build_input_audio_append_event_from_f32(samples: &[f32]) -> String {
    let pcm_bytes = pcm_f32_to_pcm16le_bytes(samples);
    let audio_base64 = base64_encode(&pcm_bytes);
    format!(
        r#"{{"type":"input_audio_buffer.append","audio":"{}"}}"#,
        audio_base64
    )
}

pub fn parse_server_event(raw: &str) -> Result<OpenAIRealtimeServerEvent> {
    let event_type = extract_json_string(raw, "type").unwrap_or_default();
    match event_type.as_str() {
        "response.audio.delta" | "response.output_audio.delta" => {
            let delta = extract_json_string(raw, "delta")
                .ok_or_else(|| EngineError::new("missing audio delta"))?;
            Ok(OpenAIRealtimeServerEvent::ResponseAudioDelta {
                response_id: extract_json_string(raw, "response_id")
                    .or_else(|| extract_nested_json_string(raw, "response", "id"))
                    .unwrap_or_default(),
                item_id: extract_json_string(raw, "item_id").unwrap_or_default(),
                audio_bytes: base64_decode(&delta)?,
            })
        }
        "response.audio.done" | "response.output_audio.done" => {
            Ok(OpenAIRealtimeServerEvent::ResponseAudioDone {
                response_id: extract_json_string(raw, "response_id")
                    .or_else(|| extract_nested_json_string(raw, "response", "id"))
                    .unwrap_or_default(),
                item_id: extract_json_string(raw, "item_id").unwrap_or_default(),
            })
        }
        "response.audio_transcript.delta"
        | "response.output_audio_transcript.delta"
        | "response.text.delta"
        | "response.output_text.delta" => Ok(OpenAIRealtimeServerEvent::ResponseTranscriptDelta {
            response_id: extract_json_string(raw, "response_id")
                .or_else(|| extract_nested_json_string(raw, "response", "id"))
                .unwrap_or_default(),
            item_id: extract_json_string(raw, "item_id").unwrap_or_default(),
            delta: extract_json_string(raw, "delta").unwrap_or_default(),
        }),
        "error" => Ok(OpenAIRealtimeServerEvent::Error {
            code: extract_nested_json_string(raw, "error", "code").unwrap_or_default(),
            message: extract_nested_json_string(raw, "error", "message")
                .unwrap_or_else(|| raw.to_string()),
        }),
        _ => Ok(OpenAIRealtimeServerEvent::Unknown { event_type }),
    }
}

pub fn apply_server_event(
    state: &mut OpenAIRealtimeRuntimeState,
    event: &OpenAIRealtimeServerEvent,
) -> Option<Vec<f32>> {
    match event {
        OpenAIRealtimeServerEvent::ResponseAudioDelta {
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
        OpenAIRealtimeServerEvent::ResponseAudioDone {
            response_id,
            item_id,
        } => {
            state.audio_done_count += 1;
            state.last_response_id = response_id.clone();
            state.last_item_id = item_id.clone();
            None
        }
        OpenAIRealtimeServerEvent::ResponseTranscriptDelta {
            response_id,
            item_id,
            ..
        } => {
            state.transcript_delta_count += 1;
            state.last_response_id = response_id.clone();
            state.last_item_id = item_id.clone();
            None
        }
        OpenAIRealtimeServerEvent::Error { .. } | OpenAIRealtimeServerEvent::Unknown { .. } => None,
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

fn interpreter_instructions(config: &OpenAIRealtimeConfig) -> String {
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

fn build_auth_headers(config: &OpenAIRealtimeConfig) -> Result<Vec<(String, String)>> {
    let api_key = resolve_api_key(config)?;
    Ok(vec![(
        "Authorization".to_string(),
        format!("Bearer {api_key}"),
    )])
}

fn resolve_api_key(config: &OpenAIRealtimeConfig) -> Result<String> {
    if !config.api_key.is_empty() {
        return Ok(config.api_key.clone());
    }
    std::env::var(&config.api_key_env).map_err(|_| {
        EngineError::new(format!(
            "openai realtime api key is missing; set {} or provide openai_realtime_api_key",
            config.api_key_env
        ))
    })
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
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
        let b1 = if index + 1 < bytes.len() {
            bytes[index + 1]
        } else {
            0
        };
        let b2 = if index + 2 < bytes.len() {
            bytes[index + 2]
        } else {
            0
        };

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

fn resample_mono_linear(
    samples: &[f32],
    input_sample_rate: u32,
    output_sample_rate: u32,
) -> Vec<f32> {
    if samples.is_empty()
        || input_sample_rate == 0
        || output_sample_rate == 0
        || input_sample_rate == output_sample_rate
    {
        return samples.to_vec();
    }

    let output_frame_count = ((samples.len() as u64 * output_sample_rate as u64)
        / input_sample_rate as u64)
        .max(1) as usize;
    let mut output = vec![0.0f32; output_frame_count];
    for (output_frame_index, output_sample) in output.iter_mut().enumerate() {
        let source_position =
            (output_frame_index as f64) * (input_sample_rate as f64) / (output_sample_rate as f64);
        let left_index = source_position.floor() as usize;
        let right_index = left_index
            .min(samples.len().saturating_sub(1))
            .saturating_add(1)
            .min(samples.len().saturating_sub(1));
        let fraction = (source_position - left_index as f64) as f32;
        let left = samples[left_index];
        let right = samples[right_index];
        *output_sample = left + ((right - left) * fraction);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{EngineConfig, EngineMode, TranslationProvider};

    fn openai_config() -> EngineConfig {
        EngineConfig::from_json_lossy(
            r#"{
                "translation_provider":"openai_realtime",
                "openai_realtime_endpoint":"wss://api.openai.com/v1/realtime",
                "openai_realtime_model":"gpt-realtime",
                "openai_realtime_api_key":"test-key",
                "openai_realtime_voice_name":"marin",
                "openai_realtime_source_locale":"auto",
                "openai_realtime_target_locale":"ja-JP"
            }"#,
        )
    }

    #[test]
    fn builds_websocket_plan() {
        let mut config = openai_config();
        config.mode = EngineMode::Translate;
        config.translation_provider = TranslationProvider::OpenAIRealtime;
        let plan = OpenAIRealtimePlan::from_config(&config).expect("plan");
        assert_eq!(
            plan.websocket_url,
            "wss://api.openai.com/v1/realtime?model=gpt-realtime"
        );
        assert!(plan
            .auth_headers
            .iter()
            .any(|(key, _)| key == "Authorization"));
        assert!(plan
            .session_update_event
            .contains("\"type\":\"session.update\""));
        assert!(plan
            .session_update_event
            .contains("\"turn_detection\":{\"type\":\"semantic_vad\"}"));
        assert!(plan
            .session_update_event
            .contains("\"output_modalities\":[\"audio\"]"));
        assert!(plan.session_update_event.contains("\"voice\":\"marin\""));
        assert!(plan.session_update_event.contains("ja-JP"));
    }

    #[test]
    fn parses_output_audio_delta_event() {
        let event = parse_server_event(
            r#"{
                "type":"response.output_audio.delta",
                "response_id":"resp_1",
                "item_id":"item_1",
                "delta":"AAABAA=="
            }"#,
        )
        .expect("parse event");

        match event {
            OpenAIRealtimeServerEvent::ResponseAudioDelta {
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
    fn bridge_queues_bootstrap_and_audio_events() {
        let config = openai_config();
        let mut bridge = OpenAIRealtimeBridge::from_config(&config).expect("bridge");
        assert!(bridge
            .take_next_event()
            .unwrap()
            .contains("\"type\":\"session.update\""));
        bridge.queue_input_audio_f32(&[0.0, 0.5, -0.5], 48_000);
        assert!(bridge
            .take_next_event()
            .unwrap()
            .contains("\"type\":\"input_audio_buffer.append\""));
    }
}
