use crate::TaskMessage;
use async_trait::async_trait;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

pub type EngineResult<T> = Result<T, EngineError>;

#[derive(Debug)]
pub enum EngineError {
    Http(reqwest::Error),
    UnexpectedStatus(reqwest::StatusCode, String),
    Io(std::io::Error),
    Voicevox(String),
    InvalidTask(String),
    TaskJoin(tokio::task::JoinError),
    Zip(zip::result::ZipError),
}

impl Display for EngineError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Http(err) => write!(f, "http error: {}", err),
            EngineError::UnexpectedStatus(status, body) => {
                write!(f, "unexpected status {}: {}", status, body)
            }
            EngineError::Io(err) => write!(f, "io error: {}", err),
            EngineError::Voicevox(err) => write!(f, "voicevox error: {}", err),
            EngineError::InvalidTask(err) => write!(f, "invalid task: {}", err),
            EngineError::TaskJoin(err) => write!(f, "task join error: {}", err),
            EngineError::Zip(err) => write!(f, "zip error: {}", err),
        }
    }
}

impl Error for EngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            EngineError::Http(err) => Some(err),
            EngineError::UnexpectedStatus(_, _) => None,
            EngineError::Io(err) => Some(err),
            EngineError::Voicevox(_) => None,
            EngineError::InvalidTask(_) => None,
            EngineError::TaskJoin(err) => Some(err),
            EngineError::Zip(err) => Some(err),
        }
    }
}

impl From<reqwest::Error> for EngineError {
    fn from(err: reqwest::Error) -> Self {
        EngineError::Http(err)
    }
}

impl From<std::io::Error> for EngineError {
    fn from(err: std::io::Error) -> Self {
        EngineError::Io(err)
    }
}

impl From<voicevox_core::Error> for EngineError {
    fn from(err: voicevox_core::Error) -> Self {
        EngineError::Voicevox(err.to_string())
    }
}

impl From<tokio::task::JoinError> for EngineError {
    fn from(err: tokio::task::JoinError) -> Self {
        EngineError::TaskJoin(err)
    }
}

impl From<zip::result::ZipError> for EngineError {
    fn from(err: zip::result::ZipError) -> Self {
        EngineError::Zip(err)
    }
}

#[async_trait]
pub trait TtsEngine: Send + Sync {
    async fn process_task(
        &self,
        engine_id: u32,
        message: &TaskMessage,
    ) -> EngineResult<Option<String>>;
}
