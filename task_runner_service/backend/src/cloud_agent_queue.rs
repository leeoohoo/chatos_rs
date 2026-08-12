// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_cloud_agent_protocol::{CloudAgentRunPhase, CloudAgentRunStatus};
use chatos_cloud_agent_runtime::CloudAgentOutboxIntent;
use chatos_cloud_agent_runtime::{CloudAgentConsumeDisposition, CloudAgentModelTrigger};
use chatos_mcp_service::McpToolCallResult;
use futures_util::StreamExt;
use lapin::{
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
        BasicQosOptions, ConfirmSelectOptions, ExchangeDeclareOptions, QueueBindOptions,
        QueueDeclareOptions,
    },
    publisher_confirm::Confirmation,
    types::{AMQPValue, FieldTable},
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::platform_queue::TaskQueueTopology;
use crate::services::RunService;

pub(crate) const TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY: &str = "cloud_agent.task_runner.runtime";
pub(crate) const TASK_RUNNER_CLOUD_AGENT_RETRY_ROUTING_KEY: &str =
    "cloud_agent.task_runner.runtime.retry";
pub(crate) const TASK_RUNNER_CLOUD_AGENT_MCP_RESULT_ROUTING_KEY: &str =
    "cloud_agent.task_runner.mcp_results";

pub fn spawn_cloud_agent_outbox_reconciler(
    topology: TaskQueueTopology,
    run_service: RunService,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match publish_ready_outbox(&topology, &run_service, 100).await {
                Ok(count) if count > 0 => info!(
                    published_count = count,
                    "task runner published Cloud Agent outbox events"
                ),
                Ok(_) => {}
                Err(error) => warn!(
                    error = error.as_str(),
                    "task runner Cloud Agent outbox publisher failed"
                ),
            }
        }
    })
}

pub fn spawn_cloud_agent_consumer(
    topology: TaskQueueTopology,
    run_service: RunService,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match run_cloud_agent_consumer(&topology, &run_service).await {
                Ok(()) => warn!("task runner Cloud Agent consumer stopped"),
                Err(error) => warn!(
                    error = error.as_str(),
                    "task runner Cloud Agent consumer failed"
                ),
            }
            tokio::time::sleep(topology.rabbitmq_reconnect_delay).await;
        }
    })
}

async fn run_cloud_agent_consumer(
    topology: &TaskQueueTopology,
    run_service: &RunService,
) -> Result<(), String> {
    let rabbitmq_url = topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "TASK_RUNNER_RABBITMQ_URL is required for Cloud Agent consumption".to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|error| error.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| error.to_string())?;
    ensure_topology(&channel, topology).await?;
    channel
        .basic_qos(32, BasicQosOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    let mut consumer = channel
        .basic_consume(
            TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY,
            "task-runner-cloud-agent-runtime",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    info!(
        queue = TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY,
        "task runner Cloud Agent consumer connected"
    );
    while let Some(delivery) = consumer.next().await {
        let delivery = delivery.map_err(|error| error.to_string())?;
        match consume_delivery(run_service, delivery.data.as_slice()).await {
            Ok(
                CloudAgentConsumeDisposition::Committed
                | CloudAgentConsumeDisposition::Duplicate
                | CloudAgentConsumeDisposition::Terminal,
            ) => delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|error| error.to_string())?,
            Ok(
                CloudAgentConsumeDisposition::OutOfOrder | CloudAgentConsumeDisposition::Conflict,
            ) => {
                defer_delivery(&channel, topology, delivery.data.as_slice()).await?;
                delivery
                    .ack(BasicAckOptions::default())
                    .await
                    .map_err(|error| error.to_string())?;
            }
            Err(error) => {
                warn!(error = error.as_str(), "Cloud Agent delivery failed");
                delivery
                    .nack(BasicNackOptions {
                        multiple: false,
                        requeue: true,
                    })
                    .await
                    .map_err(|nack_error| nack_error.to_string())?;
            }
        }
    }
    Ok(())
}

async fn consume_delivery(
    run_service: &RunService,
    payload: &[u8],
) -> Result<CloudAgentConsumeDisposition, String> {
    if let Ok(result) = serde_json::from_slice::<McpToolCallResult>(payload) {
        result.validate()?;
        if result.owner_service != "task-runner" {
            return Ok(CloudAgentConsumeDisposition::Duplicate);
        }
        return run_service
            .consume_cloud_agent_event(
                result.event_id.clone(),
                result.agent_run_id.clone(),
                CloudAgentModelTrigger::ToolResults {
                    event_id: result.event_id,
                    batch_id: result.batch_id,
                    source_step_seq: result.source_step_seq,
                    items: result
                        .items
                        .into_iter()
                        .map(|item| serde_json::to_value(item).map_err(|error| error.to_string()))
                        .collect::<Result<Vec<_>, _>>()?,
                },
                CloudAgentRunStatus::WaitingToolResult,
                CloudAgentRunPhase::ToolBatch,
            )
            .await;
    }
    let intent = serde_json::from_slice::<CloudAgentOutboxIntent>(payload)
        .map_err(|error| format!("invalid Cloud Agent delivery: {error}"))?;
    let (trigger, expected_status, expected_phase) = match intent.topic.as_str() {
        "run_started" => (
            CloudAgentModelTrigger::RunStarted {
                event_id: intent.event_id.clone(),
                payload: intent.payload.clone(),
            },
            CloudAgentRunStatus::ModelReady,
            CloudAgentRunPhase::Ready,
        ),
        "ai_runtime_continuation" => (
            CloudAgentModelTrigger::Continuation {
                event_id: intent.event_id.clone(),
                payload: intent.payload.clone(),
            },
            CloudAgentRunStatus::ModelReady,
            CloudAgentRunPhase::Ready,
        ),
        "ai_runtime_retry" => (
            CloudAgentModelTrigger::Retry {
                event_id: intent.event_id.clone(),
                model_attempt: intent
                    .payload
                    .get("model_attempt")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .unwrap_or(1),
                payload: intent.payload.clone(),
            },
            CloudAgentRunStatus::RetryScheduled,
            CloudAgentRunPhase::RetryDelay,
        ),
        _ => return Ok(CloudAgentConsumeDisposition::Duplicate),
    };
    run_service
        .consume_cloud_agent_event(
            intent.event_id,
            intent.ordering.agent_run_id,
            trigger,
            expected_status,
            expected_phase,
        )
        .await
}

async fn defer_delivery(
    channel: &Channel,
    topology: &TaskQueueTopology,
    payload: &[u8],
) -> Result<(), String> {
    let confirmation = channel
        .basic_publish(
            topology.rabbitmq_exchange.as_str(),
            TASK_RUNNER_CLOUD_AGENT_RETRY_ROUTING_KEY,
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload,
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_expiration("1000".into()),
        )
        .await
        .map_err(|error| error.to_string())?
        .await
        .map_err(|error| error.to_string())?;
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err("Cloud Agent deferred event was unroutable".to_string()),
        Confirmation::Nack(_) => Err("Cloud Agent deferred event was rejected".to_string()),
        Confirmation::NotRequested => {
            Err("Cloud Agent defer requires publisher confirms".to_string())
        }
    }
}

async fn publish_ready_outbox(
    topology: &TaskQueueTopology,
    run_service: &RunService,
    limit: i64,
) -> Result<usize, String> {
    let intents = run_service.cloud_agent_ready_outbox(limit).await?;
    if intents.is_empty() {
        return Ok(0);
    }
    let (connection, channel) = open_publisher(topology).await?;
    let _connection = connection;
    let mut published = 0usize;
    for intent in intents {
        publish_intent(&channel, topology, run_service, &intent).await?;
        run_service
            .mark_cloud_agent_outbox_published(intent.event_id.as_str())
            .await?;
        published = published.saturating_add(1);
    }
    Ok(published)
}

async fn open_publisher(topology: &TaskQueueTopology) -> Result<(Connection, Channel), String> {
    let rabbitmq_url = topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "TASK_RUNNER_RABBITMQ_URL is required for Cloud Agent outbox publishing".to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|error| error.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| error.to_string())?;
    ensure_topology(&channel, topology).await?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    Ok((connection, channel))
}

pub(crate) async fn ensure_topology(
    channel: &Channel,
    topology: &TaskQueueTopology,
) -> Result<(), String> {
    channel
        .exchange_declare(
            topology.rabbitmq_exchange.as_str(),
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_declare(
            TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY,
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    for routing_key in [
        TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY,
        TASK_RUNNER_CLOUD_AGENT_MCP_RESULT_ROUTING_KEY,
    ] {
        channel
            .queue_bind(
                TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY,
                topology.rabbitmq_exchange.as_str(),
                routing_key,
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|error| error.to_string())?;
    }
    let mut retry_arguments = FieldTable::default();
    retry_arguments.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(topology.rabbitmq_exchange.clone().into()),
    );
    retry_arguments.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY.into()),
    );
    channel
        .queue_declare(
            TASK_RUNNER_CLOUD_AGENT_RETRY_ROUTING_KEY,
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            retry_arguments,
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_bind(
            TASK_RUNNER_CLOUD_AGENT_RETRY_ROUTING_KEY,
            topology.rabbitmq_exchange.as_str(),
            TASK_RUNNER_CLOUD_AGENT_RETRY_ROUTING_KEY,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn publish_intent(
    channel: &Channel,
    topology: &TaskQueueTopology,
    run_service: &RunService,
    intent: &CloudAgentOutboxIntent,
) -> Result<(), String> {
    let routing_key = match intent.topic.as_str() {
        "ai_runtime_retry" => TASK_RUNNER_CLOUD_AGENT_RETRY_ROUTING_KEY,
        "mcp_tool_call_command" => intent.routing_key.as_str(),
        _ => TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY,
    };
    let payload = if intent.topic == "mcp_tool_call_command" {
        let run = run_service
            .cloud_agent_run(intent.ordering.agent_run_id.as_str())
            .await?
            .ok_or_else(|| "Cloud Agent run is missing while publishing MCP command".to_string())?;
        let session_ref = run
            .mcp_runtime_session_ref
            .as_deref()
            .ok_or_else(|| "Cloud Agent run has no MCP runtime session".to_string())?;
        let command = chatos_cloud_agent_runtime::materialize_mcp_command(
            &run,
            intent,
            session_ref,
            TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY,
        )?;
        serde_json::to_vec(&command).map_err(|error| error.to_string())?
    } else {
        serde_json::to_vec(intent).map_err(|error| error.to_string())?
    };
    let mut properties = BasicProperties::default()
        .with_content_type("application/json".into())
        .with_delivery_mode(2)
        .with_message_id(intent.event_id.clone().into())
        .with_correlation_id(intent.correlation_id.clone().into());
    if intent.topic == "ai_runtime_retry" {
        let delay = intent
            .available_at
            .signed_duration_since(chrono::Utc::now())
            .num_milliseconds()
            .max(1);
        properties = properties.with_expiration(delay.to_string().into());
    }
    let exchange = if intent.topic == "mcp_tool_call_command" {
        ""
    } else {
        topology.rabbitmq_exchange.as_str()
    };
    let confirmation = channel
        .basic_publish(
            exchange,
            routing_key,
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload.as_slice(),
            properties,
        )
        .await
        .map_err(|error| error.to_string())?
        .await
        .map_err(|error| error.to_string())?;
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!(
            "RabbitMQ returned unroutable Cloud Agent event for {routing_key}"
        )),
        Confirmation::Nack(_) => Err(format!(
            "RabbitMQ rejected Cloud Agent event for {routing_key}"
        )),
        Confirmation::NotRequested => {
            Err("RabbitMQ confirm mode is required for Cloud Agent outbox".to_string())
        }
    }
}

pub(crate) async fn publish_dependency_resume(
    topology: &TaskQueueTopology,
    run_service: &RunService,
    parent_run_id: &str,
    dependency_run_id: &str,
) -> Result<(), String> {
    let parent_run = run_service
        .get_run(parent_run_id)
        .await?
        .ok_or_else(|| format!("parent Task Run not found: {parent_run_id}"))?;
    let agent_run_id = parent_run
        .agent_run_id
        .as_deref()
        .ok_or_else(|| "parent Task Run has no Cloud Agent run".to_string())?;
    let cloud_run = run_service
        .cloud_agent_run(agent_run_id)
        .await?
        .ok_or_else(|| "parent Cloud Agent run not found".to_string())?;
    if cloud_run.status.is_terminal() {
        return Ok(());
    }
    let intent = CloudAgentOutboxIntent {
        event_id: format!("dependency_ready:{parent_run_id}:{dependency_run_id}"),
        topic: "run_started".to_string(),
        routing_key: TASK_RUNNER_CLOUD_AGENT_ROUTING_KEY.to_string(),
        ordering: cloud_run.ordering,
        causation_id: dependency_run_id.to_string(),
        correlation_id: agent_run_id.to_string(),
        available_at: chrono::Utc::now(),
        payload: serde_json::json!({
            "event_type": "dependency_ready",
            "task_run_id": parent_run_id,
            "dependency_run_id": dependency_run_id,
        }),
    };
    let (connection, channel) = open_publisher(topology).await?;
    let _connection = connection;
    publish_intent(&channel, topology, run_service, &intent).await
}
