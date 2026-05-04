use std::cell::UnsafeCell;
use std::cmp;
use std::fs::{create_dir_all, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use common::{AudioFrame, EngineError, Result};
use memmap2::MmapMut;

pub const SHARED_BUFFER_MAGIC: u32 = 0x314D_5654;
pub const SHARED_BUFFER_VERSION: u32 = 1;
pub const SHARED_BUFFER_FILE_PATH: &str = "/tmp/translator_virtual_mic/shared_output.bin";

// Layout: 6 x u32 + 3 x u64 = 48 bytes
const HEADER_SIZE: usize = 48;
const MAGIC_OFFSET: usize = 0;
const VERSION_OFFSET: usize = 4;
const CHANNEL_COUNT_OFFSET: usize = 8;
const SAMPLE_RATE_OFFSET: usize = 12;
const CAPACITY_FRAMES_OFFSET: usize = 16;
const RESERVED_OFFSET: usize = 20;
const WRITE_INDEX_OFFSET: usize = 24;
const READ_INDEX_OFFSET: usize = 32;
const LAST_TIMESTAMP_OFFSET: usize = 40;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SharedBufferHeader {
    pub magic: u32,
    pub version: u32,
    pub channel_count: u32,
    pub sample_rate: u32,
    pub capacity_frames: u32,
    pub reserved: u32,
    pub write_index_frames: u64,
    pub read_index_frames: u64,
    pub last_timestamp_ns: u64,
}

#[derive(Clone, Debug)]
pub struct SharedBufferSnapshot {
    pub header: SharedBufferHeader,
    pub samples: Vec<f32>,
    pub file_path: String,
}

/// File-backed shared output buffer using `mmap` for zero-copy I/O.
///
/// The file layout is identical to the original serialised format so the
/// Objective-C++ HAL plug-in remains compatible.  Instead of `seek` +
/// `write_all` + `flush` on every 20 ms frame, we `mmap` the file once
/// and write directly into the mapped memory.  `write_index_frames` and
/// `read_index_frames` are updated with `AtomicU64` so a future lock-free
/// reader (e.g. an `mmap`-aware HAL plug-in) can observe them safely.
pub struct SharedOutputBuffer {
    mmap: UnsafeCell<MmapMut>,
    capacity_samples: usize,
    channels: u16,
    sample_rate: u32,
    file_path: String,
}

unsafe impl Send for SharedOutputBuffer {}
unsafe impl Sync for SharedOutputBuffer {}

impl SharedOutputBuffer {
    pub fn new(capacity_frames: usize, channels: u16, sample_rate: u32) -> Result<Self> {
        Self::new_with_path(
            capacity_frames,
            channels,
            sample_rate,
            PathBuf::from(SHARED_BUFFER_FILE_PATH),
        )
    }

    fn new_with_path(
        capacity_frames: usize,
        channels: u16,
        sample_rate: u32,
        file_path: PathBuf,
    ) -> Result<Self> {
        if capacity_frames == 0 || channels == 0 || sample_rate == 0 {
            return Err(EngineError::new("invalid shared output buffer format"));
        }

        if let Some(parent) = file_path.parent() {
            create_dir_all(parent)
                .map_err(|err| EngineError::new(format!("create shared buffer dir: {err}")))?;
        }

        let channel_count = usize::from(channels);
        let capacity_samples = capacity_frames.saturating_mul(channel_count);
        let file_size =
            HEADER_SIZE.saturating_add(capacity_samples.saturating_mul(std::mem::size_of::<f32>()));

        let file = open_shared_file(&file_path, file_size)
            .map_err(|err| EngineError::new(format!("open shared buffer file: {err}")))?;

        let mut mmap = unsafe {
            MmapMut::map_mut(&file)
                .map_err(|err| EngineError::new(format!("mmap shared buffer: {err}")))?
        };

        // Initialise header in mapped memory.
        write_u32(&mut mmap, MAGIC_OFFSET, SHARED_BUFFER_MAGIC);
        write_u32(&mut mmap, VERSION_OFFSET, SHARED_BUFFER_VERSION);
        write_u32(&mut mmap, CHANNEL_COUNT_OFFSET, channels.into());
        write_u32(&mut mmap, SAMPLE_RATE_OFFSET, sample_rate);
        write_u32(&mut mmap, CAPACITY_FRAMES_OFFSET, capacity_frames as u32);
        write_u32(&mut mmap, RESERVED_OFFSET, 0);
        atomic_u64_at(&mmap, WRITE_INDEX_OFFSET).store(0, Ordering::Release);
        atomic_u64_at(&mmap, READ_INDEX_OFFSET).store(0, Ordering::Release);
        write_u64(&mut mmap, LAST_TIMESTAMP_OFFSET, 0);

        // Zero the sample region.
        let sample_start = HEADER_SIZE;
        let sample_end = sample_start.saturating_add(capacity_samples * std::mem::size_of::<f32>());
        if sample_end > mmap.len() {
            return Err(EngineError::new("shared buffer mmap size mismatch"));
        }
        mmap[sample_start..sample_end].fill(0);

        Ok(Self {
            mmap: UnsafeCell::new(mmap),
            capacity_samples,
            channels,
            sample_rate,
            file_path: file_path.display().to_string(),
        })
    }

    pub fn write_frame(&self, frame: &AudioFrame) -> Result<()> {
        let mmap = unsafe { &mut *self.mmap.get() };
        let expected_channels = usize::from(self.channels);
        if usize::from(frame.channels) != expected_channels {
            return Err(EngineError::new("shared output channel mismatch"));
        }
        if frame.sample_rate != self.sample_rate {
            return Err(EngineError::new("shared output sample rate mismatch"));
        }

        let frame_count = frame.frames();
        let capacity_frames = self.capacity_samples / expected_channels;
        if capacity_frames == 0 || self.capacity_samples == 0 {
            return Ok(());
        }

        let src_offset_frames = frame_count.saturating_sub(capacity_frames);
        let src = &frame.data[src_offset_frames.saturating_mul(expected_channels)..];
        let write_index = atomic_u64_at(mmap, WRITE_INDEX_OFFSET).load(Ordering::Acquire);
        let start_frame = (write_index as usize) % capacity_frames;

        // Write directly into mmap memory.
        for (index, sample) in src.iter().enumerate() {
            let frame_offset = index / expected_channels;
            let channel_offset = index % expected_channels;
            let dst_frame = (start_frame + frame_offset) % capacity_frames;
            let dst_index = dst_frame.saturating_mul(expected_channels) + channel_offset;
            let byte_offset = HEADER_SIZE.saturating_add(dst_index * std::mem::size_of::<f32>());
            let bytes = sample.to_le_bytes();
            mmap[byte_offset..byte_offset + 4].copy_from_slice(&bytes);
        }

        let new_write_index =
            write_index.saturating_add(src.len() as u64 / expected_channels as u64);
        atomic_u64_at(mmap, WRITE_INDEX_OFFSET).store(new_write_index, Ordering::Release);
        write_u64(mmap, LAST_TIMESTAMP_OFFSET, frame.timestamp_ns);

        // If the writer has lapped the reader, advance the read index so the
        // reader never sees stale data older than one buffer cycle.
        let read_index = atomic_u64_at(mmap, READ_INDEX_OFFSET).load(Ordering::Acquire);
        let unread_frames = new_write_index.saturating_sub(read_index);
        if unread_frames > capacity_frames as u64 {
            let new_read_index = new_write_index.saturating_sub(capacity_frames as u64);
            atomic_u64_at(mmap, READ_INDEX_OFFSET).store(new_read_index, Ordering::Release);
        }

        Ok(())
    }

    pub fn read_into(&self, out_samples: &mut [f32], channels: u16) -> Result<(usize, u64)> {
        let expected_channels = usize::from(self.channels);
        if usize::from(channels) != expected_channels {
            return Err(EngineError::new("shared output read channel mismatch"));
        }
        let mmap = unsafe { &*self.mmap.get() };
        if out_samples.is_empty() {
            let ts = read_u64(mmap, LAST_TIMESTAMP_OFFSET);
            return Ok((0, ts));
        }

        let capacity_frames = self.capacity_samples / expected_channels;
        let write_index = atomic_u64_at(mmap, WRITE_INDEX_OFFSET).load(Ordering::Acquire);
        let read_index = atomic_u64_at(mmap, READ_INDEX_OFFSET).load(Ordering::Acquire);

        let available_frames = cmp::min(
            capacity_frames,
            write_index.saturating_sub(read_index) as usize,
        );
        let requested_frames = out_samples.len() / expected_channels;
        let frames_to_read = cmp::min(available_frames, requested_frames);
        let start_frame = (read_index as usize) % capacity_frames;

        for frame_index in 0..frames_to_read {
            for channel_index in 0..expected_channels {
                let src_frame = (start_frame + frame_index) % capacity_frames;
                let src_index = src_frame.saturating_mul(expected_channels) + channel_index;
                let byte_offset =
                    HEADER_SIZE.saturating_add(src_index * std::mem::size_of::<f32>());
                let bytes = [
                    mmap[byte_offset],
                    mmap[byte_offset + 1],
                    mmap[byte_offset + 2],
                    mmap[byte_offset + 3],
                ];
                let dst_index = frame_index.saturating_mul(expected_channels) + channel_index;
                out_samples[dst_index] = f32::from_le_bytes(bytes);
            }
        }

        for slot in out_samples
            .iter_mut()
            .skip(frames_to_read.saturating_mul(expected_channels))
        {
            *slot = 0.0;
        }

        let new_read_index = read_index.saturating_add(frames_to_read as u64);
        atomic_u64_at(mmap, READ_INDEX_OFFSET).store(new_read_index, Ordering::Release);

        let ts = read_u64(mmap, LAST_TIMESTAMP_OFFSET);
        Ok((frames_to_read, ts))
    }

    pub fn snapshot(&self) -> SharedBufferSnapshot {
        let mmap = unsafe { &*self.mmap.get() };
        let header = self.read_header_from(mmap);
        let sample_count = self.capacity_samples;
        let mut samples = vec![0.0f32; sample_count];
        for i in 0..sample_count {
            let byte_offset = HEADER_SIZE.saturating_add(i * std::mem::size_of::<f32>());
            let bytes = [
                mmap[byte_offset],
                mmap[byte_offset + 1],
                mmap[byte_offset + 2],
                mmap[byte_offset + 3],
            ];
            samples[i] = f32::from_le_bytes(bytes);
        }
        SharedBufferSnapshot {
            header,
            samples,
            file_path: self.file_path.clone(),
        }
    }

    pub fn file_path(&self) -> String {
        self.file_path.clone()
    }

    fn read_header_from(&self, mmap: &MmapMut) -> SharedBufferHeader {
        SharedBufferHeader {
            magic: read_u32(mmap, MAGIC_OFFSET),
            version: read_u32(mmap, VERSION_OFFSET),
            channel_count: read_u32(mmap, CHANNEL_COUNT_OFFSET),
            sample_rate: read_u32(mmap, SAMPLE_RATE_OFFSET),
            capacity_frames: read_u32(mmap, CAPACITY_FRAMES_OFFSET),
            reserved: read_u32(mmap, RESERVED_OFFSET),
            write_index_frames: atomic_u64_at(mmap, WRITE_INDEX_OFFSET).load(Ordering::Acquire),
            read_index_frames: atomic_u64_at(mmap, READ_INDEX_OFFSET).load(Ordering::Acquire),
            last_timestamp_ns: read_u64(mmap, LAST_TIMESTAMP_OFFSET),
        }
    }
}

fn open_shared_file(path: &Path, byte_len: usize) -> std::io::Result<File> {
    // If the file already exists and has the expected size, reuse it so that
    // concurrent tests (or a restarted engine) do not invalidate existing mmap
    // mappings via truncate.
    if let Ok(metadata) = std::fs::metadata(path) {
        if metadata.len() == byte_len as u64 {
            return OpenOptions::new().read(true).write(true).open(path);
        }
    }
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    file.set_len(byte_len as u64)?;
    Ok(file)
}

// ---------------------------------------------------------------------------
// Unsafe helpers to treat mmap bytes as atomics / integers.
// ---------------------------------------------------------------------------

/// SAFETY: `offset` must be 8-byte aligned and within the mmap bounds.
fn atomic_u64_at(mmap: &MmapMut, offset: usize) -> &AtomicU64 {
    unsafe {
        let ptr = mmap.as_ptr().add(offset) as *const AtomicU64;
        &*ptr
    }
}

fn read_u32(mmap: &MmapMut, offset: usize) -> u32 {
    let bytes = [
        mmap[offset],
        mmap[offset + 1],
        mmap[offset + 2],
        mmap[offset + 3],
    ];
    u32::from_le_bytes(bytes)
}

fn write_u32(mmap: &mut MmapMut, offset: usize, value: u32) {
    let bytes = value.to_le_bytes();
    mmap[offset..offset + 4].copy_from_slice(&bytes);
}

fn read_u64(mmap: &MmapMut, offset: usize) -> u64 {
    let bytes = [
        mmap[offset],
        mmap[offset + 1],
        mmap[offset + 2],
        mmap[offset + 3],
        mmap[offset + 4],
        mmap[offset + 5],
        mmap[offset + 6],
        mmap[offset + 7],
    ];
    u64::from_le_bytes(bytes)
}

fn write_u64(mmap: &mut MmapMut, offset: usize, value: u64) {
    let bytes = value.to_le_bytes();
    mmap[offset..offset + 8].copy_from_slice(&bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_buffer_path() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("translator_virtual_mic_test_{id}.bin"))
    }

    #[test]
    fn writes_and_reads_interleaved_pcm() {
        let path = temp_buffer_path();
        let mut buffer =
            SharedOutputBuffer::new_with_path(4, 1, 48_000, path.clone()).expect("buffer");
        let frame = AudioFrame {
            timestamp_ns: 55,
            sample_rate: 48_000,
            channels: 1,
            data: vec![0.1, 0.2, 0.3],
        };

        buffer.write_frame(&frame).expect("write");
        let mut out = vec![0.0; 4];
        let (frames, ts) = buffer.read_into(&mut out, 1).expect("read");

        assert_eq!(frames, 3);
        assert_eq!(ts, 55);
        assert_eq!(&out[..3], &[0.1, 0.2, 0.3]);
        assert_eq!(out[3], 0.0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn wraps_on_overflow() {
        let path = temp_buffer_path();
        let mut buffer =
            SharedOutputBuffer::new_with_path(4, 1, 48_000, path.clone()).expect("buffer");

        // Fill buffer with 4 frames.
        for i in 0..4 {
            let frame = AudioFrame {
                timestamp_ns: i as u64 * 10,
                sample_rate: 48_000,
                channels: 1,
                data: vec![i as f32 + 1.0],
            };
            buffer.write_frame(&frame).unwrap();
        }

        // Write one more — oldest frame (1.0) should be overwritten.
        buffer
            .write_frame(&AudioFrame {
                timestamp_ns: 50,
                sample_rate: 48_000,
                channels: 1,
                data: vec![5.0],
            })
            .unwrap();

        // Read all 4 frames. Because we haven't consumed any, read_index
        // should have been bumped forward to keep only the latest 4.
        let mut out = vec![0.0; 4];
        let (frames, _ts) = buffer.read_into(&mut out, 1).unwrap();
        assert_eq!(frames, 4);
        // Expected order: 2.0, 3.0, 4.0, 5.0 (1.0 was overwritten)
        assert_eq!(out[0], 2.0);
        assert_eq!(out[1], 3.0);
        assert_eq!(out[2], 4.0);
        assert_eq!(out[3], 5.0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn snapshot_matches_header_and_samples() {
        let path = temp_buffer_path();
        let mut buffer =
            SharedOutputBuffer::new_with_path(2, 1, 48_000, path.clone()).expect("buffer");
        let frame = AudioFrame {
            timestamp_ns: 99,
            sample_rate: 48_000,
            channels: 1,
            data: vec![0.5, 0.6],
        };
        buffer.write_frame(&frame).unwrap();

        let snap = buffer.snapshot();
        assert_eq!(snap.header.magic, SHARED_BUFFER_MAGIC);
        assert_eq!(snap.header.version, SHARED_BUFFER_VERSION);
        assert_eq!(snap.header.sample_rate, 48_000);
        assert_eq!(snap.header.channel_count, 1);
        assert_eq!(snap.header.capacity_frames, 2);
        assert_eq!(snap.header.write_index_frames, 2);
        assert_eq!(snap.samples.len(), 2);
        assert_eq!(snap.samples[0], 0.5);
        assert_eq!(snap.samples[1], 0.6);
        let _ = std::fs::remove_file(&path);
    }
}
