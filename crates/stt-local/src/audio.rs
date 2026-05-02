use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

use crate::{Result, SttError};

pub fn stereo_to_mono(samples: &[f32]) -> Vec<f32> {
    samples
        .chunks_exact(2)
        .map(|pair| (pair[0] + pair[1]) / 2.0)
        .collect()
}

pub fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|&s| s as f32 / 32768.0).collect()
}

/// One-shot resampler. Input0 builds this fresh per call; for batch contexts
/// (small chunks where capacity changes) it is fine, but realtime callers
/// should use `CachedResampler`.
pub fn resample_once(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>> {
    if from_rate == 0 || to_rate == 0 {
        return Err(SttError::Audio("sample rate must be non-zero".into()));
    }
    if from_rate == to_rate {
        return Ok(samples.to_vec());
    }
    if samples.is_empty() {
        return Ok(Vec::new());
    }

    let ratio = f64::from(to_rate) / f64::from(from_rate);
    let mut resampler =
        SincFixedIn::<f32>::new(ratio, 2.0, sinc_params(), samples.len(), 1)
            .map_err(|e| SttError::Audio(format!("resampler init failed: {e}")))?;

    let wave_in = vec![samples.to_vec()];
    let mut output = resampler
        .process(&wave_in, None)
        .map_err(|e| SttError::Audio(format!("resample failed: {e}")))?;
    Ok(output.pop().unwrap_or_default())
}

/// Convert arbitrary PCM into 16 kHz mono float, applying channel mix down and
/// resampling. Equivalent to `prepare_for_whisper` in input0.
pub fn prepare_for_stt(samples: &[f32], channels: u16, sample_rate: u32) -> Result<Vec<f32>> {
    if samples.is_empty() {
        return Ok(Vec::new());
    }
    let mono = if channels > 1 {
        stereo_to_mono(samples)
    } else {
        samples.to_vec()
    };
    resample_once(&mono, sample_rate, 16_000)
}

fn sinc_params() -> SincInterpolationParameters {
    SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    }
}

/// Cached resampler that reuses one `SincFixedIn` instance for repeated chunks
/// of the same size. Realtime audio frames have a fixed shape (e.g. 20ms @
/// 48 kHz = 960 samples) so building the resampler once is a large win.
pub struct CachedResampler {
    from_rate: u32,
    to_rate: u32,
    chunk_frames: usize,
    inner: Option<SincFixedIn<f32>>,
    pending: Vec<f32>,
}

impl CachedResampler {
    pub fn new(from_rate: u32, to_rate: u32, chunk_frames: usize) -> Result<Self> {
        if from_rate == 0 || to_rate == 0 {
            return Err(SttError::Audio("sample rate must be non-zero".into()));
        }
        let mut s = Self {
            from_rate,
            to_rate,
            chunk_frames,
            inner: None,
            pending: Vec::with_capacity(chunk_frames * 2),
        };
        s.ensure_resampler()?;
        Ok(s)
    }

    pub fn from_rate(&self) -> u32 {
        self.from_rate
    }

    pub fn to_rate(&self) -> u32 {
        self.to_rate
    }

    /// Push mono f32 samples and pull resampled output. Output is whatever the
    /// inner resampler produced for completed `chunk_frames` blocks; partial
    /// blocks are buffered until the next call.
    pub fn push(&mut self, mono: &[f32]) -> Result<Vec<f32>> {
        if self.from_rate == self.to_rate {
            return Ok(mono.to_vec());
        }
        self.pending.extend_from_slice(mono);

        let mut produced: Vec<f32> = Vec::new();
        while self.pending.len() >= self.chunk_frames {
            let chunk: Vec<f32> = self.pending.drain(..self.chunk_frames).collect();
            let resampler = self
                .inner
                .as_mut()
                .ok_or_else(|| SttError::Audio("resampler not initialised".into()))?;
            let mut wave = resampler
                .process(&[chunk], None)
                .map_err(|e| SttError::Audio(format!("resample failed: {e}")))?;
            if let Some(out) = wave.pop() {
                produced.extend(out);
            }
        }
        Ok(produced)
    }

    /// Drain remaining buffered samples by zero-padding the final chunk.
    pub fn flush(&mut self) -> Result<Vec<f32>> {
        if self.pending.is_empty() {
            return Ok(Vec::new());
        }
        if self.from_rate == self.to_rate {
            return Ok(std::mem::take(&mut self.pending));
        }
        let mut chunk: Vec<f32> = std::mem::take(&mut self.pending);
        chunk.resize(self.chunk_frames, 0.0);
        let resampler = self
            .inner
            .as_mut()
            .ok_or_else(|| SttError::Audio("resampler not initialised".into()))?;
        let mut wave = resampler
            .process(&[chunk], None)
            .map_err(|e| SttError::Audio(format!("resample failed: {e}")))?;
        Ok(wave.pop().unwrap_or_default())
    }

    fn ensure_resampler(&mut self) -> Result<()> {
        if self.from_rate == self.to_rate {
            self.inner = None;
            return Ok(());
        }
        let ratio = f64::from(self.to_rate) / f64::from(self.from_rate);
        let inner = SincFixedIn::<f32>::new(ratio, 2.0, sinc_params(), self.chunk_frames, 1)
            .map_err(|e| SttError::Audio(format!("resampler init failed: {e}")))?;
        self.inner = Some(inner);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_rates_match() {
        let mut r = CachedResampler::new(16_000, 16_000, 320).unwrap();
        let out = r.push(&[0.1, 0.2, 0.3]).unwrap();
        assert_eq!(out, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn produces_when_chunk_filled() {
        let chunk = 480;
        let mut r = CachedResampler::new(48_000, 16_000, chunk).unwrap();
        let mono: Vec<f32> = (0..chunk * 2).map(|i| (i as f32 * 0.001).sin()).collect();
        let first = r.push(&mono[..chunk - 1]).unwrap();
        assert!(first.is_empty(), "no output until chunk fills");
        let second = r.push(&mono[chunk - 1..]).unwrap();
        assert!(!second.is_empty());
    }
}
