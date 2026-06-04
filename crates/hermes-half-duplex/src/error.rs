use thiserror::Error;

#[derive(Debug, Error)]
pub enum HalfDuplexError {
    #[error("config: {0}")]
    Config(String),
    #[error("audio: {0}")]
    Audio(String),
    #[error("asr: {0}")]
    Asr(String),
    #[error("tts: {0}")]
    Tts(String),
    #[error("llm: {0}")]
    Llm(String),
    #[error("websocket: {0}")]
    WebSocket(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, HalfDuplexError>;
