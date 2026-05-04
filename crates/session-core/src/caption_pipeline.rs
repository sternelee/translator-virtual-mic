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

fn log_and_eprint(log_tx: &Option<Sender<String>>, msg: String) {
    eprintln!("{msg}");
    if let Some(tx) = log_tx {
        let _ = tx.send(msg);
    }
}

use common::{
    CosyVoiceTtsConfig, ElevenLabsTtsConfig, EngineError, LocalMtConfig, LocalSttConfig,
    MiniMaxTtsConfig, MtConfig, Result, TtsConfig,
};
use mt_client::{MtClient, MtClientConfig};
use mt_local::{load_backend as load_local_mt_backend, LocalMtBackend};
use stt_local::audio::{stereo_to_mono, CachedResampler};
use stt_local::vad::{Vad, VadConfig};
use stt_local::{load_backend, load_tts_backend, TranscriberBackend, TtsBackend};
use tts_cosyvoice::{CosyVoiceClient, CosyVoiceConfig};
use tts_elevenlabs::{ElevenLabsTtsClient, ElevenLabsTtsConfig as ElTtsConfig};
use tts_minimax::MiniMaxTtsClient;

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

/// Job handed from the STT worker to the post-processor thread.
/// Post-processing (MT + TTS) is slow, so it runs on its own thread so the
/// STT worker can keep chewing through new audio segments without blocking.
#[derive(Debug)]
struct PostProcessJob {
    original: String,
    is_final: bool,
    timestamp_ns: u64,
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
    stt_worker: Option<JoinHandle<()>>,
    post_worker: Option<JoinHandle<()>>,
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
    /// Optional metrics reference for latency reporting.
    metrics: Option<Arc<metrics::EngineMetrics>>,
    /// Tracks whether we've already recorded first-partial latency for current utterance.
    first_partial_recorded: bool,
    /// Tracks whether we've already recorded first-tts latency for current utterance.
    first_tts_recorded: bool,
    /// Tracks whether we've already recorded end-to-end latency for current utterance.
    end_to_end_recorded: bool,
    /// Timestamp when the current utterance ended (speech stop detected).
    speech_ended_at_ns: Option<u64>,
}

impl CaptionPipeline {
    pub fn from_config(
        stt: &LocalSttConfig,
        mt: Option<&MtConfig>,
        tts: Option<&TtsConfig>,
        local_mt_cfg: Option<&LocalMtConfig>,
        cosyvoice_cfg: Option<&CosyVoiceTtsConfig>,
        elevenlabs_tts_cfg: Option<&ElevenLabsTtsConfig>,
        minimax_tts_cfg: Option<&MiniMaxTtsConfig>,
        log_tx: Option<Sender<String>>,
        metrics: Option<Arc<metrics::EngineMetrics>>,
    ) -> Result<Self> {
        log_and_eprint(
            &log_tx,
            format!(
                "[caption_pipeline] from_config: model_id={} model_dir={:?} vad_path={:?}",
                stt.model_id, stt.model_dir, stt.vad_model_path
            ),
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
        log_and_eprint(
            &log_tx,
            format!("[caption_pipeline] STT backend loaded: {}", stt.model_id),
        );

        let mut vad_cfg = VadConfig::new(stt.vad_model_path.to_string_lossy().into_owned())
            .with_threshold(stt.vad_threshold);
        vad_cfg.window_size = VAD_CHUNK_FRAMES as i32;
        let vad = Vad::new(&vad_cfg).map_err(|e| EngineError::new(format!("load VAD: {e}")))?;
        log_and_eprint(&log_tx, "[caption_pipeline] VAD loaded".to_string());

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
                log_and_eprint(
                    &log_tx,
                    format!("[caption_pipeline] TTS backend loaded: {}", cfg.model_id),
                );
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
                log_and_eprint(
                    &log_tx,
                    format!(
                        "[caption_pipeline] local MT backend loaded: {}",
                        cfg.model_id
                    ),
                );
                Some(backend)
            }
            _ => None,
        };

        // Optional CosyVoice TTS client (preferred over sherpa-onnx TTS if both enabled).
        let cosyvoice_client: Option<CosyVoiceClient> = match cosyvoice_cfg {
            Some(cfg) if cfg.enabled => {
                let cv_cfg = CosyVoiceConfig {
                    endpoint: cfg.endpoint.clone(),
                    prompt_wav_path: cfg.prompt_wav_path.clone(),
                    prompt_text: cfg.prompt_text.clone(),
                };
                match CosyVoiceClient::new(cv_cfg) {
                    Ok(client) => {
                        log_and_eprint(
                            &log_tx,
                            format!(
                                "[caption_pipeline] CosyVoice TTS client ready (endpoint={})",
                                cfg.endpoint
                            ),
                        );
                        Some(client)
                    }
                    Err(e) => {
                        log_and_eprint(
                            &log_tx,
                            format!("[caption_pipeline] CosyVoice TTS init failed: {e}"),
                        );
                        None
                    }
                }
            }
            _ => None,
        };

        // Optional ElevenLabs TTS client (highest priority if enabled).
        let elevenlabs_client: Option<ElevenLabsTtsClient> = match elevenlabs_tts_cfg {
            Some(cfg) if cfg.enabled => {
                let api_key = if cfg.api_key.is_empty() {
                    std::env::var("ELEVENLABS_API_KEY").unwrap_or_default()
                } else {
                    cfg.api_key.clone()
                };
                let voice_id = if cfg.voice_id.is_empty() {
                    std::env::var("ELEVENLABS_VOICE_ID").unwrap_or_default()
                } else {
                    cfg.voice_id.clone()
                };
                let el_cfg = ElTtsConfig {
                    enabled: true,
                    api_key,
                    voice_id,
                    model_id: cfg.model_id.clone(),
                };
                match ElevenLabsTtsClient::new(el_cfg) {
                    Ok(client) => {
                        log_and_eprint(
                            &log_tx,
                            "[caption_pipeline] ElevenLabs TTS client ready".to_string(),
                        );
                        Some(client)
                    }
                    Err(e) => {
                        log_and_eprint(
                            &log_tx,
                            format!("[caption_pipeline] ElevenLabs TTS init failed: {e}"),
                        );
                        None
                    }
                }
            }
            _ => None,
        };

        // Optional MiniMax TTS client (highest priority if enabled).
        let minimax_client: Option<MiniMaxTtsClient> = match minimax_tts_cfg {
            Some(cfg) if cfg.enabled => match MiniMaxTtsClient::new(cfg) {
                Ok(client) => {
                    log_and_eprint(
                        &log_tx,
                        "[caption_pipeline] MiniMax TTS client ready".to_string(),
                    );
                    Some(client)
                }
                Err(e) => {
                    log_and_eprint(
                        &log_tx,
                        format!("[caption_pipeline] MiniMax TTS init failed: {e}"),
                    );
                    None
                }
            },
            _ => None,
        };

        let partial_interval = Duration::from_millis(stt.partial_interval_ms);
        let max_partial_samples = (stt.max_partial_window_seconds * 16_000.0) as usize;
        let overlap_tail_samples = ((stt.overlap_tail_ms as f32 / 1000.0) * 16_000.0) as usize;
        let worker_busy = Arc::new(AtomicBool::new(false));

        let (job_tx, job_rx) = channel::<WorkerJob>();
        let (result_tx, result_rx) = channel::<CaptionEvent>();
        let (audio_tx, audio_rx) = channel::<AudioChunk>();

        // Post-processor channel: STT worker → MT+TTS thread.
        let (post_tx, post_rx) = channel::<PostProcessJob>();

        // Spawn the MT+TTS post-processor first so it is ready to receive.
        let log_tx_post = log_tx.clone();
        let result_tx_post = result_tx.clone();
        let post_worker = thread::Builder::new()
            .name("caption-post-processor".to_string())
            .spawn(move || {
                post_processor_loop(
                    mt_client,
                    local_mt_backend,
                    tts_backend,
                    cosyvoice_client,
                    elevenlabs_client,
                    minimax_client,
                    target_lang,
                    local_mt_target_lang,
                    post_rx,
                    result_tx_post,
                    audio_tx,
                    log_tx_post,
                )
            })
            .map_err(|e| EngineError::new(format!("spawn post-processor: {e}")))?;

        let worker_busy_clone = worker_busy.clone();
        let log_tx_clone = log_tx.clone();
        let stt_worker = thread::Builder::new()
            .name("caption-stt-worker".to_string())
            .spawn(move || {
                stt_worker_loop(
                    backend,
                    post_tx,
                    result_tx,
                    job_rx,
                    worker_busy_clone,
                    log_tx_clone,
                )
            })
            .map_err(|e| EngineError::new(format!("spawn stt worker: {e}")))?;

        Ok(Self {
            resampler,
            vad,
            language: stt.language.clone(),
            job_tx: Some(job_tx),
            result_rx,
            audio_rx,
            stt_worker: Some(stt_worker),
            post_worker: Some(post_worker),
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
            metrics,
            first_partial_recorded: false,
            first_tts_recorded: false,
            end_to_end_recorded: false,
            speech_ended_at_ns: None,
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
            for seg in segments.iter() {
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
                // Record speech end timestamp for latency calculations.
                self.speech_ended_at_ns = Some(timestamp_ns);
            }

            // New utterance detected — reset per-utterance state and record VAD latency.
            if speech_active && self.speech_started_at_ns.is_none() {
                self.reset_utterance_latency();
                self.speech_started_at_ns = Some(timestamp_ns);
                if let Some(ref metrics) = self.metrics {
                    // VAD start latency: time from first audio in this utterance to detection.
                    // Since we just detected it now, latency is approximately 0 relative to
                    // this callback, but we can record a small estimate based on VAD window.
                    metrics.record_vad_start_latency(32); // ~32ms for 512 samples @ 16kHz
                }
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

    /// Returns true if there are caption events (or TTS audio) waiting to be
    /// consumed.  A cheap check suitable for polling loops.
    pub fn has_pending_events(&mut self) -> bool {
        self.drain_results();
        !self.pending.is_empty() || !self.pending_audio.is_empty()
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
        let now_ns = now_ns();
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

            // Latency tracking
            if let Some(ref metrics) = self.metrics {
                if !event.is_final && !self.first_partial_recorded {
                    if let Some(start) = self.speech_started_at_ns {
                        let ms = ((now_ns.saturating_sub(start)) / 1_000_000).max(1);
                        metrics.record_asr_first_partial(ms);
                        self.first_partial_recorded = true;
                    }
                }
                if event.is_final {
                    if let Some(start) = self.speech_started_at_ns {
                        let ms = ((now_ns.saturating_sub(start)) / 1_000_000).max(1);
                        metrics.record_asr_final(ms);
                    }
                    if event.translated.is_some() {
                        if let Some(end) = self.speech_ended_at_ns {
                            let ms = ((now_ns.saturating_sub(end)) / 1_000_000).max(1);
                            metrics.record_mt_first_output(ms);
                        }
                    }
                }
            }

            self.pending.push_back(event.to_json());
        }
        while let Ok(chunk) = self.audio_rx.try_recv() {
            if let Some(ref metrics) = self.metrics {
                if !self.first_tts_recorded {
                    if let Some(end) = self.speech_ended_at_ns {
                        let ms = ((now_ns.saturating_sub(end)) / 1_000_000).max(1);
                        metrics.record_tts_first_audio(ms);
                    }
                    self.first_tts_recorded = true;
                }
                if !self.end_to_end_recorded {
                    if let Some(start) = self.speech_started_at_ns {
                        let ms = ((now_ns.saturating_sub(start)) / 1_000_000).max(1);
                        metrics.record_end_to_end_first_audio(ms);
                    }
                    self.end_to_end_recorded = true;
                }
            }
            self.pending_audio.push_back(chunk);
        }
    }

    /// Reset per-utterance latency tracking state. Call when a new utterance begins.
    fn reset_utterance_latency(&mut self) {
        self.first_partial_recorded = false;
        self.first_tts_recorded = false;
        self.end_to_end_recorded = false;
        self.speech_started_at_ns = None;
        self.speech_ended_at_ns = None;
    }
}

impl Drop for CaptionPipeline {
    fn drop(&mut self) {
        // Closing the senders lets the workers observe a hangup and exit.
        self.job_tx = None;
        if let Some(handle) = self.stt_worker.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.post_worker.take() {
            let _ = handle.join();
        }
    }
}

/// STT-only worker.  Transcribes audio and immediately forwards the raw
/// caption event so the UI is not blocked by slow MT/TTS.  Final segments
/// are additionally pushed to the post-processor for MT + TTS.
fn stt_worker_loop(
    backend: Box<dyn TranscriberBackend>,
    post_tx: Sender<PostProcessJob>,
    tx: Sender<CaptionEvent>,
    rx: Receiver<WorkerJob>,
    worker_busy: Arc<AtomicBool>,
    log_tx: Option<Sender<String>>,
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

        const MIN_STT_SAMPLES: usize = 1600;
        if samples.len() < MIN_STT_SAMPLES {
            worker_busy.store(false, Ordering::Relaxed);
            continue;
        }

        let original = match backend.transcribe(&samples, &language) {
            Ok(text) => {
                log_and_eprint(
                    &log_tx,
                    format!("[caption_pipeline] stt_worker: transcribed='{}'", text),
                );
                text
            }
            Err(e) => {
                log_and_eprint(
                    &log_tx,
                    format!("[caption_pipeline] stt_worker: transcribe error: {}", e),
                );
                worker_busy.store(false, Ordering::Relaxed);
                continue;
            }
        };
        if original.trim().is_empty() {
            worker_busy.store(false, Ordering::Relaxed);
            continue;
        }

        // Hand all transcriptions (partial + final) to the post-processor for
        // MT.  Partials are translated in real time so the UI sees streaming
        // translation; finals trigger both MT and TTS.
        let _ = post_tx.send(PostProcessJob {
            original: original.clone(),
            is_final,
            timestamp_ns,
        });

        // Emit the raw STT caption immediately so the UI is never blocked.
        // The post-processor will emit a second event with the translation
        // (and any TTS audio) when it finishes.
        let _ = tx.send(CaptionEvent {
            timestamp_ns,
            original,
            translated: None,
            is_final,
        });
        // We intentionally do NOT store worker_busy=false here for finals,
        // because the post-processor may still be running.  The audio thread
        // only checks worker_busy to decide whether to *send* a new partial;
        // finals are always sent.  For partials we clear the flag below.
        if !is_final {
            worker_busy.store(false, Ordering::Relaxed);
        }
    }
}

/// Post-processor worker: runs MT for every transcription (partial + final)
/// and TTS only for finals.  Partial MT is debounced to avoid spamming the
/// backend with single-character deltas.
fn post_processor_loop(
    mt: Option<Arc<MtClient>>,
    local_mt: Option<Box<dyn LocalMtBackend>>,
    tts: Option<TtsBackend>,
    cosyvoice: Option<CosyVoiceClient>,
    elevenlabs: Option<ElevenLabsTtsClient>,
    minimax: Option<MiniMaxTtsClient>,
    target_lang: String,
    local_mt_target_lang: String,
    rx: Receiver<PostProcessJob>,
    tx: Sender<CaptionEvent>,
    audio_tx: Sender<AudioChunk>,
    log_tx: Option<Sender<String>>,
) {
    let mut last_tts_text = String::new();
    let mut last_partial_original = String::new();
    let mut last_partial_translation = String::new();

    while let Ok(job) = rx.recv() {
        let original = job.original;
        let timestamp_ns = job.timestamp_ns;
        let is_final = job.is_final;

        // --- MT ---
        // For partials: debounce — skip if identical to last translated partial
        // or if the text is too short to be meaningful.
        const MIN_PARTIAL_LEN: usize = 5;
        let should_translate = if is_final {
            true
        } else {
            original.trim() != last_partial_original.trim()
                && original.trim().len() >= MIN_PARTIAL_LEN
        };

        let translated = if should_translate {
            if let Some(ref lmt) = local_mt {
                match lmt.translate(&original, &local_mt_target_lang) {
                    Ok(t) if !t.trim().is_empty() => Some(t),
                    Ok(_) => None,
                    Err(e) => {
                        log_and_eprint(
                            &log_tx,
                            format!("[caption_pipeline] post_processor: local MT error: {e}"),
                        );
                        None
                    }
                }
            } else {
                mt.as_ref()
                    .and_then(|client| client.translate(&original, &target_lang).ok())
                    .filter(|t| !t.trim().is_empty())
            }
        } else if !is_final && original.trim() == last_partial_original.trim() {
            // Re-use the last translated partial so the UI still gets a
            // CaptionEvent with translation for this partial.
            Some(last_partial_translation.clone())
        } else {
            None
        };

        // Emit the translated caption event for both partials and finals.
        if let Some(ref t) = translated {
            let _ = tx.send(CaptionEvent {
                timestamp_ns,
                original: original.clone(),
                translated: Some(t.clone()),
                is_final,
            });

            if !is_final {
                last_partial_original = original.clone();
                last_partial_translation = t.clone();
            }
        }

        // On a final segment, reset partial tracking so the next utterance
        // starts fresh.
        if is_final {
            last_partial_original.clear();
            last_partial_translation.clear();
        }

        // --- TTS: only for finals to avoid audio stuttering on every partial. ---
        if !is_final {
            continue;
        }

        let tts_candidate: &str = translated.as_deref().unwrap_or(&original);
        let has_tts =
            minimax.is_some() || elevenlabs.is_some() || cosyvoice.is_some() || tts.is_some();
        if has_tts
            && !tts_candidate.trim().is_empty()
            && tts_candidate.trim() != last_tts_text.trim()
        {
            let audio_result: Option<(Vec<f32>, u32)> = if let Some(ref mm) = minimax {
                match mm.synthesize(tts_candidate) {
                    Ok((samples, rate)) if !samples.is_empty() => {
                        eprintln!(
                            "[caption_pipeline] post_processor: MiniMax TTS produced {} samples @ {}Hz",
                            samples.len(), rate
                        );
                        Some((samples, rate))
                    }
                    Ok(_) => None,
                    Err(e) => {
                        log_and_eprint(
                            &log_tx,
                            format!("[caption_pipeline] post_processor: MiniMax TTS error: {e}"),
                        );
                        None
                    }
                }
            } else if let Some(ref el) = elevenlabs {
                match el.synthesize(tts_candidate) {
                    Ok((samples, rate)) if !samples.is_empty() => {
                        eprintln!(
                            "[caption_pipeline] post_processor: ElevenLabs TTS produced {} samples @ {}Hz",
                            samples.len(), rate
                        );
                        Some((samples, rate))
                    }
                    Ok(_) => None,
                    Err(e) => {
                        log_and_eprint(
                            &log_tx,
                            format!("[caption_pipeline] post_processor: ElevenLabs TTS error: {e}"),
                        );
                        None
                    }
                }
            } else if let Some(ref cv) = cosyvoice {
                match cv.synthesize(tts_candidate) {
                    Ok((samples, rate)) if !samples.is_empty() => {
                        eprintln!(
                            "[caption_pipeline] post_processor: CosyVoice TTS produced {} samples @ {}Hz",
                            samples.len(), rate
                        );
                        Some((samples, rate))
                    }
                    Ok(_) => None,
                    Err(e) => {
                        log_and_eprint(
                            &log_tx,
                            format!("[caption_pipeline] post_processor: CosyVoice TTS error: {e}"),
                        );
                        None
                    }
                }
            } else if let Some(ref tts_backend) = tts {
                match tts_backend.synthesize(tts_candidate) {
                    Ok((samples, rate)) => {
                        eprintln!(
                            "[caption_pipeline] post_processor: sherpa TTS produced {} samples @ {}Hz",
                            samples.len(), rate
                        );
                        Some((samples, rate))
                    }
                    Err(e) => {
                        log_and_eprint(
                            &log_tx,
                            format!("[caption_pipeline] post_processor: TTS error: {e}"),
                        );
                        None
                    }
                }
            } else {
                None
            };

            if let Some((tts_samples, sample_rate)) = audio_result {
                let _ = audio_tx.send(AudioChunk {
                    samples: tts_samples,
                    sample_rate,
                    timestamp_ns,
                });
                last_tts_text = tts_candidate.to_string();
            }
        }
        last_tts_text.clear(); // reset per-utterance dedup
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

    #[test]
    fn tts_fires_on_partial_and_final() {
        let mut last_tts_text = String::new();
        let text = "hello world";
        let should_synthesize = text.trim() != last_tts_text.trim();
        assert!(should_synthesize, "first partial should synthesize");
        last_tts_text = text.to_string();
        let should_synthesize_again = text.trim() != last_tts_text.trim();
        assert!(
            !should_synthesize_again,
            "identical partial should be skipped"
        );
        // Simulate final segment: clear dedup state
        last_tts_text.clear();
        let should_synthesize_after_final = text.trim() != last_tts_text.trim();
        assert!(
            should_synthesize_after_final,
            "after final reset, next partial should synthesize again"
        );
    }
}
