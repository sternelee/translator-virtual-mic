use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use common::{AudioFrame, Result};

/// Lock-free single-producer single-consumer ring buffer for interleaved
/// audio samples.
///
/// The producer calls `push_frame` and the consumer calls `pop_into`.
/// Both indices are `AtomicU64` monotonic counters; the actual slot is
/// `index % capacity_samples`.  When the buffer is full the oldest samples
/// are overwritten (read_index is advanced) so the ring never blocks.
///
/// All atomic operations use Acquire/Release semantics so that sample
/// writes are visible to the reader without a mutex.
#[derive(Debug)]
pub struct SampleRingBuffer {
    channels: AtomicU32,
    sample_rate: AtomicU32,
    capacity_samples: usize,
    buffer: Vec<f32>,
    write_index: AtomicU64,
    read_index: AtomicU64,
    last_timestamp_ns: AtomicU64,
}

impl SampleRingBuffer {
    pub fn new(capacity_frames: usize, channels: u16, sample_rate: u32) -> Self {
        let channels = channels.max(1);
        let capacity_samples = capacity_frames.saturating_mul(usize::from(channels)).max(1);
        Self {
            channels: AtomicU32::new(channels as u32),
            sample_rate: AtomicU32::new(sample_rate),
            capacity_samples,
            buffer: vec![0.0f32; capacity_samples],
            write_index: AtomicU64::new(0),
            read_index: AtomicU64::new(0),
            last_timestamp_ns: AtomicU64::new(0),
        }
    }

    pub fn push_frame(&self, frame: &AudioFrame) -> Result<usize> {
        self.channels
            .store(frame.channels.max(1) as u32, Ordering::Relaxed);
        self.sample_rate.store(frame.sample_rate, Ordering::Relaxed);
        self.last_timestamp_ns
            .store(frame.timestamp_ns, Ordering::Relaxed);

        let to_write = frame.data.len();
        if to_write == 0 {
            return Ok(0);
        }

        let write_idx = self.write_index.load(Ordering::Relaxed);
        let read_idx = self.read_index.load(Ordering::Acquire);
        let available = write_idx.saturating_sub(read_idx) as usize;
        let space = self.capacity_samples.saturating_sub(available);

        let dropped = if to_write > space {
            let drop = to_write - space;
            // Advance read_index so the overwritten samples are no longer
            // visible to the reader.  fetch_add is safe because both writer
            // and reader only ever move this index forward.
            self.read_index.fetch_add(drop as u64, Ordering::Release);
            drop
        } else {
            0
        };

        // SAFETY: The SPSC invariant guarantees that the writer and reader
        // never access the same slot simultaneously.  Writer indices are
        // always >= read_index, and the reader only reads slots < write_index.
        // The buffer pointer is stable because Vec capacity never changes.
        let buffer_ptr = self.buffer.as_ptr() as *mut f32;
        for (i, &sample) in frame.data.iter().enumerate() {
            let slot = (write_idx.wrapping_add(i as u64) as usize) % self.capacity_samples;
            unsafe { buffer_ptr.add(slot).write(sample) };
        }

        self.write_index
            .store(write_idx.wrapping_add(to_write as u64), Ordering::Release);
        Ok(dropped)
    }

    pub fn pop_into(&self, out: &mut [f32], requested_channels: u16) -> usize {
        let requested_channels = usize::from(requested_channels.max(1));
        let requested_samples = out.len();
        if requested_samples == 0 {
            return 0;
        }

        let mut read_idx = self.read_index.load(Ordering::Relaxed);
        let write_idx = self.write_index.load(Ordering::Acquire);
        let available = write_idx.saturating_sub(read_idx) as usize;
        let to_read = requested_samples.min(available);

        for slot in out.iter_mut().take(to_read) {
            let buf_slot = (read_idx as usize) % self.capacity_samples;
            *slot = self.buffer[buf_slot];
            read_idx = read_idx.wrapping_add(1);
        }

        for slot in out.iter_mut().skip(to_read) {
            *slot = 0.0;
        }

        self.read_index.store(read_idx, Ordering::Release);
        to_read / requested_channels
    }

    pub fn available_frames(&self) -> usize {
        let write_idx = self.write_index.load(Ordering::Acquire);
        let read_idx = self.read_index.load(Ordering::Acquire);
        let channels = self.channels.load(Ordering::Relaxed).max(1) as usize;
        let samples = write_idx.saturating_sub(read_idx) as usize;
        samples / channels
    }

    pub fn last_timestamp_ns(&self) -> u64 {
        self.last_timestamp_ns.load(Ordering::Relaxed)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }
}

pub fn build_frame(
    samples: &[f32],
    frame_count: usize,
    channels: u16,
    sample_rate: u32,
    timestamp_ns: u64,
) -> AudioFrame {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_round_trip() {
        let ring = SampleRingBuffer::new(10, 1, 48_000);
        let frame = AudioFrame {
            timestamp_ns: 1,
            sample_rate: 48_000,
            channels: 1,
            data: vec![0.1, 0.2, 0.3, 0.4, 0.5],
        };
        let dropped = ring.push_frame(&frame).unwrap();
        assert_eq!(dropped, 0);
        assert_eq!(ring.available_frames(), 5);

        let mut out = [0.0f32; 5];
        let frames = ring.pop_into(&mut out, 1);
        assert_eq!(frames, 5);
        assert_eq!(out, [0.1, 0.2, 0.3, 0.4, 0.5]);
        assert_eq!(ring.available_frames(), 0);
    }

    #[test]
    fn pop_zero_fills_on_underrun() {
        let ring = SampleRingBuffer::new(10, 1, 48_000);
        let frame = AudioFrame {
            timestamp_ns: 1,
            sample_rate: 48_000,
            channels: 1,
            data: vec![0.1, 0.2],
        };
        ring.push_frame(&frame).unwrap();

        let mut out = [0.0f32; 5];
        let frames = ring.pop_into(&mut out, 1);
        assert_eq!(frames, 2);
        assert_eq!(out, [0.1, 0.2, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn overwrite_drops_oldest() {
        let ring = SampleRingBuffer::new(4, 1, 48_000);
        // Fill buffer
        ring.push_frame(&AudioFrame {
            timestamp_ns: 1,
            sample_rate: 48_000,
            channels: 1,
            data: vec![1.0, 2.0, 3.0, 4.0],
        })
        .unwrap();
        assert_eq!(ring.available_frames(), 4);

        // Overwrite with 2 new samples
        let dropped = ring
            .push_frame(&AudioFrame {
                timestamp_ns: 2,
                sample_rate: 48_000,
                channels: 1,
                data: vec![5.0, 6.0],
            })
            .unwrap();
        assert_eq!(dropped, 2);
        assert_eq!(ring.available_frames(), 4);

        let mut out = [0.0f32; 4];
        ring.pop_into(&mut out, 1);
        assert_eq!(out, [3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn wrap_around_index() {
        let ring = SampleRingBuffer::new(4, 1, 48_000);
        // Write 6 samples to force wrap-around
        let dropped = ring
            .push_frame(&AudioFrame {
                timestamp_ns: 1,
                sample_rate: 48_000,
                channels: 1,
                data: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            })
            .unwrap();
        assert_eq!(dropped, 2);
        assert_eq!(ring.available_frames(), 4);

        let mut out = [0.0f32; 4];
        ring.pop_into(&mut out, 1);
        assert_eq!(out, [3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn metadata_tracked() {
        let ring = SampleRingBuffer::new(10, 2, 48_000);
        ring.push_frame(&AudioFrame {
            timestamp_ns: 42,
            sample_rate: 44_100,
            channels: 1,
            data: vec![0.5],
        })
        .unwrap();
        assert_eq!(ring.last_timestamp_ns(), 42);
        assert_eq!(ring.sample_rate(), 44_100);
        // channels stored as max(1), so 1 from frame is preserved
        assert_eq!(ring.available_frames(), 1);
    }

    #[test]
    fn multi_channel_frame_count() {
        let ring = SampleRingBuffer::new(10, 2, 48_000);
        // 6 samples = 3 stereo frames
        ring.push_frame(&AudioFrame {
            timestamp_ns: 1,
            sample_rate: 48_000,
            channels: 2,
            data: vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6],
        })
        .unwrap();
        assert_eq!(ring.available_frames(), 3);

        // Request 2 frames (4 samples) in stereo
        let mut out = [0.0f32; 4];
        let frames = ring.pop_into(&mut out, 2);
        assert_eq!(frames, 2);
        assert_eq!(out, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(ring.available_frames(), 1);
    }

    #[test]
    fn empty_push_is_no_op() {
        let ring = SampleRingBuffer::new(10, 1, 48_000);
        let dropped = ring
            .push_frame(&AudioFrame {
                timestamp_ns: 1,
                sample_rate: 48_000,
                channels: 1,
                data: vec![],
            })
            .unwrap();
        assert_eq!(dropped, 0);
        assert_eq!(ring.available_frames(), 0);
    }
}
