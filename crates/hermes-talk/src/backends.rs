//! ASR/TTS backend name normalization (all platforms).

/// Cloud DashScope / 百炼 ASR|TTS.
pub const CLOUD_BACKEND_ALIASES: &[&str] = &["bailian", "cloud", "dashscope", "aliyun"];

/// sherpa-onnx local SenseVoice / Kokoro.
pub const SHERPA_BACKEND_ALIASES: &[&str] = &["sherpa", "sensevoice", "kokoro"];

/// Board local alias (`local` / `rockchip` → sherpa SenseVoice RKNN on RK3588, CPU SenseVoice elsewhere).
pub const LOCAL_BACKEND_ALIASES: &[&str] = &["local", "rockchip"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TalkBackendKind {
    Cloud,
    Sherpa,
    LocalHardware,
}

fn matches_alias(raw: &str, aliases: &[&str]) -> bool {
    let normalized = raw.trim().to_ascii_lowercase();
    aliases.iter().any(|a| *a == normalized)
}

pub fn classify_talk_backend(raw: &str) -> TalkBackendKind {
    if matches_alias(raw, CLOUD_BACKEND_ALIASES) {
        TalkBackendKind::Cloud
    } else if matches_alias(raw, SHERPA_BACKEND_ALIASES) {
        TalkBackendKind::Sherpa
    } else if matches_alias(raw, LOCAL_BACKEND_ALIASES) {
        TalkBackendKind::LocalHardware
    } else {
        TalkBackendKind::Cloud
    }
}

pub fn uses_cloud_asr(backend: &str) -> bool {
    classify_talk_backend(backend) == TalkBackendKind::Cloud
}

pub fn uses_cloud_tts(backend: &str) -> bool {
    classify_talk_backend(backend) == TalkBackendKind::Cloud
}

pub fn uses_sherpa_asr(backend: &str) -> bool {
    match classify_talk_backend(backend) {
        TalkBackendKind::Sherpa => true,
        TalkBackendKind::LocalHardware => cfg!(feature = "sherpa-asr-tts"),
        TalkBackendKind::Cloud => false,
    }
}

pub fn uses_sherpa_tts(backend: &str) -> bool {
    match classify_talk_backend(backend) {
        TalkBackendKind::Sherpa => true,
        TalkBackendKind::LocalHardware => cfg!(feature = "sherpa-asr-tts"),
        TalkBackendKind::Cloud => false,
    }
}
