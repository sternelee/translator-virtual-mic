use std::sync::Arc;

use audio_core::{build_frame, SampleRingBuffer};
use common::{AudioFrame, EngineConfig, EngineError, EngineMode, Result};
use metrics::EngineMetrics;
use output_bridge::{SharedBufferSnapshot, SharedOutputBuffer};

pub struct EngineSession {
    config: EngineConfig,
    metrics: Arc<EngineMetrics>,
    input_ring: Arc<SampleRingBuffer>,
    output_ring: Arc<SampleRingBuffer>,
    shared_output: Option<Arc<SharedOutputBuffer>>,
    running: bool,
}

impl EngineSession {
    pub fn new(config: EngineConfig) -> Self {
        let channels = config.channels.max(1);
        let input_ring = Arc::new(SampleRingBuffer::new(48_000, channels, config.input_sample_rate));
        let output_ring = Arc::new(SampleRingBuffer::new(48_000, channels, config.output_sample_rate));
        Self {
            config,
            metrics: Arc::new(EngineMetrics::default()),
            input_ring,
            output_ring,
            shared_output: None,
            running: false,
        }
    }

    pub fn start(&mut self) {
        self.running = true;
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub fn set_target_language(&mut self, lang: &str) {
        self.config.target_language = lang.to_string();
    }

    pub fn set_mode(&mut self, mode: EngineMode) {
        self.config.mode = mode;
    }

    pub fn enable_shared_output(&mut self, capacity_frames: usize, channels: u16, sample_rate: u32) -> Result<()> {
        self.shared_output = Some(Arc::new(SharedOutputBuffer::new(capacity_frames, channels, sample_rate)?));
        Ok(())
    }

    pub fn read_shared_output_pcm(&self, out_samples: &mut [f32], channels: u16) -> Result<(usize, u64)> {
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
            EngineMode::Translate | EngineMode::CaptionOnly | EngineMode::MuteOnFailure => {}
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
            ((self.output_ring.available_frames() as u64) * 1000) / u64::from(self.output_ring.sample_rate())
        };
        self.metrics.to_json(depth_ms)
    }

    pub fn current_mode(&self) -> i32 {
        self.config.mode.as_i32()
    }

    pub fn target_language(&self) -> &str {
        &self.config.target_language
    }

    fn output_frame_from_input(&self, frame: &AudioFrame) -> AudioFrame {
        let target_sample_rate = self.config.output_sample_rate;
        if frame.sample_rate == target_sample_rate {
            return frame.clone();
        }

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
    }
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
    if input_sample_rate == 0 || output_sample_rate == 0 || input_sample_rate == output_sample_rate {
        return samples.to_vec();
    }

    let output_frame_count = ((frame_count as u64 * output_sample_rate as u64) / input_sample_rate as u64)
        .max(1) as usize;
    let mut output = vec![0.0f32; output_frame_count.saturating_mul(channel_count)];

    for output_frame_index in 0..output_frame_count {
        let source_position = (output_frame_index as f64) * (input_sample_rate as f64) / (output_sample_rate as f64);
        let left_frame_index = source_position.floor() as usize;
        let right_frame_index = left_frame_index.min(frame_count.saturating_sub(1)).saturating_add(1).min(frame_count.saturating_sub(1));
        let fraction = (source_position - left_frame_index as f64) as f32;

        for channel_index in 0..channel_count {
            let left = samples[left_frame_index.saturating_mul(channel_count) + channel_index];
            let right = samples[right_frame_index.saturating_mul(channel_count) + channel_index];
            output[output_frame_index.saturating_mul(channel_count) + channel_index] = left + ((right - left) * fraction);
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
        session.start();

        let input_samples: Vec<f32> = vec![0.0, 0.25, 0.5, 0.75, 1.0, 0.5, 0.0, -0.5, -1.0, -0.5];
        session
            .push_input_pcm(&input_samples, input_samples.len(), 1, 16_000, 123)
            .expect("push input");

        let mut out = vec![0.0f32; 32];
        let (frames_read, _) = session.read_shared_output_pcm(&mut out, 1).expect("read shared output");

        assert!(frames_read > input_samples.len());
        assert!(out.iter().take(frames_read).any(|sample| sample.abs() > 0.0));
    }
}
