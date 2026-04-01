use std::collections::VecDeque;
use std::sync::Mutex;

use common::{AudioFrame, Result};

#[derive(Debug)]
pub struct SampleRingBuffer {
    inner: Mutex<RingState>,
}

#[derive(Debug)]
struct RingState {
    channels: u16,
    sample_rate: u32,
    capacity_samples: usize,
    samples: VecDeque<f32>,
    last_timestamp_ns: u64,
}

impl SampleRingBuffer {
    pub fn new(capacity_frames: usize, channels: u16, sample_rate: u32) -> Self {
        let capacity_samples = capacity_frames.saturating_mul(usize::from(channels.max(1)));
        Self {
            inner: Mutex::new(RingState {
                channels,
                sample_rate,
                capacity_samples,
                samples: VecDeque::with_capacity(capacity_samples),
                last_timestamp_ns: 0,
            }),
        }
    }

    pub fn push_frame(&self, frame: &AudioFrame) -> Result<usize> {
        let mut state = self.inner.lock().expect("ring poisoned");
        state.channels = frame.channels.max(1);
        state.sample_rate = frame.sample_rate;
        state.last_timestamp_ns = frame.timestamp_ns;

        let mut dropped = 0usize;
        for sample in &frame.data {
            if state.samples.len() == state.capacity_samples {
                state.samples.pop_front();
                dropped += 1;
            }
            state.samples.push_back(*sample);
        }

        Ok(dropped)
    }

    pub fn pop_into(&self, out: &mut [f32], requested_channels: u16) -> usize {
        let mut state = self.inner.lock().expect("ring poisoned");
        let requested_channels = usize::from(requested_channels.max(1));
        let available = out.len().min(state.samples.len());

        for slot in out.iter_mut().take(available) {
            *slot = state.samples.pop_front().unwrap_or(0.0);
        }
        for slot in out.iter_mut().skip(available) {
            *slot = 0.0;
        }

        available / requested_channels
    }

    pub fn available_frames(&self) -> usize {
        let state = self.inner.lock().expect("ring poisoned");
        let channels = usize::from(state.channels.max(1));
        state.samples.len() / channels
    }

    pub fn last_timestamp_ns(&self) -> u64 {
        let state = self.inner.lock().expect("ring poisoned");
        state.last_timestamp_ns
    }

    pub fn sample_rate(&self) -> u32 {
        let state = self.inner.lock().expect("ring poisoned");
        state.sample_rate
    }
}

pub fn build_frame(samples: &[f32], frame_count: usize, channels: u16, sample_rate: u32, timestamp_ns: u64) -> AudioFrame {
    let sample_len = frame_count.saturating_mul(usize::from(channels.max(1)));
    let mut data = Vec::with_capacity(sample_len);
    data.extend_from_slice(&samples[..sample_len.min(samples.len())]);

    AudioFrame {
        timestamp_ns,
        sample_rate,
        channels,
        data,
    }
}
