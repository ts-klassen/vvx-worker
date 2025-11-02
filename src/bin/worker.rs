use futures::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicQosOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{Connection, ConnectionProperties};
use std::env;
use std::error::Error;
use vvx_worker::{MockTtsEngine, TaskMessage, TtsEngine};

const DEFAULT_QUEUE: &str = "vvx_tasks";
const DEFAULT_AMQP: &str = "amqp://guest:guest@127.0.0.1:5672/%2f";
const DEFAULT_API: &str = "http://127.0.0.1:8080/api/v1";

type WorkerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
struct WorkerConfigError(String);

impl std::fmt::Display for WorkerConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for WorkerConfigError {}

#[tokio::main]
async fn main() -> WorkerResult<()> {
    let mut args = env::args().skip(1);
    let engine_id = if let Some(arg) = args.next() {
        parse_engine_id(&arg)?
    } else {
        let env_value = env::var("ENGINE_ID").map_err(|_| {
            Box::new(WorkerConfigError(
                "provide engine id as first argument or ENGINE_ID environment variable".into(),
            )) as Box<dyn Error + Send + Sync>
        })?;
        parse_engine_id(&env_value)?
    };

    let api_base = env::var("VXMB_API").unwrap_or_else(|_| DEFAULT_API.to_string());
    let amqp_addr = env::var("AMQP_ADDR").unwrap_or_else(|_| DEFAULT_AMQP.to_string());
    let queue_name = env::var("TASK_QUEUE").unwrap_or_else(|_| DEFAULT_QUEUE.to_string());

    let engine = MockTtsEngine::new(api_base.clone());

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
                if let Err(err) = handle_delivery(&engine, engine_id, delivery.data.as_ref()).await
                {
                    eprintln!("engine {}: {}", engine_id, err);
                    delivery
                        .nack(BasicNackOptions {
                            requeue: false,
                            multiple: false,
                        })
                        .await?;
                } else {
                    delivery.ack(BasicAckOptions::default()).await?;
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

async fn handle_delivery(
    engine: &MockTtsEngine,
    engine_id: u32,
    payload: &[u8],
) -> Result<(), vvx_worker::EngineError> {
    let message: TaskMessage = match serde_json::from_slice(payload) {
        Ok(message) => message,
        Err(err) => {
            eprintln!("engine {}: invalid task payload: {}", engine_id, err);
            return Ok(());
        }
    };

    engine
        .set_speaker(&message.eval_id, engine_id, message.speaker_id)
        .await?;
    engine
        .synthesize(
            &message.eval_id,
            engine_id,
            message.speaker_id,
            &message.task_id,
        )
        .await?;

    println!(
        "engine {} completed task {} (speaker {})",
        engine_id, message.task_id, message.speaker_id
    );

    Ok(())
}
