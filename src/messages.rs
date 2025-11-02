use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskMessage {
    pub eval_id: String,
    pub speaker_id: u32,
    pub task_id: String,
}
