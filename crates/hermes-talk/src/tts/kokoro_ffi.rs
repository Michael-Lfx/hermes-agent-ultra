//! C FFI to Kokoro hybrid-v1 RKNN TTS (ORT prefix/tail + RKNN front). Enabled when build.rs sets `kokoro_rknn_ffi`.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::path::Path;

use crate::config::KokoroRknnTtsConfig;
use crate::error::{DemoError, Result};

#[repr(C)]
#[cfg_attr(not(kokoro_rknn_ffi), allow(dead_code))]
struct KokoroEngineConfig {
    model_dir: *const c_char,
    prefix_onnx: *const c_char,
    front_rknn: *const c_char,
    tail_onnx: *const c_char,
    vocoder_front_rknn: *const c_char,
    tail_rest_onnx: *const c_char,
    tokens: *const c_char,
    style_npy: *const c_char,
    voice: *const c_char,
    seq_len: c_int,
}

#[cfg_attr(not(kokoro_rknn_ffi), allow(dead_code))]
type KokoroPcmCallback = unsafe extern "C" fn(*const i16, usize, *mut c_void);

#[cfg(kokoro_rknn_ffi)]
mod ffi {
    use super::*;

    extern "C" {
        pub fn kokoro_engine_create(
            cfg: *const KokoroEngineConfig,
            err_buf: *mut c_char,
            err_len: usize,
        ) -> *mut c_void;
        pub fn kokoro_engine_destroy(engine: *mut c_void);
        pub fn kokoro_engine_synthesize_text(
            engine: *mut c_void,
            text: *const c_char,
            voice: *const c_char,
            speed: f32,
            british: c_int,
            callback: KokoroPcmCallback,
            user_data: *mut c_void,
            err_buf: *mut c_char,
            err_len: usize,
        ) -> c_int;
    }
}

pub struct KokoroEngineHandle {
    #[cfg(kokoro_rknn_ffi)]
    ptr: *mut c_void,
}

impl KokoroEngineHandle {
    pub fn create(cfg: &KokoroRknnTtsConfig) -> Result<Self> {
        validate_model_paths(cfg)?;
        #[cfg(not(kokoro_rknn_ffi))]
        {
            let _ = cfg;
            Err(DemoError::Tts(
                "Kokoro hybrid RKNN FFI not linked (build with KOKORO_PREBUILT_LIB or KOKORO_BUILD=1)"
                    .into(),
            ))
        }
        #[cfg(kokoro_rknn_ffi)]
        {
            let model_dir = cstr(&cfg.model_dir)?;
            let prefix = cstr(&cfg.prefix_onnx)?;
            let front = cstr(&cfg.front_rknn)?;
            let tail = cstr(&cfg.tail_onnx)?;
            let vocoder_front = cstr(&cfg.vocoder_front_rknn)?;
            let tail_rest = cstr(&cfg.tail_rest_onnx)?;
            let tokens = cstr(&cfg.tokens)?;
            let style = cstr(&cfg.style_npy)?;
            let voice = cstr(&cfg.voice)?;
            let c_cfg = KokoroEngineConfig {
                model_dir: model_dir.as_ptr(),
                prefix_onnx: prefix.as_ptr(),
                front_rknn: front.as_ptr(),
                tail_onnx: tail.as_ptr(),
                vocoder_front_rknn: vocoder_front.as_ptr(),
                tail_rest_onnx: tail_rest.as_ptr(),
                tokens: tokens.as_ptr(),
                style_npy: style.as_ptr(),
                voice: voice.as_ptr(),
                seq_len: cfg.seq_len,
            };
            let mut err = vec![0i8; 512];
            let ptr = unsafe { ffi::kokoro_engine_create(&c_cfg, err.as_mut_ptr(), err.len()) };
            if ptr.is_null() {
                return Err(DemoError::Tts(format!(
                    "kokoro_engine_create failed: {}",
                    cstr_to_string(err.as_ptr())
                )));
            }
            Ok(Self { ptr })
        }
    }

    pub fn synthesize_text(
        &self,
        text: &str,
        voice: &str,
        speed: f32,
        on_pcm: impl FnMut(&[i16]),
    ) -> Result<()> {
        #[cfg(not(kokoro_rknn_ffi))]
        {
            let _ = (text, voice, speed, on_pcm);
            Err(DemoError::Tts("Kokoro hybrid RKNN FFI not linked".into()))
        }
        #[cfg(kokoro_rknn_ffi)]
        {
            let c_text = CString::new(text).map_err(|e| DemoError::Tts(e.to_string()))?;
            let c_voice = CString::new(voice).map_err(|e| DemoError::Tts(e.to_string()))?;
            let mut err = vec![0i8; 512];
            struct Ctx<'a> {
                cb: &'a mut dyn FnMut(&[i16]),
            }
            extern "C" fn trampoline(samples: *const i16, count: usize, user: *mut c_void) {
                if samples.is_null() || user.is_null() || count == 0 {
                    return;
                }
                let ctx = unsafe { &mut *(user as *mut Ctx<'_>) };
                let slice = unsafe { std::slice::from_raw_parts(samples, count) };
                (ctx.cb)(slice);
            }
            let mut ctx = Ctx { cb: &mut on_pcm };
            let rc = unsafe {
                ffi::kokoro_engine_synthesize_text(
                    self.ptr,
                    c_text.as_ptr(),
                    c_voice.as_ptr(),
                    speed,
                    0,
                    trampoline,
                    &mut ctx as *mut Ctx<'_> as *mut c_void,
                    err.as_mut_ptr(),
                    err.len(),
                )
            };
            if rc != 0 {
                return Err(DemoError::Tts(format!(
                    "kokoro_engine_synthesize_text failed: {}",
                    cstr_to_string(err.as_ptr())
                )));
            }
            Ok(())
        }
    }
}

impl Drop for KokoroEngineHandle {
    fn drop(&mut self) {
        #[cfg(kokoro_rknn_ffi)]
        if !self.ptr.is_null() {
            unsafe { ffi::kokoro_engine_destroy(self.ptr) };
            self.ptr = std::ptr::null_mut();
        }
    }
}

pub fn validate_model_paths(cfg: &KokoroRknnTtsConfig) -> Result<()> {
    if !cfg.enabled {
        return Err(DemoError::Tts("kokoro RKNN disabled in config".into()));
    }
    for path in [
        &cfg.prefix_onnx,
        &cfg.front_rknn,
        &cfg.vocoder_front_rknn,
        &cfg.tail_rest_onnx,
        &cfg.tokens,
        &cfg.style_npy,
    ] {
        if !Path::new(path).exists() {
            return Err(DemoError::Tts(format!(
                "missing kokoro hybrid model: {path}"
            )));
        }
    }
    Ok(())
}

#[cfg_attr(not(kokoro_rknn_ffi), allow(dead_code))]
fn cstr(s: &str) -> Result<CString> {
    CString::new(s).map_err(|e| DemoError::Tts(e.to_string()))
}

#[cfg_attr(not(kokoro_rknn_ffi), allow(dead_code))]
fn cstr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}
