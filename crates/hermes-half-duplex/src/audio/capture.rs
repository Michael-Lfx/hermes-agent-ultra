use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use rubato::{Resampler, SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction};
use tracing::{error, info};

use crate::audio::pcm::f32_to_i16_le;
use crate::config::AudioConfig;
use crate::error::{HalfDuplexError, Result};

const TARGET_RATE: u32 = 16000;

pub struct AudioChunk {
    pub samples_f32: Vec<f32>,
    pub samples_i16_bytes: Vec<u8>,
}

pub struct AudioCapture {
    _thread: JoinHandle<()>,
    rx: Receiver<AudioChunk>,
}

impl AudioCapture {
    pub fn start(audio_cfg: &AudioConfig, chunk_ms: u32) -> Result<Self> {
        let host = cpal::default_host();
        let device = pick_input_device(&host, audio_cfg)?;
        let config = device
            .default_input_config()
            .map_err(|e| HalfDuplexError::Audio(e.to_string()))?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let chunk_samples_16k = (TARGET_RATE as u64 * chunk_ms as u64 / 1000) as usize;

        info!(
            device = %device.name().unwrap_or_default(),
            rate = sample_rate,
            channels,
            format = ?config.sample_format(),
            chunk_ms,
            "audio capture"
        );

        let (tx, rx) = mpsc::sync_channel::<AudioChunk>(64);
        let stream_config: cpal::StreamConfig = config.clone().into();

        let thread = thread::spawn(move || {
            let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
            let buf = buffer.clone();
            let tx = tx.clone();

            let ratio = TARGET_RATE as f64 / sample_rate as f64;
            let needs_resample = (sample_rate as f64 - TARGET_RATE as f64).abs() > 1.0;
            let mut resampler: Option<SincFixedIn<f32>> = if needs_resample {
                let params = SincInterpolationParameters {
                    sinc_len: 256,
                    f_cutoff: 0.95,
                    interpolation: SincInterpolationType::Linear,
                    oversampling_factor: 256,
                    window: WindowFunction::BlackmanHarris2,
                };
                match SincFixedIn::<f32>::new(ratio, 2.0, params, chunk_samples_16k * 2, 1) {
                    Ok(r) => Some(r),
                    Err(e) => {
                        error!(error = %e, native_rate = sample_rate, "resampler init failed");
                        return;
                    }
                }
            } else {
                None
            };

            let err_fn = |e| eprintln!("capture error: {e}");

            let stream = match config.sample_format() {
                SampleFormat::F32 => device.build_input_stream(
                    &stream_config,
                    move |data: &[f32], _| {
                        on_input(data, channels, &buf, &tx, chunk_samples_16k, &mut resampler);
                    },
                    err_fn,
                    None,
                ),
                SampleFormat::I16 => device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _| {
                        let f: Vec<f32> = data
                            .chunks(channels)
                            .map(|c| c[0] as f32 / i16::MAX as f32)
                            .collect();
                        on_input(&f, 1, &buf, &tx, chunk_samples_16k, &mut resampler);
                    },
                    err_fn,
                    None,
                ),
                SampleFormat::U16 => device.build_input_stream(
                    &stream_config,
                    move |data: &[u16], _| {
                        let f: Vec<f32> = data
                            .chunks(channels)
                            .map(|c| (c[0] as f32 - 32768.0) / 32768.0)
                            .collect();
                        on_input(&f, 1, &buf, &tx, chunk_samples_16k, &mut resampler);
                    },
                    err_fn,
                    None,
                ),
                other => {
                    eprintln!("unsupported format {other:?}");
                    return;
                }
            };

            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("build stream: {e}");
                    return;
                }
            };
            if let Err(e) = stream.play() {
                eprintln!("play stream: {e}");
                return;
            }
            loop {
                thread::sleep(std::time::Duration::from_secs(1));
            }
        });

        Ok(Self {
            _thread: thread,
            rx,
        })
    }

    pub fn try_recv_chunk(&self) -> Option<AudioChunk> {
        self.rx.try_recv().ok()
    }
}

fn pick_input_device(host: &cpal::Host, cfg: &AudioConfig) -> Result<cpal::Device> {
    if cfg.input_device.is_empty() {
        return host
            .default_input_device()
            .ok_or_else(|| HalfDuplexError::Audio("no default input device".into()));
    }
    let name = &cfg.input_device;
    host.input_devices()
        .map_err(|e| HalfDuplexError::Audio(e.to_string()))?
        .find(|d| d.name().map(|n| n == *name).unwrap_or(false))
        .ok_or_else(|| HalfDuplexError::Audio(format!("input device not found: {name}")))
}

fn on_input(
    mono: &[f32],
    _channels: usize,
    buffer: &Arc<Mutex<Vec<f32>>>,
    tx: &SyncSender<AudioChunk>,
    chunk_samples: usize,
    resampler: &mut Option<SincFixedIn<f32>>,
) {
    let mut samples = mono.to_vec();
    if let Some(r) = resampler {
        if samples.is_empty() {
            return;
        }
        match r.process(&[samples], None) {
            Ok(out) => samples = out[0].clone(),
            Err(_) => return,
        }
    }

    let mut buf = buffer.lock().unwrap();
    buf.extend_from_slice(&samples);
    while buf.len() >= chunk_samples {
        let chunk: Vec<f32> = buf.drain(..chunk_samples).collect();
        let bytes = f32_to_i16_le(&chunk);
        let _ = tx.try_send(AudioChunk {
            samples_f32: chunk,
            samples_i16_bytes: bytes,
        });
    }
}
