use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::slice;
use std::sync::Mutex;

use common::{EngineConfig, EngineMode};
use session_core::EngineSession;

pub struct EngineHandle {
    session: Mutex<EngineSession>,
    last_error: Mutex<CString>,
    metrics_json: Mutex<CString>,
    shared_output_path: Mutex<CString>,
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
        }
    }

    fn set_last_error(&self, message: &str) {
        *self.last_error.lock().expect("last_error poisoned") = cstring_clean(message);
    }

    fn update_metrics_cache(&self) {
        let metrics = self.session.lock().expect("session poisoned").metrics_json();
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
}

fn cstring_clean(input: &str) -> CString {
    let cleaned = input.replace('\0', " ");
    CString::new(cleaned).expect("sanitized CString")
}

fn read_optional_cstr(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
}

fn with_handle<T>(handle: *mut EngineHandle, f: impl FnOnce(&EngineHandle) -> Result<T, String>) -> Result<T, i32> {
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
        handle.session.lock().expect("session poisoned").start();
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
pub extern "C" fn engine_set_target_language(handle: *mut EngineHandle, lang: *const c_char) -> i32 {
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
        handle.session.lock().expect("session poisoned").set_mode(mode);
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
            .enable_shared_output(capacity_frames as usize, channels as u16, sample_rate as u32)
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
            .push_input_pcm(slice, frame_count as usize, channels as u16, sample_rate as u32, timestamp_ns)
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
    handle.last_error.lock().expect("last_error poisoned").as_ptr()
}

#[no_mangle]
pub extern "C" fn engine_get_metrics_json(handle: *mut EngineHandle) -> *const c_char {
    if handle.is_null() {
        return ptr::null();
    }
    let handle = unsafe { &*handle };
    handle.update_metrics_cache();
    handle.metrics_json.lock().expect("metrics poisoned").as_ptr()
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
