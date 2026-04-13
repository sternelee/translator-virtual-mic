pub mod azure_voice_live;
pub mod openai_realtime;

use std::sync::Arc;

use audio_core::{build_frame, SampleRingBuffer};
use common::{AudioFrame, EngineConfig, EngineError, EngineMode, Result, TranslationProvider};
use metrics::EngineMetrics;
use output_bridge::{SharedBufferSnapshot, SharedOutputBuffer};

pub use azure_voice_live::{AzureVoiceLiveBridge, AzureVoiceLivePlan, AzureVoiceLiveRuntimeState};
pub use openai_realtime::{OpenAIRealtimeBridge, OpenAIRealtimePlan, OpenAIRealtimeRuntimeState};

pub struct EngineSession {
    config: EngineConfig,
    metrics: Arc<EngineMetrics>,
    input_ring: Arc<SampleRingBuffer>,
    output_ring: Arc<SampleRingBuffer>,
    shared_output: Option<Arc<SharedOutputBuffer>>,
    azure_voice_live: Option<AzureVoiceLiveBridge>,
    openai_realtime: Option<OpenAIRealtimeBridge>,
    running: bool,
}

impl EngineSession {
    pub fn new(config: EngineConfig) -> Self {
        let channels = config.channels.max(1);
        let input_ring = Arc::new(SampleRingBuffer::new(
            48_000,
            channels,
            config.input_sample_rate,
        ));
        let output_ring = Arc::new(SampleRingBuffer::new(
            48_000,
            channels,
            config.output_sample_rate,
        ));
        Self {
            config,
            metrics: Arc::new(EngineMetrics::default()),
            input_ring,
            output_ring,
            shared_output: None,
            azure_voice_live: None,
            openai_realtime: None,
            running: false,
        }
    }

    pub fn start(&mut self) -> Result<()> {
        self.sync_translation_bridge()?;
        self.running = true;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running = false;
        self.azure_voice_live = None;
        self.openai_realtime = None;
    }

    pub fn set_target_language(&mut self, lang: &str) {
        self.config.target_language = lang.to_string();
    }

    pub fn set_mode(&mut self, mode: EngineMode) {
        self.config.mode = mode;
        if self.running {
            let _ = self.sync_translation_bridge();
        }
    }

    pub fn enable_shared_output(
        &mut self,
        capacity_frames: usize,
        channels: u16,
        sample_rate: u32,
    ) -> Result<()> {
        self.shared_output = Some(Arc::new(SharedOutputBuffer::new(
            capacity_frames,
            channels,
            sample_rate,
        )?));
        Ok(())
    }

    pub fn read_shared_output_pcm(
        &self,
        out_samples: &mut [f32],
        channels: u16,
    ) -> Result<(usize, u64)> {
        let shared_output = self
            .shared_output
            .as_ref()
            .ok_or_else(|| EngineError::new("shared output is not enabled"))?;
        shared_output.read_into(out_samples, channels)
    }

    pub fn shared_output_snapshot(&self) -> Option<SharedBufferSnapshot> {
        self.shared_output.as_ref().map(|buffer| buffer.snapshot())
    }

    pub fn shared_output_path(&self) -> Option<String> {
        self.shared_output.as_ref().map(|buffer| buffer.file_path())
    }

    pub fn push_input_pcm(
        &mut self,
        samples: &[f32],
        frame_count: usize,
        channels: u16,
        sample_rate: u32,
        timestamp_ns: u64,
    ) -> Result<()> {
        if !self.running {
            return Err(EngineError::new("engine is not running"));
        }

        let frame = build_frame(samples, frame_count, channels, sample_rate, timestamp_ns);
        let dropped = self.input_ring.push_frame(&frame)?;
        if dropped > 0 {
            self.metrics.record_overflow();
        }

        self.metrics
            .record_capture(frame_count as u64, frame.data.len() as u64);

        match self.config.mode {
            EngineMode::Bypass | EngineMode::FallbackToBypass => {
                let output_frame = self.output_frame_from_input(&frame);
                let dropped = self.output_ring.push_frame(&output_frame)?;
                if dropped > 0 {
                    self.metrics.record_overflow();
                }
                if let Some(shared_output) = &self.shared_output {
                    shared_output.write_frame(&output_frame)?;
                }
            }
            EngineMode::Translate => {
                if let Some(bridge) = &mut self.azure_voice_live {
                    bridge.queue_input_audio_f32(&frame.data, frame.sample_rate);
                }
                if let Some(bridge) = &mut self.openai_realtime {
                    bridge.queue_input_audio_f32(&frame.data, frame.sample_rate);
                }
            }
            EngineMode::CaptionOnly | EngineMode::MuteOnFailure => {}
        }

        Ok(())
    }

    pub fn pull_output_pcm(
        &self,
        out_samples: &mut [f32],
        channels: u16,
        sample_rate: u32,
    ) -> Result<u64> {
        if channels == 0 || sample_rate == 0 {
            return Err(EngineError::new("invalid output format"));
        }

        let requested_frames = out_samples.len() / usize::from(channels);
        let actual_frames = self.output_ring.pop_into(out_samples, channels);
        if actual_frames < requested_frames {
            self.metrics.record_underrun();
        }

        self.metrics
            .record_output(requested_frames as u64, out_samples.len() as u64);

        Ok(self.output_ring.last_timestamp_ns())
    }

    pub fn metrics_json(&self) -> String {
        let depth_ms = if self.output_ring.sample_rate() == 0 {
            0
        } else {
            ((self.output_ring.available_frames() as u64) * 1000)
                / u64::from(self.output_ring.sample_rate())
        };
        self.metrics.to_json(depth_ms)
    }

    pub fn current_mode(&self) -> i32 {
        self.config.mode.as_i32()
    }

    pub fn target_language(&self) -> &str {
        &self.config.target_language
    }

    pub fn azure_voice_live_plan(&self) -> Result<AzureVoiceLivePlan> {
        azure_voice_live::AzureVoiceLivePlan::from_config(&self.config)
    }

    pub fn take_next_azure_voice_live_event(&mut self) -> Result<Option<String>> {
        if let Some(bridge) = self.azure_voice_live.as_mut() {
            return Ok(bridge.take_next_event());
        }
        if let Some(bridge) = self.openai_realtime.as_mut() {
            return Ok(bridge.take_next_event());
        }
        Err(EngineError::new("translation bridge is not active"))
    }

    pub fn ingest_azure_voice_live_server_event(&mut self, raw_event: &str) -> Result<usize> {
        if let Some(bridge) = self.openai_realtime.as_mut() {
            let (samples, timestamp_ns) = {
                let Some(samples) = bridge.ingest_server_event(raw_event)? else {
                    return Ok(0);
                };
                (samples, bridge.state().translated_audio_samples)
            };
            return self.push_translated_output(samples, timestamp_ns);
        }

        let (samples, timestamp_ns) = {
            let bridge = self
                .azure_voice_live
                .as_mut()
                .ok_or_else(|| EngineError::new("translation bridge is not active"))?;
            let Some(samples) = bridge.ingest_server_event(raw_event)? else {
                return Ok(0);
            };
            (samples, bridge.state().translated_audio_samples)
        };

        self.push_translated_output(samples, timestamp_ns)
    }

    fn push_translated_output(&mut self, samples: Vec<f32>, timestamp_ns: u64) -> Result<usize> {
        let output_data = if self.config.output_sample_rate == 24_000 {
            samples
        } else {
            resample_interleaved_linear(
                &samples,
                samples.len(),
                1,
                24_000,
                self.config.output_sample_rate,
            )
        };
        let output_frame = AudioFrame {
            timestamp_ns,
            sample_rate: self.config.output_sample_rate,
            channels: 1,
            data: output_data,
        };

        let dropped = self.output_ring.push_frame(&output_frame)?;
        if dropped > 0 {
            self.metrics.record_overflow();
        }
        if let Some(shared_output) = &self.shared_output {
            shared_output.write_frame(&output_frame)?;
        }
        Ok(output_frame.frames())
    }

    pub fn azure_voice_live_state(&self) -> Option<&AzureVoiceLiveRuntimeState> {
        self.azure_voice_live
            .as_ref()
            .map(AzureVoiceLiveBridge::state)
    }

    pub fn openai_realtime_state(&self) -> Option<&OpenAIRealtimeRuntimeState> {
        self.openai_realtime
            .as_ref()
            .map(OpenAIRealtimeBridge::state)
    }

    fn sync_translation_bridge(&mut self) -> Result<()> {
        self.azure_voice_live = None;
        self.openai_realtime = None;

        if self.config.mode != EngineMode::Translate {
            return Ok(());
        }

        match self.config.translation_provider {
            TranslationProvider::AzureVoiceLive => {
                self.azure_voice_live = Some(AzureVoiceLiveBridge::from_config(&self.config)?);
            }
            TranslationProvider::OpenAIRealtime => {
                self.openai_realtime = Some(OpenAIRealtimeBridge::from_config(&self.config)?);
            }
            TranslationProvider::None => {}
        }
        Ok(())
    }

    fn output_frame_from_input(&self, frame: &AudioFrame) -> AudioFrame {
        let target_sample_rate = self.config.output_sample_rate;
        let mut output_frame = if frame.sample_rate == target_sample_rate {
            frame.clone()
        } else {
            AudioFrame {
                timestamp_ns: frame.timestamp_ns,
                sample_rate: target_sample_rate,
                channels: frame.channels,
                data: resample_interleaved_linear(
                    &frame.data,
                    frame.frames(),
                    frame.channels,
                    frame.sample_rate,
                    target_sample_rate,
                ),
            }
        };
        apply_gain_and_limiter(
            &mut output_frame.data,
            self.config.input_gain_db,
            self.config.limiter_threshold_db,
        );
        output_frame
    }
}

fn apply_gain_and_limiter(samples: &mut [f32], input_gain_db: f32, limiter_threshold_db: f32) {
    let gain = db_to_linear(input_gain_db);
    let threshold = db_to_linear(limiter_threshold_db).clamp(0.05, 0.999);
    for sample in samples.iter_mut() {
        let gained = *sample * gain;
        *sample = soft_limit(gained, threshold);
    }
}

fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn soft_limit(sample: f32, threshold: f32) -> f32 {
    let abs = sample.abs();
    if abs <= threshold {
        return sample;
    }

    let sign = sample.signum();
    let normalized = (abs - threshold) / (1.0 - threshold);
    let compressed = threshold + (1.0 - threshold) * normalized.tanh();
    sign * compressed.min(1.0)
}

fn resample_interleaved_linear(
    samples: &[f32],
    frame_count: usize,
    channels: u16,
    input_sample_rate: u32,
    output_sample_rate: u32,
) -> Vec<f32> {
    let channel_count = usize::from(channels.max(1));
    if frame_count == 0 || samples.is_empty() {
        return Vec::new();
    }
    if input_sample_rate == 0 || output_sample_rate == 0 || input_sample_rate == output_sample_rate
    {
        return samples.to_vec();
    }

    let output_frame_count = ((frame_count as u64 * output_sample_rate as u64)
        / input_sample_rate as u64)
        .max(1) as usize;
    let mut output = vec![0.0f32; output_frame_count.saturating_mul(channel_count)];

    for output_frame_index in 0..output_frame_count {
        let source_position =
            (output_frame_index as f64) * (input_sample_rate as f64) / (output_sample_rate as f64);
        let left_frame_index = source_position.floor() as usize;
        let right_frame_index = left_frame_index
            .min(frame_count.saturating_sub(1))
            .saturating_add(1)
            .min(frame_count.saturating_sub(1));
        let fraction = (source_position - left_frame_index as f64) as f32;

        for channel_index in 0..channel_count {
            let left = samples[left_frame_index.saturating_mul(channel_count) + channel_index];
            let right = samples[right_frame_index.saturating_mul(channel_count) + channel_index];
            output[output_frame_index.saturating_mul(channel_count) + channel_index] =
                left + ((right - left) * fraction);
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_resamples_input_for_shared_output() {
        let mut session = EngineSession::new(EngineConfig {
            mode: EngineMode::Bypass,
            output_sample_rate: 48_000,
            ..EngineConfig::default()
        });
        session
            .enable_shared_output(960, 1, 48_000)
            .expect("enable shared output");
        session.start().expect("start");

        let input_samples: Vec<f32> = vec![0.0, 0.25, 0.5, 0.75, 1.0, 0.5, 0.0, -0.5, -1.0, -0.5];
        session
            .push_input_pcm(&input_samples, input_samples.len(), 1, 16_000, 123)
            .expect("push input");

        let mut out = vec![0.0f32; 32];
        let (frames_read, _) = session
            .read_shared_output_pcm(&mut out, 1)
            .expect("read shared output");

        assert!(frames_read > input_samples.len());
        assert!(out
            .iter()
            .take(frames_read)
            .any(|sample| sample.abs() > 0.0));
    }

    #[test]
    fn output_processing_applies_gain_and_limiter() {
        let mut frame = AudioFrame {
            timestamp_ns: 1,
            sample_rate: 48_000,
            channels: 1,
            data: vec![0.25, 0.5, 0.75, 1.0],
        };

        apply_gain_and_limiter(&mut frame.data, 6.0, -6.0);

        assert!(frame.data[0] > 0.25);
        assert!(frame.data[3] < 1.0);
        assert!(frame.data.iter().all(|sample| sample.abs() <= 1.0));
    }

    #[test]
    fn translate_mode_queues_and_ingests_azure_events() {
        let mut session = EngineSession::new(EngineConfig::from_json_lossy(
            r#"{
                "translation_provider":"azure_voice_live",
                "azure_voice_live_endpoint":"https://example-resource.cognitiveservices.azure.com",
                "azure_voice_live_api_version":"2025-10-01",
                "azure_voice_live_model":"gpt-realtime",
                "azure_voice_live_api_key":"test-key",
                "azure_voice_live_voice_name":"en-US-Ava:DragonHDLatestNeural",
                "azure_voice_live_source_locale":"auto",
                "azure_voice_live_target_locale":"en-US"
            }"#,
        ));
        session.set_mode(EngineMode::Translate);
        session
            .enable_shared_output(960, 1, 48_000)
            .expect("enable shared output");
        session.start().expect("start");

        let bootstrap = session
            .take_next_azure_voice_live_event()
            .expect("take event")
            .expect("session update");
        assert!(bootstrap.contains("\"type\":\"session.update\""));
        let response_create = session
            .take_next_azure_voice_live_event()
            .expect("take event")
            .expect("response create");
        assert!(response_create.contains("\"type\":\"response.create\""));

        session
            .push_input_pcm(&[0.0, 0.25, -0.25, 0.5], 4, 1, 48_000, 1)
            .expect("push input");
        let append_event = session
            .take_next_azure_voice_live_event()
            .expect("take event")
            .expect("audio append");
        assert!(append_event.contains("\"type\":\"input_audio_buffer.append\""));

        let event = r#"{
            "type":"response.audio.delta",
            "response_id":"resp_1",
            "item_id":"item_1",
            "delta":"AAABAA=="
        }"#;
        let frames = session
            .ingest_azure_voice_live_server_event(event)
            .expect("ingest event");
        assert!(frames > 0);
        let state = session.azure_voice_live_state().expect("state");
        assert_eq!(state.audio_delta_count, 1);
    }

    #[test]
    fn translate_mode_queues_and_ingests_openai_events() {
        let mut session = EngineSession::new(EngineConfig::from_json_lossy(
            r#"{
                "translation_provider":"openai_realtime",
                "openai_realtime_endpoint":"wss://api.openai.com/v1/realtime",
                "openai_realtime_model":"gpt-realtime",
                "openai_realtime_api_key":"test-key",
                "openai_realtime_voice_name":"alloy",
                "openai_realtime_source_locale":"auto",
                "openai_realtime_target_locale":"en-US"
            }"#,
        ));
        session.set_mode(EngineMode::Translate);
        session
            .enable_shared_output(960, 1, 48_000)
            .expect("enable shared output");
        session.start().expect("start");

        let bootstrap = session
            .take_next_azure_voice_live_event()
            .expect("take event")
            .expect("session update");
        assert!(bootstrap.contains("\"type\":\"session.update\""));

        session
            .push_input_pcm(&[0.0, 0.25, -0.25, 0.5], 4, 1, 48_000, 1)
            .expect("push input");
        let append_event = session
            .take_next_azure_voice_live_event()
            .expect("take event")
            .expect("audio append");
        assert!(append_event.contains("\"type\":\"input_audio_buffer.append\""));

        let event = r#"{
            "type":"response.output_audio.delta",
            "response_id":"resp_1",
            "item_id":"item_1",
            "delta":"AAABAA=="
        }"#;
        let frames = session
            .ingest_azure_voice_live_server_event(event)
            .expect("ingest event");
        assert!(frames > 0);
        let state = session.openai_realtime_state().expect("state");
        assert_eq!(state.audio_delta_count, 1);
    }
}
