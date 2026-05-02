use std::path::Path;

use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};

use crate::{Result, SttError};

#[derive(Debug, Clone)]
pub struct VadConfig {
    pub model_path: String,
    pub threshold: f32,
    pub min_silence_duration: f32,
    pub min_speech_duration: f32,
    pub max_speech_duration: f32,
    pub window_size: i32,
    pub sample_rate: i32,
    pub num_threads: i32,
    /// VAD internal ring buffer size, in seconds.
    pub buffer_seconds: f32,
}

impl VadConfig {
    pub fn new(model_path: impl Into<String>) -> Self {
        Self {
            model_path: model_path.into(),
            threshold: 0.3,
            min_silence_duration: 0.2,
            min_speech_duration: 0.1,
            max_speech_duration: 8.0,
            window_size: 512,
            sample_rate: 16_000,
            num_threads: 1,
            buffer_seconds: 60.0,
        }
    }

    pub fn with_threshold(mut self, t: f32) -> Self {
        self.threshold = t;
        self
    }
}

/// Wrapper around `sherpa_onnx::VoiceActivityDetector`. Pushes 16 kHz mono
/// `f32` samples and pulls completed speech segments.
///
/// sherpa-onnx Silero VAD requires `accept_waveform` inputs to be a multiple
/// of `window_size` (default 512). This wrapper buffers incoming samples and
/// only calls `accept_waveform` once a full window is available.
pub struct Vad {
    inner: VoiceActivityDetector,
    window_size: usize,
    pending: Vec<f32>,
}

unsafe impl Send for Vad {}
unsafe impl Sync for Vad {}

impl Vad {
    pub fn new(config: &VadConfig) -> Result<Self> {
        if !Path::new(&config.model_path).exists() {
            return Err(SttError::Model(format!(
                "Silero VAD model not found: {}",
                config.model_path
            )));
        }
        let mut model_cfg = VadModelConfig::default();
        model_cfg.silero_vad = SileroVadModelConfig {
            model: Some(config.model_path.clone()),
            threshold: config.threshold,
            min_silence_duration: config.min_silence_duration,
            min_speech_duration: config.min_speech_duration,
            window_size: config.window_size,
            max_speech_duration: config.max_speech_duration,
        };
        model_cfg.sample_rate = config.sample_rate;
        model_cfg.num_threads = config.num_threads;
        model_cfg.debug = false;
        model_cfg.provider = None;

        let window_size = config.window_size as usize;
        let inner = VoiceActivityDetector::create(&model_cfg, config.buffer_seconds)
            .ok_or_else(|| SttError::Backend("Failed to create VoiceActivityDetector".into()))?;
        Ok(Self {
            inner,
            window_size,
            pending: Vec::new(),
        })
    }

    /// Push 16 kHz mono `f32` samples and drain any newly completed speech
    /// segments. Internally buffers samples until a full `window_size` block
    /// is ready before calling `accept_waveform` (sherpa-onnx requirement).
    pub fn push(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        if samples.is_empty() {
            return Vec::new();
        }
        self.pending.extend_from_slice(samples);
        let mut windows_processed = 0usize;
        while self.pending.len() >= self.window_size {
            let chunk: Vec<f32> = self.pending.drain(..self.window_size).collect();
            self.inner.accept_waveform(&chunk);
            windows_processed += 1;
        }
        if windows_processed > 0 {
            eprintln!(
                "[vad] accept_waveform x{} pending_left={} detected={} empty={}",
                windows_processed,
                self.pending.len(),
                self.inner.detected(),
                self.inner.is_empty()
            );
        }
        self.drain()
    }

    /// Drain any completed segments without pushing new audio.
    pub fn drain(&mut self) -> Vec<Vec<f32>> {
        let mut out = Vec::new();
        while !self.inner.is_empty() {
            if let Some(seg) = self.inner.front() {
                out.push(seg.samples().to_vec());
            }
            self.inner.pop();
        }
        out
    }

    /// Force the VAD to flush any in-progress speech as a final segment.
    pub fn flush(&mut self) -> Vec<Vec<f32>> {
        // Pad pending to a full window and process it before flushing.
        if !self.pending.is_empty() {
            let mut chunk = std::mem::take(&mut self.pending);
            chunk.resize(self.window_size, 0.0);
            self.inner.accept_waveform(&chunk);
        }
        self.inner.flush();
        self.drain()
    }

    pub fn detected(&self) -> bool {
        self.inner.detected()
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}
