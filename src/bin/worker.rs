use futures::StreamExt;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions, BasicQosOptions,
    ExchangeDeclareOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind};
use std::env;
use std::error::Error;
use vvx_worker::{MockTtsEngine, TaskMessage, TaskResultMessage, TtsEngine};

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
    let result_exchange = env::var("RESULT_EXCHANGE")
        .unwrap_or_else(|_| DEFAULT_RESULT_EXCHANGE.to_string());

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

                let process_result = handle_delivery(&engine, engine_id, &task).await;
                let success = process_result.is_ok();
                let error = process_result.err().map(|err| err.to_string());

                let result_message = TaskResultMessage {
                    eval_id: task.eval_id.clone(),
                    task_id: task.task_id.clone(),
                    engine_id,
                    speaker_id: task.speaker_id,
                    success,
                    error,
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
                        "engine {} completed task {} (speaker {})",
                        engine_id, result_message.task_id, result_message.speaker_id
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

async fn handle_delivery(
    engine: &MockTtsEngine,
    engine_id: u32,
    message: &TaskMessage,
) -> Result<(), vvx_worker::EngineError> {
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

    Ok(())
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
