use lapin::options::{BasicPublishOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Connection, ConnectionProperties};
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::error::Error;
use std::time::Duration;
use vvx_worker::TaskMessage;

const DEFAULT_QUEUE: &str = "vvx_tasks";
const DEFAULT_AMQP: &str = "amqp://guest:guest@127.0.0.1:5672/%2f";
const DEFAULT_API: &str = "http://127.0.0.1:8080/api/v1";

type ClientResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Deserialize)]
struct EvaluationResponse {
    eval_id: String,
    config: BenchmarkConfig,
}

#[derive(Debug, Deserialize)]
struct BenchmarkConfig {
    engine_count: u32,
}

#[derive(Debug, Deserialize)]
struct TasksResponse {
    tasks: Vec<TaskDescriptor>,
}

#[derive(Debug, Deserialize)]
struct TaskDescriptor {
    task_id: String,
    speaker_id: u32,
}

#[derive(Debug, Deserialize)]
struct MetricsResponse {
    score: f64,
}

#[tokio::main]
async fn main() -> ClientResult<()> {
    let api_base = env::var("VXMB_API").unwrap_or_else(|_| DEFAULT_API.to_string());
    let amqp_addr = env::var("AMQP_ADDR").unwrap_or_else(|_| DEFAULT_AMQP.to_string());
    let queue_name = env::var("TASK_QUEUE").unwrap_or_else(|_| DEFAULT_QUEUE.to_string());

    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let evaluation = create_evaluation(&http_client, &api_base).await?;
    if evaluation.config.engine_count == 0 {
        return Err("engine_count reported as zero".into());
    }

    println!(
        "Created evaluation {} with {} engines",
        evaluation.eval_id, evaluation.config.engine_count
    );

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

    loop {
        let tasks = fetch_tasks(&http_client, &api_base, &evaluation.eval_id).await?;
        if tasks.is_empty() {
            break;
        }

        for task in tasks {
            let message = TaskMessage {
                eval_id: evaluation.eval_id.clone(),
                speaker_id: task.speaker_id,
                task_id: task.task_id,
            };

            let payload = serde_json::to_vec(&message)?;
            channel
                .basic_publish(
                    "",
                    &queue_name,
                    BasicPublishOptions::default(),
                    &payload,
                    BasicProperties::default().with_delivery_mode(2),
                )
                .await?;
        }
    }

    let metrics = fetch_metrics(&http_client, &api_base, &evaluation.eval_id).await?;
    println!("Current score: {}", metrics.score);

    connection.close(0, "").await?;

    Ok(())
}

async fn create_evaluation(
    client: &reqwest::Client,
    api_base: &str,
) -> ClientResult<EvaluationResponse> {
    let url = format!("{}/evaluations", api_base);
    let response = client.post(&url).json(&json!({})).send().await?;
    let evaluation = response
        .error_for_status()?
        .json::<EvaluationResponse>()
        .await?;
    Ok(evaluation)
}

async fn fetch_tasks(
    client: &reqwest::Client,
    api_base: &str,
    eval_id: &str,
) -> ClientResult<Vec<TaskDescriptor>> {
    let url = format!("{}/evaluations/{}/tasks", api_base, eval_id);
    let response = client.post(&url).json(&json!({})).send().await?;
    let parsed = response.error_for_status()?.json::<TasksResponse>().await?;
    Ok(parsed.tasks)
}

async fn fetch_metrics(
    client: &reqwest::Client,
    api_base: &str,
    eval_id: &str,
) -> ClientResult<MetricsResponse> {
    let url = format!("{}/evaluations/{}/metrics", api_base, eval_id);
    let response = client.get(&url).send().await?;
    let metrics = response
        .error_for_status()?
        .json::<MetricsResponse>()
        .await?;
    Ok(metrics)
}
