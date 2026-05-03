use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::slice;
use std::sync::{Mutex, Once};

use common::{EngineConfig, EngineMode};
use session_core::EngineSession;

static INIT_STDIO: Once = Once::new();

/// Force stderr to line-buffered mode so logs flush immediately when the
/// engine is loaded from a GUI bundle (where stderr defaults to fully
/// buffered).  Called once on first `engine_create`.
fn ensure_line_buffered_stderr() {
    INIT_STDIO.call_once(|| {
        // SAFETY: stderr is a valid FILE*, and _IOLBF is a valid mode.
        unsafe {
            let fp = stderr_file();
            if !fp.is_null() {
                libc::setvbuf(fp, std::ptr::null_mut(), libc::_IOLBF, 0);
            }
        }
    });
}

#[cfg(target_os = "macos")]
unsafe fn stderr_file() -> *mut libc::FILE {
    extern "C" {
        static __stderrp: *mut libc::FILE;
    }
    __stderrp
}

#[cfg(all(target_family = "unix", not(target_os = "macos")))]
unsafe fn stderr_file() -> *mut libc::FILE {
    extern "C" {
        static stderr: *mut libc::FILE;
    }
    stderr
}

#[cfg(not(target_family = "unix"))]
unsafe fn stderr_file() -> *mut libc::FILE {
    std::ptr::null_mut()
}

pub struct EngineHandle {
    session: Mutex<EngineSession>,
    last_error: Mutex<CString>,
    metrics_json: Mutex<CString>,
    shared_output_path: Mutex<CString>,
    translation_state_json: Mutex<CString>,
    caption_state_json: Mutex<CString>,
}

impl EngineHandle {
    fn new(config_json: &str) -> Self {
        let config = EngineConfig::from_json_lossy(config_json);
        let session = EngineSession::new(config);
        Self {
            session: Mutex::new(session),
            last_error: Mutex::new(cstring_clean("")),
            metrics_json: Mutex::new(cstring_clean("{}")),
            shared_output_path: Mutex::new(cstring_clean("")),
            translation_state_json: Mutex::new(cstring_clean("{}")),
            caption_state_json: Mutex::new(cstring_clean("{}")),
        }
    }

    fn set_last_error(&self, message: &str) {
        *self.last_error.lock().expect("last_error poisoned") = cstring_clean(message);
    }

    fn update_metrics_cache(&self) {
        let metrics = self
            .session
            .lock()
            .expect("session poisoned")
            .metrics_json();
        *self.metrics_json.lock().expect("metrics poisoned") = cstring_clean(&metrics);
    }

    fn update_shared_output_path_cache(&self) {
        let path = self
            .session
            .lock()
            .expect("session poisoned")
            .shared_output_path()
            .unwrap_or_default();
        *self
            .shared_output_path
            .lock()
            .expect("shared_output_path poisoned") = cstring_clean(&path);
    }

    fn update_translation_state_cache(&self) {
        let state_json = {
            let session = self.session.lock().expect("session poisoned");
            if let Some(state) = session.azure_voice_live_state() {
                format!(
                    "{{\"audio_delta_count\":{},\"audio_done_count\":{},\"transcript_delta_count\":{},\"translated_audio_samples\":{},\"last_response_id\":\"{}\",\"last_item_id\":\"{}\"}}",
                    state.audio_delta_count,
                    state.audio_done_count,
                    state.transcript_delta_count,
                    state.translated_audio_samples,
                    state.last_response_id,
                    state.last_item_id
                )
            } else if let Some(state) = session.openai_realtime_state() {
                format!(
                    "{{\"audio_delta_count\":{},\"audio_done_count\":{},\"transcript_delta_count\":{},\"translated_audio_samples\":{},\"last_response_id\":\"{}\",\"last_item_id\":\"{}\"}}",
                    state.audio_delta_count,
                    state.audio_done_count,
                    state.transcript_delta_count,
                    state.translated_audio_samples,
                    state.last_response_id,
                    state.last_item_id
                )
            } else {
                "{}".to_string()
            }
        };
        *self
            .translation_state_json
            .lock()
            .expect("translation_state_json poisoned") = cstring_clean(&state_json);
    }

    fn update_caption_state_cache(&self) {
        let state_json = self
            .session
            .lock()
            .expect("session poisoned")
            .caption_state_json();
        *self
            .caption_state_json
            .lock()
            .expect("caption_state_json poisoned") = cstring_clean(&state_json);
    }
}

fn cstring_clean(input: &str) -> CString {
    let cleaned = input.replace('\0', " ");
    CString::new(cleaned).expect("sanitized CString")
}

fn read_optional_cstr(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

fn with_handle<T>(
    handle: *mut EngineHandle,
    f: impl FnOnce(&EngineHandle) -> Result<T, String>,
) -> Result<T, i32> {
    if handle.is_null() {
        return Err(-1);
    }
    let handle = unsafe { &*handle };
    match f(handle) {
        Ok(value) => Ok(value),
        Err(error) => {
            handle.set_last_error(&error);
            Err(-1)
        }
    }
}

#[no_mangle]
pub extern "C" fn engine_create(config_json: *const c_char) -> *mut EngineHandle {
    ensure_line_buffered_stderr();
    let config_json = read_optional_cstr(config_json);
    Box::into_raw(Box::new(EngineHandle::new(&config_json)))
}

#[no_mangle]
pub extern "C" fn engine_destroy(handle: *mut EngineHandle) {
    if handle.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(handle));
    }
}

#[no_mangle]
pub extern "C" fn engine_start(handle: *mut EngineHandle) -> i32 {
    with_handle(handle, |handle| {
        handle
            .session
            .lock()
            .expect("session poisoned")
            .start()
            .map_err(|err| err.to_string())?;
        handle.update_metrics_cache();
        Ok(())
    })
    .map(|_| 0)
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn engine_stop(handle: *mut EngineHandle) -> i32 {
    with_handle(handle, |handle| {
        handle.session.lock().expect("session poisoned").stop();
        handle.update_metrics_cache();
        Ok(())
    })
    .map(|_| 0)
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn engine_set_target_language(
    handle: *mut EngineHandle,
    lang: *const c_char,
) -> i32 {
    with_handle(handle, |handle| {
        let lang = read_optional_cstr(lang);
        handle
            .session
            .lock()
            .expect("session poisoned")
            .set_target_language(if lang.is_empty() { "en" } else { &lang });
        Ok(())
    })
    .map(|_| 0)
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn engine_set_mode(handle: *mut EngineHandle, mode: i32) -> i32 {
    with_handle(handle, |handle| {
        let mode = EngineMode::from_i32(mode).ok_or_else(|| "invalid engine mode".to_string())?;
        handle
            .session
            .lock()
            .expect("session poisoned")
            .set_mode(mode);
        Ok(())
    })
    .map(|_| 0)
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn engine_enable_shared_output(
    handle: *mut EngineHandle,
    capacity_frames: i32,
    channels: i32,
    sample_rate: i32,
) -> i32 {
    with_handle(handle, |handle| {
        if capacity_frames <= 0 || channels <= 0 || sample_rate <= 0 {
            return Err("invalid shared output format".to_string());
        }
        handle
            .session
            .lock()
            .expect("session poisoned")
            .enable_shared_output(
                capacity_frames as usize,
                channels as u16,
                sample_rate as u32,
            )
            .map_err(|err| err.to_string())?;
        handle.update_shared_output_path_cache();
        Ok(())
    })
    .map(|_| 0)
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn engine_push_input_pcm(
    handle: *mut EngineHandle,
    samples: *const f32,
    frame_count: i32,
    channels: i32,
    sample_rate: i32,
    timestamp_ns: u64,
) -> i32 {
    with_handle(handle, |handle| {
        if samples.is_null() {
            return Err("samples pointer is null".to_string());
        }
        if frame_count <= 0 || channels <= 0 || sample_rate <= 0 {
            return Err("invalid PCM shape".to_string());
        }

        let sample_len = (frame_count as usize).saturating_mul(channels as usize);
        let slice = unsafe { slice::from_raw_parts(samples, sample_len) };
        handle
            .session
            .lock()
            .expect("session poisoned")
            .push_input_pcm(
                slice,
                frame_count as usize,
                channels as u16,
                sample_rate as u32,
                timestamp_ns,
            )
            .map_err(|err| err.to_string())?;
        handle.update_metrics_cache();
        Ok(())
    })
    .map(|_| 0)
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn engine_pull_output_pcm(
    handle: *mut EngineHandle,
    out_samples: *mut f32,
    max_frames: i32,
    channels: i32,
    sample_rate: i32,
    out_timestamp_ns: *mut u64,
) -> i32 {
    with_handle(handle, |handle| {
        if out_samples.is_null() {
            return Err("out_samples pointer is null".to_string());
        }
        if max_frames <= 0 || channels <= 0 || sample_rate <= 0 {
            return Err("invalid output shape".to_string());
        }

        let sample_len = (max_frames as usize).saturating_mul(channels as usize);
        let out_slice = unsafe { slice::from_raw_parts_mut(out_samples, sample_len) };
        let timestamp = handle
            .session
            .lock()
            .expect("session poisoned")
            .pull_output_pcm(out_slice, channels as u16, sample_rate as u32)
            .map_err(|err| err.to_string())?;

        if !out_timestamp_ns.is_null() {
            unsafe {
                ptr::write(out_timestamp_ns, timestamp);
            }
        }
        handle.update_metrics_cache();
        Ok(())
    })
    .map(|_| 0)
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn engine_read_shared_output_pcm(
    handle: *mut EngineHandle,
    out_samples: *mut f32,
    max_frames: i32,
    channels: i32,
    out_timestamp_ns: *mut u64,
) -> i32 {
    with_handle(handle, |handle| {
        if out_samples.is_null() {
            return Err("out_samples pointer is null".to_string());
        }
        if max_frames <= 0 || channels <= 0 {
            return Err("invalid shared output read shape".to_string());
        }

        let sample_len = (max_frames as usize).saturating_mul(channels as usize);
        let out_slice = unsafe { slice::from_raw_parts_mut(out_samples, sample_len) };
        let (frames_read, timestamp) = handle
            .session
            .lock()
            .expect("session poisoned")
            .read_shared_output_pcm(out_slice, channels as u16)
            .map_err(|err| err.to_string())?;

        if !out_timestamp_ns.is_null() {
            unsafe {
                ptr::write(out_timestamp_ns, timestamp);
            }
        }
        Ok(frames_read as i32)
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn engine_get_last_error(handle: *mut EngineHandle) -> *const c_char {
    if handle.is_null() {
        return ptr::null();
    }
    let handle = unsafe { &*handle };
    handle
        .last_error
        .lock()
        .expect("last_error poisoned")
        .as_ptr()
}

#[no_mangle]
pub extern "C" fn engine_get_metrics_json(handle: *mut EngineHandle) -> *const c_char {
    if handle.is_null() {
        return ptr::null();
    }
    let handle = unsafe { &*handle };
    handle.update_metrics_cache();
    handle
        .metrics_json
        .lock()
        .expect("metrics poisoned")
        .as_ptr()
}

#[no_mangle]
pub extern "C" fn engine_get_shared_output_path(handle: *mut EngineHandle) -> *const c_char {
    if handle.is_null() {
        return ptr::null();
    }
    let handle = unsafe { &*handle };
    handle.update_shared_output_path_cache();
    handle
        .shared_output_path
        .lock()
        .expect("shared_output_path poisoned")
        .as_ptr()
}

#[no_mangle]
pub extern "C" fn engine_take_next_translation_event(
    handle: *mut EngineHandle,
    out_json: *mut c_char,
    max_len: i32,
) -> i32 {
    with_handle(handle, |handle| {
        if out_json.is_null() {
            return Err("out_json pointer is null".to_string());
        }
        if max_len <= 0 {
            return Err("max_len must be positive".to_string());
        }

        let maybe_event = handle
            .session
            .lock()
            .expect("session poisoned")
            .take_next_azure_voice_live_event()
            .map_err(|err| err.to_string())?;
        let Some(event) = maybe_event else {
            unsafe { ptr::write(out_json, 0) };
            return Ok(0);
        };

        let bytes = event.as_bytes();
        let writable = (max_len as usize).saturating_sub(1);
        let copy_len = writable.min(bytes.len());
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), out_json, copy_len);
            ptr::write(out_json.add(copy_len), 0);
        }
        Ok(copy_len as i32)
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn engine_ingest_translation_event(
    handle: *mut EngineHandle,
    event_json: *const c_char,
) -> i32 {
    with_handle(handle, |handle| {
        let event_json = read_optional_cstr(event_json);
        if event_json.is_empty() {
            return Err("event_json is empty".to_string());
        }
        let frames = handle
            .session
            .lock()
            .expect("session poisoned")
            .ingest_azure_voice_live_server_event(&event_json)
            .map_err(|err| err.to_string())?;
        handle.update_translation_state_cache();
        handle.update_metrics_cache();
        Ok(frames as i32)
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn engine_get_translation_state_json(handle: *mut EngineHandle) -> *const c_char {
    if handle.is_null() {
        return ptr::null();
    }
    let handle = unsafe { &*handle };
    handle.update_translation_state_cache();
    handle
        .translation_state_json
        .lock()
        .expect("translation_state_json poisoned")
        .as_ptr()
}

#[no_mangle]
pub extern "C" fn engine_push_translated_pcm(
    handle: *mut EngineHandle,
    samples: *const f32,
    frame_count: i32,
    channels: i32,
    sample_rate: i32,
    timestamp_ns: u64,
) -> i32 {
    with_handle(handle, |handle| {
        if samples.is_null() {
            return Err("samples pointer is null".to_string());
        }
        if frame_count <= 0 || channels <= 0 || sample_rate <= 0 {
            return Err("invalid PCM shape".to_string());
        }

        let sample_len = (frame_count as usize).saturating_mul(channels as usize);
        let slice = unsafe { slice::from_raw_parts(samples, sample_len) };
        handle
            .session
            .lock()
            .expect("session poisoned")
            .push_translated_output(slice.to_vec(), timestamp_ns)
            .map_err(|err| err.to_string())?;
        handle.update_metrics_cache();
        Ok(())
    })
    .map(|_| 0)
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn engine_take_next_caption_event(
    handle: *mut EngineHandle,
    out_json: *mut c_char,
    max_len: i32,
) -> i32 {
    with_handle(handle, |handle| {
        if out_json.is_null() {
            return Err("out_json pointer is null".to_string());
        }
        if max_len <= 0 {
            return Err("max_len must be positive".to_string());
        }

        let maybe_event = handle
            .session
            .lock()
            .expect("session poisoned")
            .take_next_caption_event()
            .map_err(|err| err.to_string())?;
        let Some(event) = maybe_event else {
            unsafe { ptr::write(out_json, 0) };
            return Ok(0);
        };

        let bytes = event.as_bytes();
        let writable = (max_len as usize).saturating_sub(1);
        let copy_len = writable.min(bytes.len());
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), out_json, copy_len);
            ptr::write(out_json.add(copy_len), 0);
        }
        handle.update_caption_state_cache();
        Ok(copy_len as i32)
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn engine_take_next_log_line(
    handle: *mut EngineHandle,
    out_buf: *mut c_char,
    max_len: i32,
) -> i32 {
    with_handle(handle, |handle| {
        if out_buf.is_null() {
            return Err("out_buf pointer is null".to_string());
        }
        if max_len <= 0 {
            return Err("max_len must be positive".to_string());
        }

        let maybe_line = handle
            .session
            .lock()
            .expect("session poisoned")
            .take_next_log();
        let Some(line) = maybe_line else {
            unsafe { ptr::write(out_buf, 0) };
            return Ok(0);
        };

        let bytes = line.as_bytes();
        let writable = (max_len as usize).saturating_sub(1);
        let copy_len = writable.min(bytes.len());
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), out_buf, copy_len);
            ptr::write(out_buf.add(copy_len), 0);
        }
        Ok(copy_len as i32)
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn engine_get_caption_state_json(handle: *mut EngineHandle) -> *const c_char {
    if handle.is_null() {
        return ptr::null();
    }
    let handle = unsafe { &*handle };
    handle.update_caption_state_cache();
    handle
        .caption_state_json
        .lock()
        .expect("caption_state_json poisoned")
        .as_ptr()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn push_translated_pcm_rejects_null_samples() {
        let config = CString::new("{}").unwrap();
        let handle = engine_create(config.as_ptr());
        assert!(!handle.is_null());
        assert_eq!(engine_start(handle), 0);
        assert_eq!(
            engine_push_translated_pcm(handle, std::ptr::null(), 10, 1, 24_000, 0),
            -1
        );
        engine_destroy(handle);
    }

    #[test]
    fn push_translated_pcm_rejects_null_handle() {
        let samples = vec![0.0f32; 10];
        assert_eq!(
            engine_push_translated_pcm(std::ptr::null_mut(), samples.as_ptr(), 10, 1, 24_000, 0,),
            -1
        );
    }

    #[test]
    fn push_translated_pcm_writes_to_output_ring() {
        let config = CString::new("{}").unwrap();
        let handle = engine_create(config.as_ptr());
        assert_eq!(engine_start(handle), 0);

        let samples = vec![0.1f32; 240]; // 10ms at 24kHz
        let result = engine_push_translated_pcm(handle, samples.as_ptr(), 240, 1, 24_000, 0);
        assert_eq!(result, 0, "expected success");
        engine_destroy(handle);
    }
}
