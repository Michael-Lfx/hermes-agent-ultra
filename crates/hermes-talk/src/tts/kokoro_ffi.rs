//! C FFI to libkokoro (RK3588 NPU TTS). Enabled when build.rs sets `kokoro_rknn_ffi`.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::path::Path;

use crate::config::KokoroRknnTtsConfig;
use crate::error::{DemoError, Result};

#[repr(C)]
#[cfg_attr(not(kokoro_rknn_ffi), allow(dead_code))]
struct KokoroEngineConfig {
    vocab_json: *const c_char,
    encoder_onnx: *const c_char,
    har_onnx: *const c_char,
    decoder_path: *const c_char,
    voices_dir: *const c_char,
    espeak_data: *const c_char,
    lexicon_dir: *const c_char,
    t_fix: c_int,
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
                "Kokoro RKNN FFI not linked (build with KOKORO_PREBUILT_LIB or KOKORO_BUILD=1)"
                    .into(),
            ))
        }
        #[cfg(kokoro_rknn_ffi)]
        {
            let vocab = cstr(&cfg.vocab)?;
            let encoder = cstr(&cfg.encoder)?;
            let har = cstr(&cfg.har_gen)?;
            let decoder = cstr(&cfg.decoder)?;
            let voices = cstr(&cfg.voices_dir)?;
            let espeak = cstr(&cfg.espeak_data)?;
            let lexicon = cstr(&cfg.lexicon_dir)?;
            let c_cfg = KokoroEngineConfig {
                vocab_json: vocab.as_ptr(),
                encoder_onnx: encoder.as_ptr(),
                har_onnx: har.as_ptr(),
                decoder_path: decoder.as_ptr(),
                voices_dir: voices.as_ptr(),
                espeak_data: espeak.as_ptr(),
                lexicon_dir: lexicon.as_ptr(),
                t_fix: cfg.t_fix,
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
        british: bool,
        on_pcm: impl FnMut(&[i16]),
    ) -> Result<()> {
        #[cfg(not(kokoro_rknn_ffi))]
        {
            let _ = (text, voice, speed, british, on_pcm);
            Err(DemoError::Tts("Kokoro RKNN FFI not linked".into()))
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
                    i32::from(british),
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
    for path in [&cfg.encoder, &cfg.har_gen, &cfg.decoder, &cfg.vocab] {
        if !Path::new(path).exists() {
            return Err(DemoError::Tts(format!("missing kokoro RKNN model: {path}")));
        }
    }
    if !Path::new(&cfg.voices_dir).is_dir() {
        return Err(DemoError::Tts(format!(
            "missing kokoro voices_dir: {}",
            cfg.voices_dir
        )));
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
