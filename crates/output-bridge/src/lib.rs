use std::cmp;
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use common::{AudioFrame, EngineError, Result};

pub const SHARED_BUFFER_MAGIC: u32 = 0x314D_5654;
pub const SHARED_BUFFER_VERSION: u32 = 1;
pub const SHARED_BUFFER_FILE_PATH: &str = "/tmp/translator_virtual_mic/shared_output.bin";

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

#[derive(Debug)]
pub struct SharedOutputBuffer {
    inner: Mutex<SharedState>,
}

#[derive(Debug)]
struct SharedState {
    file: File,
    file_path: PathBuf,
    header: SharedBufferHeader,
    samples: Vec<f32>,
}

impl SharedOutputBuffer {
    pub fn new(capacity_frames: usize, channels: u16, sample_rate: u32) -> Result<Self> {
        if capacity_frames == 0 || channels == 0 || sample_rate == 0 {
            return Err(EngineError::new("invalid shared output buffer format"));
        }

        let file_path = PathBuf::from(SHARED_BUFFER_FILE_PATH);
        if let Some(parent) = file_path.parent() {
            create_dir_all(parent)
                .map_err(|err| EngineError::new(format!("create shared buffer dir: {err}")))?;
        }

        let channel_count = usize::from(channels);
        let samples = vec![0.0; capacity_frames.saturating_mul(channel_count)];
        let header = SharedBufferHeader {
            magic: SHARED_BUFFER_MAGIC,
            version: SHARED_BUFFER_VERSION,
            channel_count: channels.into(),
            sample_rate,
            capacity_frames: capacity_frames as u32,
            reserved: 0,
            write_index_frames: 0,
            read_index_frames: 0,
            last_timestamp_ns: 0,
        };

        let file = open_shared_file(&file_path, total_bytes(samples.len()))
            .map_err(|err| EngineError::new(format!("open shared buffer file: {err}")))?;

        let mut state = SharedState {
            file,
            file_path,
            header,
            samples,
        };
        persist_state(&mut state)?;

        Ok(Self {
            inner: Mutex::new(state),
        })
    }

    pub fn write_frame(&self, frame: &AudioFrame) -> Result<()> {
        let mut state = self.inner.lock().expect("shared output buffer poisoned");
        let expected_channels = state.header.channel_count as usize;
        if usize::from(frame.channels) != expected_channels {
            return Err(EngineError::new("shared output channel mismatch"));
        }
        if frame.sample_rate != state.header.sample_rate {
            return Err(EngineError::new("shared output sample rate mismatch"));
        }

        let frame_count = frame.frames();
        let capacity_frames = state.header.capacity_frames as usize;
        if capacity_frames == 0 || state.samples.is_empty() {
            return Ok(());
        }

        let src_offset_frames = frame_count.saturating_sub(capacity_frames);
        let src = &frame.data[src_offset_frames.saturating_mul(expected_channels)..];
        let start_frame = (state.header.write_index_frames as usize) % capacity_frames;

        for (index, sample) in src.iter().enumerate() {
            let frame_offset = index / expected_channels;
            let channel_offset = index % expected_channels;
            let dst_frame = (start_frame + frame_offset) % capacity_frames;
            let dst_index = dst_frame.saturating_mul(expected_channels) + channel_offset;
            state.samples[dst_index] = *sample;
        }

        state.header.write_index_frames = state
            .header
            .write_index_frames
            .saturating_add(src.len() as u64 / expected_channels as u64);
        state.header.last_timestamp_ns = frame.timestamp_ns;

        let unread_frames = state
            .header
            .write_index_frames
            .saturating_sub(state.header.read_index_frames);
        if unread_frames > capacity_frames as u64 {
            state.header.read_index_frames = state
                .header
                .write_index_frames
                .saturating_sub(capacity_frames as u64);
        }

        persist_state(&mut state)
    }

    pub fn read_into(&self, out_samples: &mut [f32], channels: u16) -> Result<(usize, u64)> {
        let mut state = self.inner.lock().expect("shared output buffer poisoned");
        load_state(&mut state)?;

        let expected_channels = state.header.channel_count as usize;
        if usize::from(channels) != expected_channels {
            return Err(EngineError::new("shared output read channel mismatch"));
        }
        if out_samples.is_empty() {
            return Ok((0, state.header.last_timestamp_ns));
        }

        let capacity_frames = state.header.capacity_frames as usize;
        let available_frames = cmp::min(
            capacity_frames,
            state
                .header
                .write_index_frames
                .saturating_sub(state.header.read_index_frames) as usize,
        );
        let requested_frames = out_samples.len() / expected_channels;
        let frames_to_read = cmp::min(available_frames, requested_frames);
        let start_frame = (state.header.read_index_frames as usize) % capacity_frames;

        for frame_index in 0..frames_to_read {
            for channel_index in 0..expected_channels {
                let src_frame = (start_frame + frame_index) % capacity_frames;
                let src_index = src_frame.saturating_mul(expected_channels) + channel_index;
                let dst_index = frame_index.saturating_mul(expected_channels) + channel_index;
                out_samples[dst_index] = state.samples[src_index];
            }
        }

        for slot in out_samples
            .iter_mut()
            .skip(frames_to_read.saturating_mul(expected_channels))
        {
            *slot = 0.0;
        }

        state.header.read_index_frames = state
            .header
            .read_index_frames
            .saturating_add(frames_to_read as u64);
        persist_state(&mut state)?;
        Ok((frames_to_read, state.header.last_timestamp_ns))
    }

    pub fn snapshot(&self) -> SharedBufferSnapshot {
        let state = self.inner.lock().expect("shared output buffer poisoned");
        SharedBufferSnapshot {
            header: state.header,
            samples: state.samples.clone(),
            file_path: state.file_path.display().to_string(),
        }
    }

    pub fn file_path(&self) -> String {
        let state = self.inner.lock().expect("shared output buffer poisoned");
        state.file_path.display().to_string()
    }
}

fn open_shared_file(path: &Path, byte_len: usize) -> std::io::Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    file.set_len(byte_len as u64)?;
    Ok(file)
}

fn header_byte_len() -> usize {
    (6 * std::mem::size_of::<u32>()) + (3 * std::mem::size_of::<u64>())
}

fn total_bytes(sample_count: usize) -> usize {
    header_byte_len() + sample_count.saturating_mul(std::mem::size_of::<f32>())
}

fn persist_state(state: &mut SharedState) -> Result<()> {
    state
        .file
        .seek(SeekFrom::Start(0))
        .map_err(|err| EngineError::new(format!("seek shared buffer file: {err}")))?;

    let mut bytes = Vec::with_capacity(total_bytes(state.samples.len()));
    push_u32(&mut bytes, state.header.magic);
    push_u32(&mut bytes, state.header.version);
    push_u32(&mut bytes, state.header.channel_count);
    push_u32(&mut bytes, state.header.sample_rate);
    push_u32(&mut bytes, state.header.capacity_frames);
    push_u32(&mut bytes, state.header.reserved);
    push_u64(&mut bytes, state.header.write_index_frames);
    push_u64(&mut bytes, state.header.read_index_frames);
    push_u64(&mut bytes, state.header.last_timestamp_ns);
    for sample in &state.samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }

    state
        .file
        .write_all(&bytes)
        .and_then(|_| state.file.flush())
        .map_err(|err| EngineError::new(format!("write shared buffer file: {err}")))?;

    Ok(())
}

fn load_state(state: &mut SharedState) -> Result<()> {
    state
        .file
        .seek(SeekFrom::Start(0))
        .map_err(|err| EngineError::new(format!("seek shared buffer file: {err}")))?;

    let mut header_bytes = vec![0u8; header_byte_len()];
    state
        .file
        .read_exact(&mut header_bytes)
        .map_err(|err| EngineError::new(format!("read shared buffer header: {err}")))?;

    let mut cursor = 0usize;
    state.header.magic = read_u32(&header_bytes, &mut cursor);
    state.header.version = read_u32(&header_bytes, &mut cursor);
    state.header.channel_count = read_u32(&header_bytes, &mut cursor);
    state.header.sample_rate = read_u32(&header_bytes, &mut cursor);
    state.header.capacity_frames = read_u32(&header_bytes, &mut cursor);
    state.header.reserved = read_u32(&header_bytes, &mut cursor);
    state.header.write_index_frames = read_u64(&header_bytes, &mut cursor);
    state.header.read_index_frames = read_u64(&header_bytes, &mut cursor);
    state.header.last_timestamp_ns = read_u64(&header_bytes, &mut cursor);

    let sample_count = state.samples.len();
    let mut sample_bytes = vec![0u8; sample_count.saturating_mul(std::mem::size_of::<f32>())];
    state
        .file
        .read_exact(&mut sample_bytes)
        .map_err(|err| EngineError::new(format!("read shared buffer samples: {err}")))?;

    for (index, chunk) in sample_bytes.chunks_exact(4).enumerate() {
        state.samples[index] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }

    Ok(())
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> u32 {
    let start = *cursor;
    *cursor += 4;
    u32::from_le_bytes(bytes[start..*cursor].try_into().expect("u32 slice"))
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> u64 {
    let start = *cursor;
    *cursor += 8;
    u64::from_le_bytes(bytes[start..*cursor].try_into().expect("u64 slice"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_interleaved_pcm() {
        let buffer = SharedOutputBuffer::new(4, 1, 48_000).expect("buffer");
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
        assert_eq!(buffer.file_path(), SHARED_BUFFER_FILE_PATH);
    }
}
