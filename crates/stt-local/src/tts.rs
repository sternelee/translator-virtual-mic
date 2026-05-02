//! Offline TTS via sherpa-onnx.
//!
//! Currently supports:
//!   - Kokoro (English / Chinese-English bilingual) — `model_id` prefix `"kokoro-"`
//!   - VITS (any language, fallback)                — `model_id` prefix `"vits-"`
//!
//! # File layout expected under `model_dir/<model_id>/`
//!
//! ## Kokoro models
//! ```text
//! model.onnx          (or model.int8.onnx)
//! voices.bin
//! tokens.txt
//! ```
//!
//! ## VITS models
//! ```text
//! model.onnx          (or model.int8.onnx)
//! tokens.txt
//! lexicon.txt         (optional, language-dependent)
//! ```

use std::path::Path;

use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig, OfflineTtsVitsModelConfig,
};

use crate::{Result, SttError};

/// A loaded offline TTS engine, ready to synthesise audio.
///
/// `Send + Sync` because `OfflineTts` is declared `unsafe impl Send + Sync` in
/// the sherpa-onnx crate and the C library is thread-safe for single-object use.
pub struct TtsBackend {
    inner: OfflineTts,
    model_id: String,
    speaker_id: i32,
    speed: f32,
}

// The sherpa-onnx wrapper itself is Send+Sync; we propagate that.
unsafe impl Send for TtsBackend {}
unsafe impl Sync for TtsBackend {}

impl TtsBackend {
    /// Load a TTS backend from files under `model_dir/<model_id>/`.
    ///
    /// `model_id` must start with `"kokoro-"` or `"vits-"`.
    pub fn new(model_id: &str, model_dir: &Path, speaker_id: i32, speed: f32) -> Result<Self> {
        eprintln!("[tts] loading model_id={model_id} model_dir={model_dir:?}");
        let dir = model_dir.join(model_id);

        let config = if model_id.starts_with("kokoro") {
            build_kokoro_config(&dir)?
        } else {
            build_vits_config(&dir)?
        };

        let tts = OfflineTts::create(&config)
            .ok_or_else(|| SttError::Model(format!("OfflineTts::create failed for {model_id}")))?;

        eprintln!(
            "[tts] loaded: sample_rate={} num_speakers={}",
            tts.sample_rate(),
            tts.num_speakers(),
        );

        Ok(Self {
            inner: tts,
            model_id: model_id.to_string(),
            speaker_id,
            speed,
        })
    }

    /// Synthesise `text` and return `(samples_f32, sample_rate)`.
    pub fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32)> {
        eprintln!(
            "[tts] synthesize: sid={} speed={} text='{}'",
            self.speaker_id,
            self.speed,
            &text[..text.len().min(60)]
        );

        let gen_cfg = GenerationConfig {
            sid: self.speaker_id,
            speed: self.speed,
            ..Default::default()
        };

        let audio = self
            .inner
            .generate_with_config(text, &gen_cfg, None::<fn(&[f32], f32) -> bool>)
            .ok_or_else(|| SttError::Backend("TTS generate returned None".to_string()))?;

        let samples = audio.samples().to_vec();
        let sample_rate = audio.sample_rate() as u32;
        eprintln!(
            "[tts] synthesized {} samples @ {}Hz",
            samples.len(),
            sample_rate
        );
        Ok((samples, sample_rate))
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn sample_rate(&self) -> u32 {
        self.inner.sample_rate() as u32
    }
}

// ---------------------------------------------------------------------------
// Config builders
// ---------------------------------------------------------------------------

fn best_onnx(dir: &Path, base: &str) -> Option<String> {
    // Prefer int8 quantised if present.
    let int8 = dir.join(format!("{base}.int8.onnx"));
    if int8.exists() {
        return Some(int8.to_string_lossy().into_owned());
    }
    let fp32 = dir.join(format!("{base}.onnx"));
    if fp32.exists() {
        return Some(fp32.to_string_lossy().into_owned());
    }
    None
}

fn require_file(dir: &Path, name: &str) -> Result<String> {
    let p = dir.join(name);
    if p.exists() {
        Ok(p.to_string_lossy().into_owned())
    } else {
        Err(SttError::Model(format!(
            "TTS file not found: {}",
            p.display()
        )))
    }
}

fn build_kokoro_config(dir: &Path) -> Result<OfflineTtsConfig> {
    let model = best_onnx(dir, "model").ok_or_else(|| {
        SttError::Model(format!("kokoro model.onnx not found in {}", dir.display()))
    })?;
    let voices = require_file(dir, "voices.bin")?;
    let tokens = require_file(dir, "tokens.txt")?;

    // Optional data_dir (for Chinese text normalisation)
    let data_dir = {
        let d = dir.join("espeak-ng-data");
        if d.exists() {
            Some(d.to_string_lossy().into_owned())
        } else {
            None
        }
    };

    Ok(OfflineTtsConfig {
        model: OfflineTtsModelConfig {
            kokoro: OfflineTtsKokoroModelConfig {
                model: Some(model),
                voices: Some(voices),
                tokens: Some(tokens),
                data_dir,
                ..Default::default()
            },
            num_threads: 2,
            ..Default::default()
        },
        max_num_sentences: 1,
        ..Default::default()
    })
}

fn build_vits_config(dir: &Path) -> Result<OfflineTtsConfig> {
    let model = best_onnx(dir, "model").ok_or_else(|| {
        SttError::Model(format!("vits model.onnx not found in {}", dir.display()))
    })?;
    let tokens = require_file(dir, "tokens.txt")?;

    let lexicon = {
        let lx = dir.join("lexicon.txt");
        if lx.exists() {
            Some(lx.to_string_lossy().into_owned())
        } else {
            None
        }
    };
    let data_dir = {
        let d = dir.join("espeak-ng-data");
        if d.exists() {
            Some(d.to_string_lossy().into_owned())
        } else {
            None
        }
    };

    Ok(OfflineTtsConfig {
        model: OfflineTtsModelConfig {
            vits: OfflineTtsVitsModelConfig {
                model: Some(model),
                tokens: Some(tokens),
                lexicon,
                data_dir,
                ..Default::default()
            },
            num_threads: 2,
            ..Default::default()
        },
        max_num_sentences: 1,
        ..Default::default()
    })
}
