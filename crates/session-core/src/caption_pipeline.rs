//! Local-STT caption pipeline used in `EngineMode::CaptionOnly`.
//!
//! Push 48 kHz interleaved mic samples → resample to 16 kHz mono → Silero VAD
//! → on completed speech segment, hand off to a worker thread that runs the
//! sherpa-onnx STT decode (heavy CPU work, must stay off the audio thread)
//! and an optional remote MT translation, then enqueues a JSON caption event
//! that the Swift host pulls via FFI.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use common::{EngineError, LocalMtConfig, LocalSttConfig, MtConfig, Result, TtsConfig};
use mt_client::{MtClient, MtClientConfig};
use mt_local::{load_backend as load_local_mt_backend, LocalMtBackend};
use stt_local::audio::{stereo_to_mono, CachedResampler};
use stt_local::vad::{Vad, VadConfig};
use stt_local::{load_backend, load_tts_backend, TranscriberBackend, TtsBackend};

const VAD_CHUNK_FRAMES: usize = 512; // sherpa Silero default window

#[derive(Debug)]
struct SegmentJob {
    samples: Vec<f32>,
    timestamp_ns: u64,
    language: String,
}

#[derive(Debug)]
struct PartialJob {
    samples: Vec<f32>,
    timestamp_ns: u64,
    language: String,
}

#[derive(Debug)]
enum WorkerJob {
    Partial(PartialJob),
    Segment(SegmentJob),
}

/// Synthesized audio returned from the worker to the pipeline (and then to the
/// session) so it can be pushed to the virtual-mic output ring.
pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub timestamp_ns: u64,
}

#[derive(Debug)]
pub struct CaptionEvent {
    pub timestamp_ns: u64,
    pub original: String,
    pub translated: Option<String>,
    pub is_final: bool,
}

impl CaptionEvent {
    fn to_json(&self) -> String {
        let mut s = String::with_capacity(128 + self.original.len());
        s.push_str("{\"type\":\"caption\",\"is_final\":");
        s.push_str(if self.is_final { "true" } else { "false" });
        s.push_str(",\"timestamp_ns\":");
        s.push_str(&self.timestamp_ns.to_string());
        s.push_str(",\"text\":\"");
        s.push_str(&escape_json(&self.original));
        s.push('"');
        if let Some(translated) = &self.translated {
            s.push_str(",\"translation\":\"");
            s.push_str(&escape_json(translated));
            s.push('"');
        }
        s.push('}');
        s
    }
}

pub struct CaptionPipeline {
    resampler: CachedResampler,
    vad: Vad,
    language: String,
    job_tx: Option<Sender<WorkerJob>>,
    result_rx: Receiver<CaptionEvent>,
    audio_rx: Receiver<AudioChunk>,
    worker: Option<JoinHandle<()>>,
    pending: VecDeque<String>,
    pending_audio: VecDeque<AudioChunk>,
    last_event_at_ns: u64,
    /// Timestamp (ns) when speech was first detected in the current utterance.
    speech_started_at_ns: Option<u64>,
    // Streaming state
    utterance_buffer: Vec<f32>,
    last_partial_at: Option<Instant>,
    worker_busy: Arc<AtomicBool>,
    last_emitted_text: Option<String>,
    partial_interval: Duration,
    max_partial_samples: usize,
    overlap_tail_samples: usize,
    skip_partial_if_busy: bool,
}

impl CaptionPipeline {
    pub fn from_config(
        stt: &LocalSttConfig,
        mt: Option<&MtConfig>,
        tts: Option<&TtsConfig>,
        local_mt_cfg: Option<&LocalMtConfig>,
    ) -> Result<Self> {
        eprintln!(
            "[caption_pipeline] from_config: model_id={} model_dir={:?} vad_path={:?}",
            stt.model_id, stt.model_dir, stt.vad_model_path
        );
        if !stt.enabled {
            return Err(EngineError::new("local STT is not enabled"));
        }
        if stt.model_dir.as_os_str().is_empty() {
            return Err(EngineError::new("local_stt.model_dir is empty"));
        }
        if stt.vad_model_path.as_os_str().is_empty() {
            return Err(EngineError::new("local_stt.vad_model_path is empty"));
        }

        let backend = load_backend(&stt.model_id, &stt.model_dir)
            .map_err(|e| EngineError::new(format!("load STT backend: {e}")))?;
        eprintln!("[caption_pipeline] STT backend loaded: {}", stt.model_id);

        let mut vad_cfg = VadConfig::new(stt.vad_model_path.to_string_lossy().into_owned())
            .with_threshold(stt.vad_threshold);
        vad_cfg.window_size = VAD_CHUNK_FRAMES as i32;
        let vad = Vad::new(&vad_cfg).map_err(|e| EngineError::new(format!("load VAD: {e}")))?;
        eprintln!("[caption_pipeline] VAD loaded");

        let resampler = CachedResampler::new(48_000, 16_000, 480)
            .map_err(|e| EngineError::new(format!("init resampler: {e}")))?;

        let mt_client = match mt {
            Some(cfg) if cfg.enabled => {
                let key = if cfg.api_key.is_empty() {
                    std::env::var(&cfg.api_key_env).unwrap_or_default()
                } else {
                    cfg.api_key.clone()
                };
                if key.is_empty() {
                    return Err(EngineError::new(format!(
                        "MT enabled but no API key (env {} unset)",
                        cfg.api_key_env
                    )));
                }
                let client_cfg = MtClientConfig::new(key)
                    .with_endpoint(cfg.endpoint.clone())
                    .with_model(cfg.model.clone());
                Some(Arc::new(MtClient::new(client_cfg).map_err(|e| {
                    EngineError::new(format!("MT client init: {e}"))
                })?))
            }
            _ => None,
        };

        let target_lang = mt.map(|m| m.target_language.clone()).unwrap_or_default();

        // Local MT target lang comes from LocalMtConfig (separate from remote MT config).
        let local_mt_target_lang = local_mt_cfg
            .filter(|c| c.enabled)
            .map(|c| c.target_lang.clone())
            .unwrap_or_else(|| target_lang.clone());

        // Optional TTS backend.
        let tts_backend: Option<TtsBackend> = match tts {
            Some(cfg) if cfg.enabled => {
                let backend =
                    load_tts_backend(&cfg.model_id, &cfg.model_dir, cfg.speaker_id, cfg.speed)
                        .map_err(|e| EngineError::new(format!("load TTS backend: {e}")))?;
                eprintln!("[caption_pipeline] TTS backend loaded: {}", cfg.model_id);
                Some(backend)
            }
            _ => None,
        };

        // Optional local MT backend.
        let local_mt_backend: Option<Box<dyn LocalMtBackend>> = match local_mt_cfg {
            Some(cfg) if cfg.enabled => {
                let backend =
                    load_local_mt_backend(&cfg.model_id, &cfg.model_dir, &cfg.source_lang)
                        .map_err(|e| EngineError::new(format!("load local MT backend: {e}")))?;
                eprintln!(
                    "[caption_pipeline] local MT backend loaded: {}",
                    cfg.model_id
                );
                Some(backend)
            }
            _ => None,
        };

        let partial_interval = Duration::from_millis(stt.partial_interval_ms);
        let max_partial_samples = (stt.max_partial_window_seconds * 16_000.0) as usize;
        let overlap_tail_samples = ((stt.overlap_tail_ms as f32 / 1000.0) * 16_000.0) as usize;
        let worker_busy = Arc::new(AtomicBool::new(false));

        let (job_tx, job_rx) = channel::<WorkerJob>();
        let (result_tx, result_rx) = channel::<CaptionEvent>();
        let (audio_tx, audio_rx) = channel::<AudioChunk>();

        let worker_busy_clone = worker_busy.clone();
        let worker = thread::Builder::new()
            .name("caption-stt-worker".to_string())
            .spawn(move || {
                worker_loop(
                    backend,
                    mt_client,
                    local_mt_backend,
                    tts_backend,
                    target_lang,
                    local_mt_target_lang,
                    job_rx,
                    result_tx,
                    audio_tx,
                    worker_busy_clone,
                )
            })
            .map_err(|e| EngineError::new(format!("spawn worker: {e}")))?;

        Ok(Self {
            resampler,
            vad,
            language: stt.language.clone(),
            job_tx: Some(job_tx),
            result_rx,
            audio_rx,
            worker: Some(worker),
            pending: VecDeque::new(),
            pending_audio: VecDeque::new(),
            last_event_at_ns: 0,
            speech_started_at_ns: None,
            utterance_buffer: Vec::new(),
            last_partial_at: None,
            worker_busy,
            last_emitted_text: None,
            partial_interval,
            max_partial_samples,
            overlap_tail_samples,
            skip_partial_if_busy: stt.skip_partial_if_busy,
        })
    }

    /// Feed interleaved PCM (any sample rate, any channel count) and drain any
    /// caption events that the worker has produced since the last call.
    pub fn push_pcm(
        &mut self,
        samples: &[f32],
        channels: u16,
        sample_rate: u32,
        timestamp_ns: u64,
    ) -> Result<()> {
        let mono: Vec<f32> = if channels > 1 {
            stereo_to_mono(samples)
        } else {
            samples.to_vec()
        };

        let resampled = if sample_rate == 16_000 {
            mono
        } else if sample_rate == 48_000 {
            self.resampler
                .push(&mono)
                .map_err(|e| EngineError::new(format!("resample: {e}")))?
        } else {
            // Fall back to a one-shot conversion at unusual rates. Realtime
            // path is 48 kHz; this branch is only hit by tests / odd hosts.
            stt_local::audio::prepare_for_stt(&mono, 1, sample_rate)
                .map_err(|e| EngineError::new(format!("resample: {e}")))?
        };

        // RMS energy — helps diagnose if mic signal is reaching VAD
        // (commented out to reduce log spam)
        // if !resampled.is_empty() {
        //     let rms =
        //         (resampled.iter().map(|s| s * s).sum::<f32>() / resampled.len() as f32).sqrt();
        //     eprintln!(
        //         "[caption_pipeline] push_pcm: input={} resampled={} rms={:.4} vad_detected={}",
        //         samples.len(),
        //         resampled.len(),
        //         rms,
        //         self.vad.detected()
        //     );
        // } else {
        //     eprintln!(
        //         "[caption_pipeline] push_pcm: input={} resampled=0 (buffering)",
        //         samples.len()
        //     );
        // }

        if !resampled.is_empty() {
            let segments = self.vad.push(&resampled);
            let speech_active = self.vad.detected();

            // Accumulate for partials while speech is active.
            if speech_active {
                self.utterance_buffer.extend_from_slice(&resampled);
                let should_send_partial = self
                    .last_partial_at
                    .is_none_or(|t| t.elapsed() >= self.partial_interval);
                if should_send_partial {
                    let window_len = self.utterance_buffer.len().min(self.max_partial_samples);
                    let window =
                        self.utterance_buffer[self.utterance_buffer.len() - window_len..].to_vec();
                    let can_send =
                        !self.skip_partial_if_busy || !self.worker_busy.load(Ordering::Relaxed);
                    if can_send {
                        if let Some(tx) = &self.job_tx {
                            self.worker_busy.store(true, Ordering::Relaxed);
                            let _ = tx.send(WorkerJob::Partial(PartialJob {
                                samples: window,
                                timestamp_ns,
                                language: self.language.clone(),
                            }));
                        }
                    }
                    self.last_partial_at = Some(Instant::now());
                }
            }

            // eprintln!(
            //     "[caption_pipeline] vad produced {} segments",
            //     segments.len()
            // );
            for (_i, seg) in segments.iter().enumerate() {
                // eprintln!("[caption_pipeline] segment {}: {} samples", i, seg.len());
                if let Some(tx) = &self.job_tx {
                    let _ = tx.send(WorkerJob::Segment(SegmentJob {
                        samples: seg.clone(),
                        timestamp_ns,
                        language: self.language.clone(),
                    }));
                }
            }

            // If speech ended, handle tail overlap for next utterance.
            if !speech_active && !self.utterance_buffer.is_empty() {
                if self.utterance_buffer.len() > self.overlap_tail_samples {
                    let tail_start = self.utterance_buffer.len() - self.overlap_tail_samples;
                    let tail = self.utterance_buffer.split_off(tail_start);
                    self.utterance_buffer = tail;
                } else {
                    self.utterance_buffer.clear();
                }
                self.last_partial_at = None;
            }
        }

        self.drain_results();
        Ok(())
    }

    /// Force the VAD to flush any in-progress utterance — call on session stop.
    pub fn flush(&mut self) -> Result<()> {
        let segments = self.vad.flush();
        for samples in segments {
            if let Some(tx) = &self.job_tx {
                let _ = tx.send(WorkerJob::Segment(SegmentJob {
                    samples,
                    timestamp_ns: now_ns(),
                    language: self.language.clone(),
                }));
            }
        }
        self.utterance_buffer.clear();
        self.last_partial_at = None;
        self.drain_results();
        Ok(())
    }

    pub fn take_next_event(&mut self) -> Option<String> {
        self.drain_results();
        self.pending.pop_front()
    }

    /// Drain synthesized audio chunks produced by the TTS worker.
    /// Call after `push_pcm` or `flush`; returns `None` when queue is empty.
    pub fn take_next_audio(&mut self) -> Option<AudioChunk> {
        self.drain_results();
        self.pending_audio.pop_front()
    }

    pub fn last_event_at_ns(&self) -> u64 {
        self.last_event_at_ns
    }

    fn drain_results(&mut self) {
        while let Ok(event) = self.result_rx.try_recv() {
            if event.timestamp_ns > self.last_event_at_ns {
                self.last_event_at_ns = event.timestamp_ns;
            }
            // Deduplication: if this final event's text matches the last
            // emitted text AND has no new translation, skip it — the UI
            // already has the correct draft.  If it carries a translation
            // (which partials never do), we must emit it.
            if event.is_final {
                if let Some(ref last) = self.last_emitted_text {
                    if last == &event.original && event.translated.is_none() {
                        continue;
                    }
                }
                self.last_emitted_text = Some(event.original.clone());
            } else {
                // Partial replaces last_emitted_text so we can still catch
                // duplicate finals later.
                self.last_emitted_text = Some(event.original.clone());
            }
            self.pending.push_back(event.to_json());
        }
        while let Ok(chunk) = self.audio_rx.try_recv() {
            self.pending_audio.push_back(chunk);
        }
    }
}

impl Drop for CaptionPipeline {
    fn drop(&mut self) {
        // Closing the sender lets the worker observe a hangup and exit.
        self.job_tx = None;
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}

fn worker_loop(
    backend: Box<dyn TranscriberBackend>,
    mt: Option<Arc<MtClient>>,
    local_mt: Option<Box<dyn LocalMtBackend>>,
    tts: Option<TtsBackend>,
    target_lang: String,
    local_mt_target_lang: String,
    rx: Receiver<WorkerJob>,
    tx: Sender<CaptionEvent>,
    audio_tx: Sender<AudioChunk>,
    worker_busy: Arc<AtomicBool>,
) {
    while let Ok(job) = rx.recv() {
        let is_final = matches!(job, WorkerJob::Segment(_));
        let (samples, timestamp_ns, language) = match job {
            WorkerJob::Partial(j) => (j.samples, j.timestamp_ns, j.language),
            WorkerJob::Segment(j) => (j.samples, j.timestamp_ns, j.language),
        };

        if samples.is_empty() {
            worker_busy.store(false, Ordering::Relaxed);
            continue;
        }

        // eprintln!(
        //     "[caption_pipeline] worker: transcribing {} samples (is_final={})",
        //     samples.len(),
        //     is_final
        // );
        let original = match backend.transcribe(&samples, &language) {
            Ok(text) => {
                eprintln!("[caption_pipeline] worker: transcribed='{}'", text);
                text
            }
            Err(e) => {
                eprintln!("[caption_pipeline] worker: transcribe error: {}", e);
                worker_busy.store(false, Ordering::Relaxed);
                continue;
            }
        };
        if original.trim().is_empty() {
            worker_busy.store(false, Ordering::Relaxed);
            continue;
        }

        // Only run MT/TTS on final segments.
        let (translated, tts_text) = if is_final {
            let translated = if let Some(ref lmt) = local_mt {
                let result = lmt.translate(&original, &local_mt_target_lang);
                eprintln!("[caption_pipeline] worker: MT result={:?}", result);
                result.ok().filter(|t| !t.trim().is_empty())
            } else {
                mt.as_ref()
                    .and_then(|client| client.translate(&original, &target_lang).ok())
                    .filter(|t| !t.trim().is_empty())
            };
            let tts_text = translated.as_deref().unwrap_or(&original).to_string();
            (translated, Some(tts_text))
        } else {
            (None, None)
        };

        // TTS only for final.
        if is_final {
            if let Some(ref tts_backend) = tts {
                if let Some(ref text) = tts_text {
                    match tts_backend.synthesize(text) {
                        Ok((samples, sample_rate)) => {
                            eprintln!(
                                "[caption_pipeline] worker: TTS produced {} samples @ {}Hz",
                                samples.len(),
                                sample_rate
                            );
                            let _ = audio_tx.send(AudioChunk {
                                samples,
                                sample_rate,
                                timestamp_ns,
                            });
                        }
                        Err(e) => eprintln!("[caption_pipeline] worker: TTS error: {}", e),
                    }
                }
            }
        }

        let event = CaptionEvent {
            timestamp_ns,
            original,
            translated,
            is_final,
        };
        // eprintln!(
        //     "[caption_pipeline] worker: sending caption event (is_final={})",
        //     is_final
        // );
        if tx.send(event).is_err() {
            worker_busy.store(false, Ordering::Relaxed);
            break;
        }
        worker_busy.store(false, Ordering::Relaxed);
    }
}

fn escape_json(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 2);
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caption_event_json_shape() {
        let ev = CaptionEvent {
            timestamp_ns: 12345,
            original: "hello \"world\"".to_string(),
            translated: Some("你好".to_string()),
            is_final: true,
        };
        let json = ev.to_json();
        assert!(json.contains("\"type\":\"caption\""));
        assert!(json.contains("\"is_final\":true"));
        assert!(json.contains("\"timestamp_ns\":12345"));
        assert!(json.contains("\"text\":\"hello \\\"world\\\"\""));
        assert!(json.contains("\"translation\":\"你好\""));
    }

    #[test]
    fn caption_event_json_without_translation() {
        let ev = CaptionEvent {
            timestamp_ns: 1,
            original: "hi".to_string(),
            translated: None,
            is_final: true,
        };
        let json = ev.to_json();
        assert!(!json.contains("translation"));
        assert!(json.contains("\"text\":\"hi\""));
    }

    #[test]
    fn caption_event_partial_json_shape() {
        let ev = CaptionEvent {
            timestamp_ns: 12345,
            original: "hello worl".to_string(),
            translated: None,
            is_final: false,
        };
        let json = ev.to_json();
        assert!(json.contains("\"is_final\":false"));
        assert!(json.contains("\"text\":\"hello worl\""));
        assert!(!json.contains("translation"));
    }

    #[test]
    fn worker_job_is_final_matching() {
        let partial = WorkerJob::Partial(PartialJob {
            samples: vec![0.0],
            timestamp_ns: 1,
            language: "en".into(),
        });
        let segment = WorkerJob::Segment(SegmentJob {
            samples: vec![0.0],
            timestamp_ns: 2,
            language: "en".into(),
        });
        assert!(!matches!(partial, WorkerJob::Segment(_)));
        assert!(matches!(segment, WorkerJob::Segment(_)));
    }
}
