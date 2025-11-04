use crate::{
    tts::{EngineError, EngineResult, TtsEngine},
    TaskMessage,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;

#[derive(Clone)]
pub struct MockTtsEngine {
    client: Client,
    base_url: String,
}

impl MockTtsEngine {
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into();
        let normalized = base_url.trim_end_matches('/').to_string();
        Self {
            client: Client::new(),
            base_url: normalized,
        }
    }

    fn speaker_url(&self, eval_id: &str, engine_id: u32) -> String {
        format!(
            "{}/evaluations/{}/engines/{}/speaker",
            self.base_url, eval_id, engine_id
        )
    }

    fn synthesis_url(&self, eval_id: &str, engine_id: u32) -> String {
        format!(
            "{}/evaluations/{}/engines/{}/synthesis",
            self.base_url, eval_id, engine_id
        )
    }

    async fn ensure_success(response: reqwest::Response) -> EngineResult<()> {
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }

        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable>".into());
        Err(EngineError::UnexpectedStatus(status, body))
    }
}

#[derive(Serialize)]
struct SpeakerRequest {
    speaker_id: u32,
}

#[derive(Serialize)]
struct SynthesisRequest<'a> {
    speaker_id: u32,
    task_id: &'a str,
}

#[async_trait]
impl TtsEngine for MockTtsEngine {
    async fn process_task(
        &self,
        engine_id: u32,
        message: &TaskMessage,
    ) -> EngineResult<Option<String>> {
        let response = self
            .client
            .put(self.speaker_url(&message.eval_id, engine_id))
            .json(&SpeakerRequest {
                speaker_id: message.speaker_id,
            })
            .send()
            .await?;
        Self::ensure_success(response).await?;

        let response = self
            .client
            .post(self.synthesis_url(&message.eval_id, engine_id))
            .json(&SynthesisRequest {
                speaker_id: message.speaker_id,
                task_id: &message.task_id,
            })
            .send()
            .await?;
        Self::ensure_success(response).await?;

        Ok(None)
    }
}
