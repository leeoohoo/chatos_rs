// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use async_trait::async_trait;
use chatos_cloud_agent_protocol::{CloudAgentRunPhase, CloudAgentRunStatus};
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

use crate::{
    materialize_mcp_command, CloudAgentConsumeDisposition, CloudAgentModelTrigger,
    CloudAgentOutboxIntent, CloudAgentRunStore, CloudAgentStateStore,
};

#[derive(Debug, Clone)]
pub struct CloudAgentRabbitMqTopology {
    pub rabbitmq_url: String,
    pub exchange: String,
    pub runtime_queue: String,
    pub retry_queue: String,
    pub consumer_tag: String,
    pub reconnect_delay: Duration,
    pub outbox_reconcile_interval: Duration,
    pub outbox_batch_size: i64,
    pub prefetch_count: u16,
    pub conflict_retry_delay: Duration,
}

impl CloudAgentRabbitMqTopology {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("rabbitmq_url", self.rabbitmq_url.as_str()),
            ("exchange", self.exchange.as_str()),
            ("runtime_queue", self.runtime_queue.as_str()),
            ("retry_queue", self.retry_queue.as_str()),
            ("consumer_tag", self.consumer_tag.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("Cloud Agent RabbitMQ {name} must not be empty"));
            }
        }
        if self.prefetch_count == 0 || self.outbox_batch_size <= 0 {
            return Err(
                "Cloud Agent RabbitMQ prefetch and outbox batch size must be positive".to_string(),
            );
        }
        Ok(())
    }
}

#[async_trait]
pub trait CloudAgentQueueOwner: Clone + Send + Sync + 'static {
    fn owner_service(&self) -> &'static str;
    fn cloud_agent_store(&self) -> CloudAgentStateStore;

    async fn consume_cloud_agent_event(
        &self,
        event_id: String,
        agent_run_id: String,
        trigger: CloudAgentModelTrigger,
        expected_status: CloudAgentRunStatus,
        expected_phase: CloudAgentRunPhase,
    ) -> Result<CloudAgentConsumeDisposition, String>;

    async fn finalize_cloud_agent_terminal(&self, agent_run_id: &str) -> Result<(), String>;
}

pub fn spawn_cloud_agent_outbox_reconciler<O>(
    topology: CloudAgentRabbitMqTopology,
    owner: O,
) -> JoinHandle<()>
where
    O: CloudAgentQueueOwner,
{
    tokio::spawn(async move {
        if let Err(error) = topology.validate() {
            warn!(
                error = error.as_str(),
                "Cloud Agent outbox topology is invalid"
            );
            return;
        }
        let mut interval = tokio::time::interval(topology.outbox_reconcile_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match publish_ready_outbox(&topology, &owner).await {
                Ok(count) if count > 0 => info!(
                    owner_service = owner.owner_service(),
                    published_count = count,
                    "published Cloud Agent outbox events"
                ),
                Ok(_) => {}
                Err(error) => warn!(
                    owner_service = owner.owner_service(),
                    error = error.as_str(),
                    "Cloud Agent outbox publisher failed"
                ),
            }
        }
    })
}

pub fn spawn_cloud_agent_consumer<O>(
    topology: CloudAgentRabbitMqTopology,
    owner: O,
) -> JoinHandle<()>
where
    O: CloudAgentQueueOwner,
{
    tokio::spawn(async move {
        if let Err(error) = topology.validate() {
            warn!(
                error = error.as_str(),
                "Cloud Agent consumer topology is invalid"
            );
            return;
        }
        loop {
            match run_consumer(&topology, &owner).await {
                Ok(()) => warn!(
                    owner_service = owner.owner_service(),
                    "Cloud Agent consumer stopped"
                ),
                Err(error) => warn!(
                    owner_service = owner.owner_service(),
                    error = error.as_str(),
                    "Cloud Agent consumer failed"
                ),
            }
            tokio::time::sleep(topology.reconnect_delay).await;
        }
    })
}

async fn run_consumer<O>(topology: &CloudAgentRabbitMqTopology, owner: &O) -> Result<(), String>
where
    O: CloudAgentQueueOwner,
{
    let connection = Connection::connect(
        topology.rabbitmq_url.as_str(),
        ConnectionProperties::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| error.to_string())?;
    ensure_topology(&channel, topology).await?;
    channel
        .basic_qos(topology.prefetch_count, BasicQosOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    let mut consumer = channel
        .basic_consume(
            topology.runtime_queue.as_str(),
            topology.consumer_tag.as_str(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    info!(
        owner_service = owner.owner_service(),
        queue = topology.runtime_queue.as_str(),
        "Cloud Agent consumer connected"
    );
    while let Some(delivery) = consumer.next().await {
        let delivery = delivery.map_err(|error| error.to_string())?;
        match consume_delivery(owner, delivery.data.as_slice()).await {
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
                warn!(
                    owner_service = owner.owner_service(),
                    error = error.as_str(),
                    "Cloud Agent delivery failed"
                );
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

async fn consume_delivery<O>(
    owner: &O,
    payload: &[u8],
) -> Result<CloudAgentConsumeDisposition, String>
where
    O: CloudAgentQueueOwner,
{
    if let Ok(result) = serde_json::from_slice::<McpToolCallResult>(payload) {
        result.validate()?;
        if result.owner_service != owner.owner_service() {
            return Ok(CloudAgentConsumeDisposition::Duplicate);
        }
        return owner
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
        "owner_lifecycle_terminal" => {
            owner
                .finalize_cloud_agent_terminal(intent.ordering.agent_run_id.as_str())
                .await?;
            return Ok(CloudAgentConsumeDisposition::Committed);
        }
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
    owner
        .consume_cloud_agent_event(
            intent.event_id,
            intent.ordering.agent_run_id,
            trigger,
            expected_status,
            expected_phase,
        )
        .await
}

async fn publish_ready_outbox<O>(
    topology: &CloudAgentRabbitMqTopology,
    owner: &O,
) -> Result<usize, String>
where
    O: CloudAgentQueueOwner,
{
    let store = owner.cloud_agent_store();
    let intents = store.list_ready_outbox(topology.outbox_batch_size).await?;
    if intents.is_empty() {
        return Ok(0);
    }
    let (connection, channel) = open_publisher(topology).await?;
    let _connection = connection;
    let mut published = 0usize;
    for intent in intents {
        publish_intent(&channel, topology, &store, &intent).await?;
        store
            .mark_outbox_published(intent.event_id.as_str())
            .await?;
        published = published.saturating_add(1);
    }
    Ok(published)
}

pub async fn publish_cloud_agent_intent<O>(
    topology: &CloudAgentRabbitMqTopology,
    owner: &O,
    intent: &CloudAgentOutboxIntent,
) -> Result<(), String>
where
    O: CloudAgentQueueOwner,
{
    topology.validate()?;
    let (connection, channel) = open_publisher(topology).await?;
    let _connection = connection;
    publish_intent(&channel, topology, &owner.cloud_agent_store(), intent).await
}

async fn open_publisher(
    topology: &CloudAgentRabbitMqTopology,
) -> Result<(Connection, Channel), String> {
    let connection = Connection::connect(
        topology.rabbitmq_url.as_str(),
        ConnectionProperties::default(),
    )
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

async fn ensure_topology(
    channel: &Channel,
    topology: &CloudAgentRabbitMqTopology,
) -> Result<(), String> {
    channel
        .exchange_declare(
            topology.exchange.as_str(),
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
            topology.runtime_queue.as_str(),
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_bind(
            topology.runtime_queue.as_str(),
            topology.exchange.as_str(),
            topology.runtime_queue.as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let mut retry_arguments = FieldTable::default();
    retry_arguments.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(topology.exchange.clone().into()),
    );
    retry_arguments.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(topology.runtime_queue.clone().into()),
    );
    channel
        .queue_declare(
            topology.retry_queue.as_str(),
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
            topology.retry_queue.as_str(),
            topology.exchange.as_str(),
            topology.retry_queue.as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn defer_delivery(
    channel: &Channel,
    topology: &CloudAgentRabbitMqTopology,
    payload: &[u8],
) -> Result<(), String> {
    let expiration = topology.conflict_retry_delay.as_millis().max(1).to_string();
    let confirmation = channel
        .basic_publish(
            topology.exchange.as_str(),
            topology.retry_queue.as_str(),
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload,
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_expiration(expiration.into()),
        )
        .await
        .map_err(|error| error.to_string())?
        .await
        .map_err(|error| error.to_string())?;
    confirmed("deferred Cloud Agent event", confirmation)
}

async fn publish_intent(
    channel: &Channel,
    topology: &CloudAgentRabbitMqTopology,
    store: &CloudAgentStateStore,
    intent: &CloudAgentOutboxIntent,
) -> Result<(), String> {
    let routing_key = match intent.topic.as_str() {
        "ai_runtime_retry" => topology.retry_queue.as_str(),
        "mcp_tool_call_command" => intent.routing_key.as_str(),
        _ => topology.runtime_queue.as_str(),
    };
    let payload = if intent.topic == "mcp_tool_call_command" {
        let run = store
            .load_run(intent.ordering.agent_run_id.as_str())
            .await?
            .ok_or_else(|| "Cloud Agent run is missing while publishing MCP command".to_string())?;
        let session_ref = run
            .mcp_runtime_session_ref
            .as_deref()
            .ok_or_else(|| "Cloud Agent run has no MCP runtime session".to_string())?;
        serde_json::to_vec(&materialize_mcp_command(
            &run,
            intent,
            session_ref,
            topology.runtime_queue.as_str(),
        )?)
        .map_err(|error| error.to_string())?
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
        topology.exchange.as_str()
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
    confirmed(
        format!("Cloud Agent event for {routing_key}").as_str(),
        confirmation,
    )
}

fn confirmed(label: &str, confirmation: Confirmation) -> Result<(), String> {
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!("RabbitMQ returned unroutable {label}")),
        Confirmation::Nack(_) => Err(format!("RabbitMQ rejected {label}")),
        Confirmation::NotRequested => Err(format!(
            "RabbitMQ confirm mode is required while publishing {label}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topology_requires_distinct_durable_queue_identities() {
        let topology = CloudAgentRabbitMqTopology {
            rabbitmq_url: "amqp://localhost".to_string(),
            exchange: "cloud_agent".to_string(),
            runtime_queue: "cloud_agent.project.runtime".to_string(),
            retry_queue: "cloud_agent.project.runtime.retry".to_string(),
            consumer_tag: "project-cloud-agent".to_string(),
            reconnect_delay: Duration::from_secs(1),
            outbox_reconcile_interval: Duration::from_secs(1),
            outbox_batch_size: 100,
            prefetch_count: 32,
            conflict_retry_delay: Duration::from_secs(1),
        };
        assert!(topology.validate().is_ok());
    }
}
