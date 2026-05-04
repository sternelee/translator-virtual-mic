use std::sync::atomic::{AtomicU64, Ordering};

/// Latency tracker that stores the most recent measurement in milliseconds.
/// A value of `u64::MAX` means "not yet measured".
#[derive(Default)]
pub struct LatencyTracker {
    last_ms: AtomicU64,
}

impl LatencyTracker {
    const UNSET: u64 = u64::MAX;

    pub fn record(&self, ms: u64) {
        self.last_ms.store(ms, Ordering::Relaxed);
    }

    pub fn get_ms(&self) -> u64 {
        let v = self.last_ms.load(Ordering::Relaxed);
        if v == Self::UNSET { 0 } else { v }
    }
}

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
    // Latency measurements (all in ms, most-recent value)
    vad_start_latency_ms: LatencyTracker,
    asr_first_partial_ms: LatencyTracker,
    asr_final_ms: LatencyTracker,
    mt_first_output_ms: LatencyTracker,
    tts_first_audio_ms: LatencyTracker,
    end_to_end_first_audio_ms: LatencyTracker,
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

    pub fn record_vad_start_latency(&self, ms: u64) {
        self.vad_start_latency_ms.record(ms);
    }

    pub fn record_asr_first_partial(&self, ms: u64) {
        self.asr_first_partial_ms.record(ms);
    }

    pub fn record_asr_final(&self, ms: u64) {
        self.asr_final_ms.record(ms);
    }

    pub fn record_mt_first_output(&self, ms: u64) {
        self.mt_first_output_ms.record(ms);
    }

    pub fn record_tts_first_audio(&self, ms: u64) {
        self.tts_first_audio_ms.record(ms);
    }

    pub fn record_end_to_end_first_audio(&self, ms: u64) {
        self.end_to_end_first_audio_ms.record(ms);
    }

    pub fn to_json(&self, output_queue_depth_ms: u64) -> String {
        format!(
            concat!(
                "{{",
                "\"mic_capture_gap_ms\":0,",
                "\"vad_start_latency_ms\":{},",
                "\"asr_first_partial_ms\":{},",
                "\"asr_final_ms\":{},",
                "\"mt_first_output_ms\":{},",
                "\"tts_first_audio_ms\":{},",
                "\"end_to_end_first_audio_ms\":{},",
                "\"capture_frames\":{},",
                "\"output_frames\":{},",
                "\"pushed_samples\":{},",
                "\"pulled_samples\":{},",
                "\"output_queue_depth_ms\":{},",
                "\"underrun_count\":{},",
                "\"overflow_count\":{},",
                "\"reconnect_count\":{},",
                "\"fallback_count\":{}" ,
                "}}"
            ),
            self.vad_start_latency_ms.get_ms(),
            self.asr_first_partial_ms.get_ms(),
            self.asr_final_ms.get_ms(),
            self.mt_first_output_ms.get_ms(),
            self.tts_first_audio_ms.get_ms(),
            self.end_to_end_first_audio_ms.get_ms(),
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
