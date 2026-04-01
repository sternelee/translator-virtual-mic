use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct EngineMetrics {
    capture_frames: AtomicU64,
    output_frames: AtomicU64,
    pushed_samples: AtomicU64,
    pulled_samples: AtomicU64,
    underrun_count: AtomicU64,
    overflow_count: AtomicU64,
    reconnect_count: AtomicU64,
    fallback_count: AtomicU64,
}

impl EngineMetrics {
    pub fn record_capture(&self, frames: u64, samples: u64) {
        self.capture_frames.fetch_add(frames, Ordering::Relaxed);
        self.pushed_samples.fetch_add(samples, Ordering::Relaxed);
    }

    pub fn record_output(&self, frames: u64, samples: u64) {
        self.output_frames.fetch_add(frames, Ordering::Relaxed);
        self.pulled_samples.fetch_add(samples, Ordering::Relaxed);
    }

    pub fn record_underrun(&self) {
        self.underrun_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_overflow(&self) {
        self.overflow_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_reconnect(&self) {
        self.reconnect_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_fallback(&self) {
        self.fallback_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn to_json(&self, output_queue_depth_ms: u64) -> String {
        format!(
            concat!(
                "{{",
                "\"mic_capture_gap_ms\":0,",
                "\"vad_start_latency_ms\":0,",
                "\"asr_first_partial_ms\":0,",
                "\"asr_final_ms\":0,",
                "\"mt_first_output_ms\":0,",
                "\"tts_first_audio_ms\":0,",
                "\"end_to_end_first_audio_ms\":0,",
                "\"capture_frames\":{},",
                "\"output_frames\":{},",
                "\"pushed_samples\":{},",
                "\"pulled_samples\":{},",
                "\"output_queue_depth_ms\":{},",
                "\"underrun_count\":{},",
                "\"overflow_count\":{},",
                "\"reconnect_count\":{},",
                "\"fallback_count\":{}",
                "}}"
            ),
            self.capture_frames.load(Ordering::Relaxed),
            self.output_frames.load(Ordering::Relaxed),
            self.pushed_samples.load(Ordering::Relaxed),
            self.pulled_samples.load(Ordering::Relaxed),
            output_queue_depth_ms,
            self.underrun_count.load(Ordering::Relaxed),
            self.overflow_count.load(Ordering::Relaxed),
            self.reconnect_count.load(Ordering::Relaxed),
            self.fallback_count.load(Ordering::Relaxed),
        )
    }
}
