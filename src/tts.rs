use async_trait::async_trait;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

pub type EngineResult<T> = Result<T, EngineError>;

#[derive(Debug)]
pub enum EngineError {
    Http(reqwest::Error),
    UnexpectedStatus(reqwest::StatusCode, String),
}

impl Display for EngineError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Http(err) => write!(f, "http error: {}", err),
            EngineError::UnexpectedStatus(status, body) => {
                write!(f, "unexpected status {}: {}", status, body)
            }
        }
    }
}

impl Error for EngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            EngineError::Http(err) => Some(err),
            EngineError::UnexpectedStatus(_, _) => None,
        }
    }
}

impl From<reqwest::Error> for EngineError {
    fn from(err: reqwest::Error) -> Self {
        EngineError::Http(err)
    }
}

#[async_trait]
pub trait TtsEngine: Send + Sync {
    async fn set_speaker(&self, eval_id: &str, engine_id: u32, speaker_id: u32)
        -> EngineResult<()>;
    async fn synthesize(
        &self,
        eval_id: &str,
        engine_id: u32,
        speaker_id: u32,
        task_id: &str,
    ) -> EngineResult<()>;
}
