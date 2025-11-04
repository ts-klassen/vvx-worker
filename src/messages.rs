use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskMessage {
    pub eval_id: String,
    pub speaker_id: u32,
    pub task_id: String,
    pub text: Option<String>,
    pub output_dir: Option<String>,
    pub result_filename: Option<String>,
}

impl Default for TaskMessage {
    fn default() -> Self {
        Self {
            eval_id: String::new(),
            speaker_id: 0,
            task_id: String::new(),
            text: None,
            output_dir: None,
            result_filename: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskResultMessage {
    pub eval_id: String,
    pub task_id: String,
    pub engine_id: u32,
    pub speaker_id: u32,
    pub success: bool,
    pub error: Option<String>,
    pub output_file: Option<String>,
}

impl Default for TaskResultMessage {
    fn default() -> Self {
        Self {
            eval_id: String::new(),
            task_id: String::new(),
            engine_id: 0,
            speaker_id: 0,
            success: false,
            error: None,
            output_file: None,
        }
    }
}
