use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{HalfDuplexError, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub dashscope: DashscopeConfig,
    #[serde(default)]
    pub asr: AsrConfig,
    #[serde(default)]
    pub tts: TtsConfig,
    #[serde(default)]
    pub orchestrator: OrchestratorConfig,
    #[serde(default)]
    pub busy_replies: BusyRepliesConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub wake: WakeConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DashscopeConfig {
    pub api_key: String,
    #[serde(default = "default_ws_url")]
    pub ws_url: String,
}

fn default_ws_url() -> String {
    "wss://dashscope.aliyuncs.com/api-ws/v1/inference".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct AsrConfig {
    #[serde(default = "default_asr_model")]
    pub model: String,
    #[serde(default = "default_16k")]
    pub sample_rate: u32,
    #[serde(default = "default_chunk_ms")]
    pub chunk_ms: u32,
    #[serde(default = "default_pcm")]
    pub format: String,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            model: default_asr_model(),
            sample_rate: default_16k(),
            chunk_ms: default_chunk_ms(),
            format: default_pcm(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TtsConfig {
    #[serde(default = "default_tts_model")]
    pub model: String,
    #[serde(default = "default_voice")]
    pub voice: String,
    #[serde(default = "default_24k")]
    pub sample_rate: u32,
    #[serde(default = "default_pcm")]
    pub format: String,
    /// Wait for DashScope `task-started` (raise on slow boards / networks).
    #[serde(default = "default_tts_task_started_timeout_sec")]
    pub task_started_timeout_sec: u64,
    #[serde(default = "default_tts_finish_timeout_sec")]
    pub finish_timeout_sec: u64,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            model: default_tts_model(),
            voice: default_voice(),
            sample_rate: default_24k(),
            format: default_pcm(),
            task_started_timeout_sec: default_tts_task_started_timeout_sec(),
            finish_timeout_sec: default_tts_finish_timeout_sec(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BusyRepliesConfig {
    #[serde(default = "default_busy_enabled")]
    pub enabled: bool,
    #[serde(default = "default_busy_cooldown")]
    pub cooldown_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrchestratorConfig {
    /// Legacy; used only if `endpoint_silence_ms` is not set in old configs.
    #[serde(default = "default_min_silence")]
    pub min_silence_ms: u32,
    #[serde(default = "default_endpoint_silence")]
    pub endpoint_silence_ms: u32,
    #[serde(default = "default_trigger_on_asr_final")]
    pub trigger_on_asr_final: bool,
    #[serde(default = "default_cold_start")]
    pub cold_start_sec: u64,
    #[serde(default = "default_min_final")]
    pub min_final_chars: usize,
    #[serde(default = "default_sentence_len")]
    pub sentence_min_len: usize,
    #[serde(default = "default_tts_first_chunk")]
    pub tts_first_chunk_chars: usize,
    #[serde(default = "default_barge_frames")]
    pub barge_in_frames: u32,
    #[serde(default)]
    pub speculative_llm: bool,
    #[serde(default = "default_speculative_stable")]
    pub speculative_stable_ms: u32,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            min_silence_ms: default_min_silence(),
            endpoint_silence_ms: default_endpoint_silence(),
            trigger_on_asr_final: default_trigger_on_asr_final(),
            cold_start_sec: default_cold_start(),
            min_final_chars: default_min_final(),
            sentence_min_len: default_sentence_len(),
            tts_first_chunk_chars: default_tts_first_chunk(),
            barge_in_frames: default_barge_frames(),
            speculative_llm: false,
            speculative_stable_ms: default_speculative_stable(),
        }
    }
}

impl OrchestratorConfig {
    pub fn endpoint_silence_ms(&self) -> u32 {
        self.endpoint_silence_ms
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WakeConfig {
    #[serde(default = "default_wake_enabled")]
    pub enabled: bool,
    /// Spoken immediately after wake word is detected (empty to disable).
    #[serde(default = "default_wake_ack_reply")]
    pub ack_reply: String,
    /// Wake phrases; encoded at startup via sherpa-onnx text2token.
    #[serde(default)]
    pub phrases: Vec<String>,
    /// Deprecated: use `phrases = ["…"]`; merged in [`WakeConfig::normalize`].
    #[serde(default)]
    pub phrase: Option<String>,
    #[serde(default)]
    pub model_dir: String,
    #[serde(default)]
    pub encoder: String,
    #[serde(default)]
    pub decoder: String,
    #[serde(default)]
    pub joiner: String,
    #[serde(default)]
    pub tokens: String,
    /// Modeling units for text2token (`phone+ppinyin` for zh-en KWS model).
    #[serde(default = "default_wake_tokens_type")]
    pub tokens_type: String,
    #[serde(default)]
    pub bpe_model: String,
    #[serde(default)]
    pub lexicon: String,
    #[serde(default = "default_wake_boost")]
    pub boost_score: f32,
    #[serde(default = "default_wake_threshold")]
    pub trigger_threshold: f32,
    #[serde(default = "default_grace_after_wake")]
    pub grace_after_wake_sec: u64,
    #[serde(default = "default_idle_after_turn")]
    pub idle_after_turn_sec: u64,
    #[serde(default = "default_kws_threads")]
    pub num_threads: i32,
}

impl Default for WakeConfig {
    fn default() -> Self {
        Self {
            enabled: default_wake_enabled(),
            ack_reply: default_wake_ack_reply(),
            phrases: vec![default_wake_phrase()],
            phrase: None,
            model_dir: "models/kws-zh-en".to_string(),
            encoder: String::new(),
            decoder: String::new(),
            joiner: String::new(),
            tokens: String::new(),
            tokens_type: default_wake_tokens_type(),
            bpe_model: String::new(),
            lexicon: String::new(),
            boost_score: default_wake_boost(),
            trigger_threshold: default_wake_threshold(),
            grace_after_wake_sec: default_grace_after_wake(),
            idle_after_turn_sec: default_idle_after_turn(),
            num_threads: default_kws_threads(),
        }
    }
}

impl WakeConfig {
    pub fn normalize(&mut self) {
        if self.phrases.is_empty() {
            if let Some(p) = self.phrase.take() {
                if !p.trim().is_empty() {
                    self.phrases.push(p);
                }
            }
        }
    }

    /// Resolve `model_dir` and default onnx filenames relative to `half_duplex.toml`.
    pub fn normalize_paths(&mut self, config_dir: &Path) {
        self.normalize();
        if !self.model_dir.is_empty() {
            self.model_dir = absolutize_config_path(config_dir, &self.model_dir);
        }
        self.resolve_paths();
        for path in [&mut self.encoder, &mut self.decoder, &mut self.joiner, &mut self.tokens] {
            if !path.is_empty() {
                *path = absolutize_config_path(config_dir, path);
            }
        }
        if !self.lexicon.is_empty() {
            self.lexicon = absolutize_config_path(config_dir, &self.lexicon);
        }
        if !self.bpe_model.is_empty() {
            self.bpe_model = absolutize_config_path(config_dir, &self.bpe_model);
        }
    }

    pub fn effective_phrases(&self) -> Vec<String> {
        self.phrases
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub fn resolve_paths(&mut self) {
        if self.model_dir.is_empty() {
            return;
        }
        let dir = self.model_dir.trim_end_matches(['/', '\\']);
        if self.encoder.is_empty() {
            self.encoder = format!("{dir}/encoder.onnx");
        }
        if self.decoder.is_empty() {
            self.decoder = format!("{dir}/decoder.onnx");
        }
        if self.joiner.is_empty() {
            self.joiner = format!("{dir}/joiner.onnx");
        }
        if self.tokens.is_empty() {
            self.tokens = format!("{dir}/tokens.txt");
        }
        if self.lexicon.is_empty() && self.tokens_type == "phone+ppinyin" {
            self.lexicon = format!("{dir}/en.phone");
        }
        if self.bpe_model.is_empty()
            && (self.tokens_type == "bpe" || self.tokens_type == "cjkchar+bpe")
        {
            self.bpe_model = format!("{dir}/bpe.model");
        }
    }

    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.effective_phrases().is_empty() {
            return Err(HalfDuplexError::Config(
                "wake.phrases is empty; add at least one phrase".into(),
            ));
        }
        if self.grace_after_wake_sec == 0 {
            return Err(HalfDuplexError::Config(
                "wake.grace_after_wake_sec must be >= 1".into(),
            ));
        }
        if self.idle_after_turn_sec == 0 {
            return Err(HalfDuplexError::Config(
                "wake.idle_after_turn_sec must be >= 1".into(),
            ));
        }
        for (name, path) in [
            ("encoder", &self.encoder),
            ("decoder", &self.decoder),
            ("joiner", &self.joiner),
            ("tokens", &self.tokens),
        ] {
            if path.is_empty() {
                return Err(HalfDuplexError::Config(format!(
                    "wake.{name} is empty; set wake.model_dir or explicit paths"
                )));
            }
            if !Path::new(path).exists() {
                let native = path.replace('/', std::path::MAIN_SEPARATOR_STR);
                if Path::new(&native).exists() {
                    continue;
                }
                return Err(HalfDuplexError::Config(format!(
                    "wake.{name} not found: {path} (download sherpa-onnx KWS into wake.model_dir; see crates/hermes-half-duplex/half_duplex.example.toml)"
                )));
            }
        }
        if (self.tokens_type == "bpe" || self.tokens_type == "cjkchar+bpe")
            && !self.bpe_model.is_empty()
            && !std::path::Path::new(&self.bpe_model).exists()
        {
            return Err(HalfDuplexError::Config(format!(
                "wake.bpe_model not found: {}",
                self.bpe_model
            )));
        }
        if self.tokens_type == "phone+ppinyin"
            && !self.lexicon.is_empty()
            && !std::path::Path::new(&self.lexicon).exists()
        {
            return Err(HalfDuplexError::Config(format!(
                "wake.lexicon not found: {}",
                self.lexicon
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AudioConfig {
    #[serde(default)]
    pub input_device: String,
    #[serde(default)]
    pub output_device: String,
}

fn absolutize_config_path(base: &Path, raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return trimmed.to_string();
    }
    base.join(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn default_asr_model() -> String {
    "fun-asr-realtime".to_string()
}
fn default_tts_model() -> String {
    "cosyvoice-v3-flash".to_string()
}
fn default_voice() -> String {
    "longanyang".to_string()
}
fn default_16k() -> u32 {
    16000
}
fn default_24k() -> u32 {
    24000
}
fn default_chunk_ms() -> u32 {
    100
}
fn default_pcm() -> String {
    "pcm".to_string()
}
fn default_min_silence() -> u32 {
    450
}
fn default_endpoint_silence() -> u32 {
    150
}
fn default_trigger_on_asr_final() -> bool {
    true
}
fn default_cold_start() -> u64 {
    3
}
fn default_min_final() -> usize {
    2
}
fn default_sentence_len() -> usize {
    12
}
fn default_tts_first_chunk() -> usize {
    6
}
fn default_barge_frames() -> u32 {
    2
}
fn default_speculative_stable() -> u32 {
    300
}
fn default_busy_enabled() -> bool {
    false
}
fn default_busy_cooldown() -> u64 {
    10
}
fn default_wake_enabled() -> bool {
    false
}
fn default_wake_ack_reply() -> String {
    "哎，我在！".to_string()
}
fn default_wake_phrase() -> String {
    "小智小智".to_string()
}
fn default_wake_tokens_type() -> String {
    "phone+ppinyin".to_string()
}
fn default_wake_boost() -> f32 {
    2.0
}
fn default_wake_threshold() -> f32 {
    0.35
}
fn default_grace_after_wake() -> u64 {
    5
}
fn default_idle_after_turn() -> u64 {
    30
}
fn default_kws_threads() -> i32 {
    1
}
fn default_tts_task_started_timeout_sec() -> u64 {
    45
}
fn default_tts_finish_timeout_sec() -> u64 {
    60
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path)
            .map_err(|e| HalfDuplexError::Config(format!("read {}: {e}", path.display())))?;
        let mut cfg: Config =
            toml::from_str(&raw).map_err(|e| HalfDuplexError::Config(format!("parse toml: {e}")))?;
        if let Ok(key) = std::env::var("DASHSCOPE_API_KEY") {
            if !key.trim().is_empty() {
                cfg.dashscope.api_key = key.trim().to_string();
            }
        }
        let config_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        cfg.wake.normalize_paths(&config_dir);
        cfg.wake.validate()?;
        if cfg.dashscope.api_key.trim().is_empty() {
            return Err(HalfDuplexError::Config(
                "dashscope.api_key is empty; set in half_duplex.toml or DASHSCOPE_API_KEY".into(),
            ));
        }
        Ok(cfg)
    }
}
