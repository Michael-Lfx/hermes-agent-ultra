use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::SampleFormat;
use tracing::{error, info};

use crate::audio::devices::{audio_host, resolve_output_device};
use crate::audio::fault::AudioFault;
use crate::config::AudioConfig;
use crate::error::{HalfDuplexError, Result};

pub struct AudioPlayback {
    queue: Arc<Mutex<PlaybackState>>,
    generation: Arc<AtomicU64>,
    source_rate: u32,
    _thread: JoinHandle<()>,
}

struct PlaybackState {
    buffer: VecDeque<f32>,
    /// Fractional read position into `buffer` (in source-rate samples).
    playhead: f64,
    active_generation: u64,
    stopped: bool,
    source_rate: u32,
    device_rate: u32,
}

impl AudioPlayback {
    pub fn start(
        audio_cfg: &AudioConfig,
        source_rate: u32,
        audio_fault: Arc<AudioFault>,
    ) -> Result<Self> {
        let host = audio_host();
        let device = resolve_output_device(&host, audio_cfg)?;
        let config = device
            .default_output_config()
            .or_else(|_| {
                device
                    .supported_output_configs()
                    .map_err(|e| HalfDuplexError::Audio(e.to_string()))?
                    .max_by(|a, b| {
                        a.max_sample_rate()
                            .0
                            .cmp(&b.max_sample_rate().0)
                            .then(a.channels().cmp(&b.channels()))
                    })
                    .map(|r| r.with_max_sample_rate())
                    .ok_or_else(|| {
                        HalfDuplexError::Audio("no supported output config for device".into())
                    })
            })
            .map_err(|e: HalfDuplexError| e)?;
        let device_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        info!(
            device = %device.name().unwrap_or_default(),
            device_rate,
            source_rate,
            "audio playback (resampling source -> device)"
        );

        if device_rate != source_rate {
            info!(
                "TTS PCM is {source_rate} Hz; output device is {device_rate} Hz; resampling in playback"
            );
        }

        let queue = Arc::new(Mutex::new(PlaybackState {
            buffer: VecDeque::new(),
            playhead: 0.0,
            active_generation: 0,
            stopped: false,
            source_rate,
            device_rate,
        }));
        let generation = Arc::new(AtomicU64::new(0));
        let q = queue.clone();

        let stream_config: cpal::StreamConfig = config.clone().into();
        let thread = thread::spawn(move || {
            let fault_cb = audio_fault.clone();
            let err_fn = move |e: cpal::StreamError| {
                fault_cb.report_playback(e);
            };
            let stream = match config.sample_format() {
                SampleFormat::F32 => device.build_output_stream(
                    &stream_config,
                    move |out: &mut [f32], _| {
                        fill_output(out, channels, &q);
                    },
                    err_fn,
                    None,
                ),
                SampleFormat::I16 => device.build_output_stream(
                    &stream_config,
                    move |out: &mut [i16], _| {
                        let mut tmp = vec![0.0f32; out.len()];
                        fill_output(&mut tmp, channels, &q);
                        for (o, s) in out.iter_mut().zip(tmp.iter()) {
                            *o = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                        }
                    },
                    err_fn,
                    None,
                ),
                SampleFormat::U16 => device.build_output_stream(
                    &stream_config,
                    move |out: &mut [u16], _| {
                        let mut tmp = vec![0.0f32; out.len()];
                        fill_output(&mut tmp, channels, &q);
                        for (o, s) in out.iter_mut().zip(tmp.iter()) {
                            *o = ((s.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16;
                        }
                    },
                    err_fn,
                    None,
                ),
                other => {
                    error!(format = ?other, "unsupported playback sample format");
                    return;
                }
            };
            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    error!(error = %e, "build playback stream failed");
                    return;
                }
            };
            if let Err(e) = stream.play() {
                error!(error = %e, "start playback stream failed");
            }
            loop {
                thread::sleep(std::time::Duration::from_secs(1));
            }
        });

        Ok(Self {
            queue,
            generation,
            source_rate,
            _thread: thread,
        })
    }

    pub fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn bump_generation(&self) -> u64 {
        let g = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let mut st = self.queue.lock().unwrap();
        st.active_generation = g;
        st.stopped = false;
        st.playhead = 0.0;
        g
    }

    pub fn enqueue_pcm_i16(&self, generation: u64, pcm: &[u8]) {
        let samples = crate::audio::pcm::i16_le_to_f32(pcm);
        self.enqueue_f32(generation, &samples);
    }

    pub fn enqueue_f32(&self, generation: u64, samples: &[f32]) {
        let mut st = self.queue.lock().unwrap();
        if st.stopped || generation < st.active_generation {
            return;
        }
        st.buffer.extend(samples);
    }

    pub fn stop_clear(&self) -> u64 {
        let g = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let mut st = self.queue.lock().unwrap();
        st.buffer.clear();
        st.playhead = 0.0;
        st.active_generation = g;
        st.stopped = true;
        g
    }

    /// Start a fresh playout generation (after barge-in or before a new TTS turn).
    pub fn begin_playout(&self, play_gen: &AtomicU64) -> u64 {
        let g = self.bump_generation();
        self.resume_playback();
        play_gen.store(g, Ordering::SeqCst);
        g
    }

    /// Halt playout immediately and discard queued PCM.
    pub fn halt_playout(&self, play_gen: &AtomicU64) -> u64 {
        let g = self.stop_clear();
        play_gen.store(g, Ordering::SeqCst);
        g
    }

    pub fn resume_playback(&self) {
        let mut st = self.queue.lock().unwrap();
        st.stopped = false;
    }

    pub fn is_stopped(&self) -> bool {
        self.queue.lock().unwrap().stopped
    }

    pub fn sample_rate(&self) -> u32 {
        self.source_rate
    }

    pub fn buffered_samples(&self) -> usize {
        self.queue.lock().unwrap().buffer.len()
    }

    /// Wait until the play queue is nearly empty or timeout.
    pub async fn wait_drain(&self, timeout: std::time::Duration) {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if self.buffered_samples() < self.source_rate as usize / 50 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}

/// Resample from `source_rate` buffer to `device_rate` output using linear interpolation.
fn fill_output(out: &mut [f32], channels: usize, queue: &Arc<Mutex<PlaybackState>>) {
    let frames = out.len() / channels;
    let mut st = queue.lock().unwrap();

    if st.stopped {
        for s in out.iter_mut() {
            *s = 0.0;
        }
        return;
    }

    // Source samples consumed per one device output frame.
    let step = st.source_rate as f64 / st.device_rate as f64;

    let buf = &st.buffer;
    let mut pos = st.playhead;

    for frame in 0..frames {
        let idx = pos as usize;
        let frac = (pos - idx as f64) as f32;
        let s0 = buf.get(idx).copied().unwrap_or(0.0);
        let s1 = buf.get(idx + 1).copied().unwrap_or(0.0);
        let sample = s0 + (s1 - s0) * frac;

        for ch in 0..channels {
            out[frame * channels + ch] = sample;
        }
        pos += step;
    }

    st.playhead = pos;

    // Drop fully consumed source samples from the front of the queue.
    let drain = st.playhead as usize;
    if drain > 0 {
        for _ in 0..drain {
            st.buffer.pop_front();
        }
        st.playhead -= drain as f64;
    }
}
