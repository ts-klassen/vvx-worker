use camino::Utf8PathBuf;
use clap::Parser;
use futures::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions, BasicQosOptions,
    ExchangeDeclareOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind};
use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use vvx_worker::{
    MockTtsEngine, TaskMessage, TaskResultMessage, TtsEngine, VoicevoxConfig, VoicevoxTtsEngine,
};

const DEFAULT_QUEUE: &str = "vvx_tasks";
const DEFAULT_AMQP: &str = "amqp://guest:guest@127.0.0.1:5672/%2f";
const DEFAULT_API: &str = "http://127.0.0.1:8080/api/v1";
const DEFAULT_RESULT_EXCHANGE: &str = "vvx_results";

type WorkerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
struct WorkerConfigError(String);

impl std::fmt::Display for WorkerConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for WorkerConfigError {}

#[derive(Debug, Parser)]
#[command(
    name = "vvx-worker",
    about = "RabbitMQ worker that performs VOICEVOX or mock synthesis"
)]
struct Args {
    /// Engine identifier used when reporting results (falls back to ENGINE_ID env var)
    #[arg(value_name = "ENGINE_ID")]
    engine_id: Option<u32>,

    /// Use the mock HTTP engine instead of VOICEVOX.
    #[arg(long)]
    mock: bool,

    /// Path to the ONNX Runtime shared library.
    #[arg(long)]
    voicevox_onnx: Option<PathBuf>,

    /// Path to the Open JTalk dictionary directory.
    #[arg(long)]
    voicevox_dict: Option<PathBuf>,

    /// Directory containing VOICEVOX model assets (.vvm files or folders).
    #[arg(long)]
    voicevox_model_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> WorkerResult<()> {
    let args = Args::parse();

    let engine_id = if let Some(id) = args.engine_id {
        id
    } else {
        let env_value = env::var("ENGINE_ID").map_err(|_| {
            Box::new(WorkerConfigError(
                "provide engine id as positional argument or ENGINE_ID environment variable".into(),
            )) as Box<dyn Error + Send + Sync>
        })?;
        parse_engine_id(&env_value)?
    };

    let api_base = env::var("VXMB_API").unwrap_or_else(|_| DEFAULT_API.to_string());
    let amqp_addr = env::var("AMQP_ADDR").unwrap_or_else(|_| DEFAULT_AMQP.to_string());
    let queue_name = env::var("TASK_QUEUE").unwrap_or_else(|_| DEFAULT_QUEUE.to_string());
    let result_exchange =
        env::var("RESULT_EXCHANGE").unwrap_or_else(|_| DEFAULT_RESULT_EXCHANGE.to_string());

    let engine: Arc<dyn TtsEngine> = if args.mock {
        Arc::new(MockTtsEngine::new(api_base.clone()))
    } else {
        let config = build_voicevox_config(&args)?;
        Arc::new(VoicevoxTtsEngine::new(config)?)
    };

    let connection = Connection::connect(&amqp_addr, ConnectionProperties::default()).await?;
    let channel = connection.create_channel().await?;
    channel
        .queue_declare(
            &queue_name,
            QueueDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;

    channel
        .exchange_declare(
            &result_exchange,
            ExchangeKind::Topic,
            ExchangeDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;

    channel
        .basic_qos(
            1,
            BasicQosOptions {
                global: false,
                ..Default::default()
            },
        )
        .await?;

    let consumer_tag = format!("vvx-worker-{}", engine_id);
    let mut consumer = channel
        .basic_consume(
            &queue_name,
            &consumer_tag,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    println!(
        "Worker for engine {} listening on queue {}",
        engine_id, queue_name
    );

    while let Some(delivery) = consumer.next().await {
        match delivery {
            Ok(delivery) => {
                let task: TaskMessage = match serde_json::from_slice(delivery.data.as_ref()) {
                    Ok(message) => message,
                    Err(err) => {
                        eprintln!("engine {}: invalid task payload: {}", engine_id, err);
                        delivery.ack(BasicAckOptions::default()).await?;
                        continue;
                    }
                };

                let process_result = engine.process_task(engine_id, &task).await;
                let (success, output_file, error) = match process_result {
                    Ok(path) => (true, path, None),
                    Err(err) => (false, None, Some(err.to_string())),
                };

                let result_message = TaskResultMessage {
                    eval_id: task.eval_id.clone(),
                    task_id: task.task_id.clone(),
                    engine_id,
                    speaker_id: task.speaker_id,
                    success,
                    error,
                    output_file,
                };

                if let Err(err) = publish_result(&channel, &result_exchange, &result_message).await
                {
                    eprintln!(
                        "engine {}: failed to publish result for task {}: {}",
                        engine_id, result_message.task_id, err
                    );
                    delivery
                        .nack(BasicNackOptions {
                            requeue: true,
                            multiple: false,
                        })
                        .await?;
                    continue;
                }

                if result_message.success {
                    println!(
                        "engine {} completed task {} (speaker {}){}",
                        engine_id,
                        result_message.task_id,
                        result_message.speaker_id,
                        result_message
                            .output_file
                            .as_ref()
                            .map(|path| format!(" -> {}", path))
                            .unwrap_or_default()
                    );
                    delivery.ack(BasicAckOptions::default()).await?;
                } else {
                    eprintln!(
                        "engine {} failed task {} (speaker {}): {}",
                        engine_id,
                        result_message.task_id,
                        result_message.speaker_id,
                        result_message.error.as_deref().unwrap_or("unknown error")
                    );
                    delivery
                        .nack(BasicNackOptions {
                            requeue: false,
                            multiple: false,
                        })
                        .await?;
                }
            }
            Err(err) => {
                eprintln!("consumer error: {}", err);
            }
        }
    }

    connection.close(0, "").await?;

    Ok(())
}

fn parse_engine_id(value: &str) -> Result<u32, Box<dyn Error + Send + Sync>> {
    value.parse::<u32>().map_err(|_| {
        Box::new(WorkerConfigError(format!("invalid engine id '{}'", value)))
            as Box<dyn Error + Send + Sync>
    })
}

fn build_voicevox_config(args: &Args) -> WorkerResult<VoicevoxConfig> {
    let onnxruntime_path = args
        .voicevox_onnx
        .clone()
        .or_else(|| env::var("VOICEVOX_ORT_LIB").ok().map(PathBuf::from))
        .filter(|path| !path.as_os_str().is_empty());

    if let Some(ref path) = onnxruntime_path {
        if !path.exists() {
            return Err(Box::new(WorkerConfigError(format!(
                "onnx runtime library not found at {}",
                path.display()
            ))) as Box<dyn Error + Send + Sync>);
        }
    }

    let dict_dir_path = args
        .voicevox_dict
        .clone()
        .or_else(|| env::var("VOICEVOX_OPEN_JTALK_DIR").ok().map(PathBuf::from))
        .ok_or_else(|| {
            Box::new(WorkerConfigError(
                "provide --voicevox-dict or VOICEVOX_OPEN_JTALK_DIR".into(),
            )) as Box<dyn Error + Send + Sync>
        })?;

    if !dict_dir_path.exists() {
        return Err(Box::new(WorkerConfigError(format!(
            "open jtalk dictionary directory not found: {}",
            dict_dir_path.display()
        ))) as Box<dyn Error + Send + Sync>);
    }

    let dict_dir = Utf8PathBuf::from_path_buf(dict_dir_path).map_err(|_| {
        Box::new(WorkerConfigError(
            "Open JTalk dictionary path must be valid UTF-8".into(),
        )) as Box<dyn Error + Send + Sync>
    })?;

    let model_dir_path = args
        .voicevox_model_dir
        .clone()
        .or_else(|| env::var("VOICEVOX_MODEL_DIR").ok().map(PathBuf::from))
        .ok_or_else(|| {
            Box::new(WorkerConfigError(
                "provide --voicevox-model-dir or VOICEVOX_MODEL_DIR".into(),
            )) as Box<dyn Error + Send + Sync>
        })?;

    if !model_dir_path.exists() {
        return Err(Box::new(WorkerConfigError(format!(
            "voice model directory not found: {}",
            model_dir_path.display()
        ))) as Box<dyn Error + Send + Sync>);
    }

    let model_dir = Utf8PathBuf::from_path_buf(model_dir_path).map_err(|_| {
        Box::new(WorkerConfigError(
            "voice model directory path must be valid UTF-8".into(),
        )) as Box<dyn Error + Send + Sync>
    })?;

    Ok(VoicevoxConfig {
        onnxruntime_path,
        open_jtalk_dict_dir: dict_dir,
        model_dir,
    })
}

async fn publish_result(
    channel: &Channel,
    exchange: &str,
    result: &TaskResultMessage,
) -> WorkerResult<()> {
    let payload = serde_json::to_vec(result)?;
    channel
        .basic_publish(
            exchange,
            &result.eval_id,
            BasicPublishOptions::default(),
            &payload,
            BasicProperties::default().with_delivery_mode(2),
        )
        .await?
        .await?;
    Ok(())
}
