use std::sync::Arc;

use audio_core::{build_frame, SampleRingBuffer};
use common::{EngineConfig, EngineError, EngineMode, Result};
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
                let dropped = self.output_ring.push_frame(&frame)?;
                if dropped > 0 {
                    self.metrics.record_overflow();
                }
                if let Some(shared_output) = &self.shared_output {
                    shared_output.write_frame(&frame)?;
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
}
