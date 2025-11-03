use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskMessage {
    pub eval_id: String,
    pub speaker_id: u32,
    pub task_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskResultMessage {
    pub eval_id: String,
    pub task_id: String,
    pub engine_id: u32,
    pub speaker_id: u32,
    pub success: bool,
    pub error: Option<String>,
}
