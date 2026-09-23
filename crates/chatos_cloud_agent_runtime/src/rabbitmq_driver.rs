// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chatos_cloud_agent_protocol::{CloudAgentRunPhase, CloudAgentRunStatus};
use chatos_mcp_service::McpToolCallResult;
use futures_util::StreamExt;
use lapin::{
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, BasicQosOptions,
        ConfirmSelectOptions, ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
    },
    publisher_confirm::Confirmation,
    types::{AMQPValue, FieldTable},
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};
use sha2::{Digest, Sha256};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::{
    consume_cloud_agent_single_step, materialize_mcp_command, CloudAgentConsumeDisposition,
    CloudAgentConsumeInput, CloudAgentModelTrigger, CloudAgentOutboxIntent,
    CloudAgentProfileRegistry, CloudAgentRunStore, CloudAgentSingleStepExecutor,
    CloudAgentStateStore,
};

const DELIVERY_ATTEMPT_HEADER: &str = "x-chatos-delivery-attempt";
const DELIVERY_FAILURE_HEADER: &str = "x-chatos-delivery-failure";
const MAX_DELIVERY_ATTEMPTS: u32 = 8;
const MAX_OUTBOX_PUBLISH_ATTEMPTS: u32 = 8;
const OUTBOX_PUBLISH_CLAIM_TTL: Duration = Duration::from_secs(60);
const MAX_AMQP_SHORT_STRING_BYTES: usize = 255;

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
    pub consumer_concurrency: usize,
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
        if self.prefetch_count == 0
            || self.consumer_concurrency == 0
            || self.outbox_batch_size <= 0
            || self.outbox_reconcile_interval.is_zero()
        {
            return Err(
                "Cloud Agent RabbitMQ prefetch, outbox interval and batch size must be positive"
                    .to_string(),
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

/// Service-owned hooks around the shared Cloud Agent state machine.
///
/// One adapter may serve any number of Agent keys in the owner service. The
/// shared runtime owns delivery decoding, ordering checks, short claims,
/// single-step reduction and outbox materialization; the adapter only builds
/// one model step and performs owner-specific terminal work.
#[async_trait]
pub trait CloudAgentServiceAdapter:
    CloudAgentSingleStepExecutor + Clone + Send + Sync + 'static
{
    fn owner_service(&self) -> &'static str;
    fn cloud_agent_store(&self) -> CloudAgentStateStore;

    async fn finalize_cloud_agent_terminal(&self, agent_run_id: &str) -> Result<(), String>;
}

#[async_trait]
impl CloudAgentServiceAdapter for CloudAgentProfileRegistry {
    fn owner_service(&self) -> &'static str {
        self.owner_service
    }

    fn cloud_agent_store(&self) -> CloudAgentStateStore {
        self.store.clone()
    }

    async fn finalize_cloud_agent_terminal(&self, agent_run_id: &str) -> Result<(), String> {
        let run = self
            .store
            .load_run(agent_run_id)
            .await?
            .ok_or_else(|| format!("Cloud Agent run not found: {agent_run_id}"))?;
        if !run.status.is_terminal() {
            return Err("Cloud Agent lifecycle arrived before terminal state".to_string());
        }
        self.profile_for(&run)?.finalize_terminal(&run).await
    }
}

#[derive(Clone)]
pub struct CloudAgentServiceRuntime<A> {
    adapter: A,
    output_routing_key: String,
    claim_ttl: chrono::Duration,
}

impl<A> CloudAgentServiceRuntime<A>
where
    A: CloudAgentServiceAdapter,
{
    pub fn new(adapter: A, output_routing_key: impl Into<String>) -> Self {
        Self {
            adapter,
            output_routing_key: output_routing_key.into(),
            claim_ttl: chrono::Duration::seconds(30),
        }
    }

    pub fn with_claim_ttl(mut self, claim_ttl: chrono::Duration) -> Self {
        self.claim_ttl = claim_ttl;
        self
    }
}

#[async_trait]
impl<A> CloudAgentQueueOwner for CloudAgentServiceRuntime<A>
where
    A: CloudAgentServiceAdapter,
{
    fn owner_service(&self) -> &'static str {
        self.adapter.owner_service()
    }

    fn cloud_agent_store(&self) -> CloudAgentStateStore {
        self.adapter.cloud_agent_store()
    }

    async fn consume_cloud_agent_event(
        &self,
        event_id: String,
        agent_run_id: String,
        trigger: CloudAgentModelTrigger,
        expected_status: CloudAgentRunStatus,
        expected_phase: CloudAgentRunPhase,
    ) -> Result<CloudAgentConsumeDisposition, String> {
        consume_cloud_agent_single_step(
            &self.adapter.cloud_agent_store(),
            &self.adapter,
            CloudAgentConsumeInput {
                agent_run_id,
                event_id,
                trigger,
                expected_status,
                expected_phase,
                claim_token: uuid::Uuid::new_v4().to_string(),
                claim_until: chrono::Utc::now() + self.claim_ttl,
                output_routing_key: self.output_routing_key.clone(),
            },
        )
        .await
    }

    async fn finalize_cloud_agent_terminal(&self, agent_run_id: &str) -> Result<(), String> {
        self.adapter
            .finalize_cloud_agent_terminal(agent_run_id)
            .await
    }
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
        let jitter_seed = format!(
            "{}:{}:{}",
            owner.owner_service(),
            std::process::id(),
            uuid::Uuid::new_v4()
        );
        tokio::time::sleep(outbox_reconcile_startup_jitter(
            topology.outbox_reconcile_interval,
            jitter_seed.as_str(),
        ))
        .await;
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
    let semaphore = Arc::new(tokio::sync::Semaphore::new(topology.consumer_concurrency));
    let mut jobs = tokio::task::JoinSet::new();
    while let Some(delivery) = consumer.next().await {
        let delivery = delivery.map_err(|error| error.to_string())?;
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "Cloud Agent consumer concurrency gate closed".to_string())?;
        let owner = owner.clone();
        let channel = channel.clone();
        let topology = topology.clone();
        jobs.spawn(async move {
            let _permit = permit;
            if let Err(error) = process_delivery(&channel, &topology, &owner, delivery).await {
                warn!(
                    owner_service = owner.owner_service(),
                    error = error.as_str(),
                    "Cloud Agent delivery processing failed"
                );
            }
        });
        while jobs.try_join_next().is_some() {}
    }
    while jobs.join_next().await.is_some() {}
    Ok(())
}

async fn process_delivery<O>(
    channel: &Channel,
    topology: &CloudAgentRabbitMqTopology,
    owner: &O,
    delivery: lapin::message::Delivery,
) -> Result<(), String>
where
    O: CloudAgentQueueOwner,
{
    let delivery_attempt = cloud_agent_delivery_attempt(&delivery.properties);
    match consume_delivery(owner, delivery.data.as_slice()).await {
        Ok(
            CloudAgentConsumeDisposition::Committed
            | CloudAgentConsumeDisposition::Duplicate
            | CloudAgentConsumeDisposition::Terminal,
        ) => delivery
            .ack(BasicAckOptions::default())
            .await
            .map_err(|error| error.to_string())?,
        Ok(CloudAgentConsumeDisposition::OutOfOrder | CloudAgentConsumeDisposition::Conflict) => {
            // Ordering conflicts are expected while an earlier event in the same
            // lane is still running (a model step may take much longer than the
            // retry delay). Keep deferring them without consuming the bounded
            // failure budget; only actual processing errors are DLQ-bounded.
            defer_delivery(
                channel,
                topology,
                delivery.data.as_slice(),
                delivery_attempt,
            )
            .await?;
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
            if cloud_agent_delivery_error_is_stale(error.as_str()) {
                delivery
                    .ack(BasicAckOptions::default())
                    .await
                    .map_err(|ack_error| ack_error.to_string())?;
                return Ok(());
            }
            if delivery_attempt >= MAX_DELIVERY_ATTEMPTS {
                dead_letter_delivery(
                    channel,
                    topology,
                    delivery.data.as_slice(),
                    delivery_attempt,
                    error.as_str(),
                )
                .await?;
            } else {
                defer_delivery(
                    channel,
                    topology,
                    delivery.data.as_slice(),
                    delivery_attempt.saturating_add(1),
                )
                .await?;
            }
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|ack_error| ack_error.to_string())?;
        }
    }
    Ok(())
}

fn cloud_agent_delivery_attempt(properties: &BasicProperties) -> u32 {
    properties
        .headers()
        .as_ref()
        .and_then(|headers| {
            headers
                .inner()
                .iter()
                .find(|(key, _)| key.as_str() == DELIVERY_ATTEMPT_HEADER)
                .and_then(|(_, value)| match value {
                    AMQPValue::LongUInt(value) => Some(*value),
                    _ => None,
                })
        })
        .filter(|attempt| *attempt > 0)
        .unwrap_or(1)
}

fn cloud_agent_delivery_error_is_stale(error: &str) -> bool {
    [
        "Cloud Agent run not found:",
        "Task Run not found:",
        "Task not found:",
        "parent Task Run not found:",
        "parent Cloud Agent run not found",
    ]
    .iter()
    .any(|prefix| error.starts_with(prefix))
}

fn cloud_agent_delivery_headers(attempt: u32, failure: Option<&str>) -> FieldTable {
    let mut headers = FieldTable::default();
    headers.insert(
        DELIVERY_ATTEMPT_HEADER.into(),
        AMQPValue::LongUInt(attempt.max(1)),
    );
    if let Some(failure) = failure {
        headers.insert(
            DELIVERY_FAILURE_HEADER.into(),
            AMQPValue::LongString(truncate_delivery_failure(failure).into()),
        );
    }
    headers
}

fn truncate_delivery_failure(error: &str) -> String {
    const MAX_CHARS: usize = 1_024;
    error.chars().take(MAX_CHARS).collect()
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
    let claim_token = uuid::Uuid::new_v4().to_string();
    let claim_until = chrono::Utc::now()
        + chrono::Duration::from_std(OUTBOX_PUBLISH_CLAIM_TTL)
            .map_err(|error| format!("invalid Cloud Agent outbox claim TTL: {error}"))?;
    let mut pending = store
        .claim_ready_outbox_with_attempts(
            topology.outbox_batch_size,
            claim_token.as_str(),
            claim_until,
        )
        .await?;
    if pending.is_empty() {
        return Ok(0);
    }
    pending.sort_by(|left, right| {
        left.intent
            .available_at
            .cmp(&right.intent.available_at)
            .then_with(|| left.intent.event_id.cmp(&right.intent.event_id))
    });
    let (connection, channel) = open_publisher(topology).await?;
    let _connection = connection;
    let mut published = 0usize;
    let mut state_errors = Vec::new();
    for record in pending {
        let intent = record.intent;
        match publish_intent(&channel, topology, &store, &intent).await {
            Ok(()) => {
                store
                    .mark_claimed_outbox_published(intent.event_id.as_str(), claim_token.as_str())
                    .await?;
                published = published.saturating_add(1);
            }
            Err(error) => {
                let next_attempt = record.publish_attempts.saturating_add(1);
                let next_available_at = chrono::Utc::now()
                    + chrono::Duration::from_std(outbox_publish_retry_delay(next_attempt))
                        .unwrap_or_else(|_| chrono::Duration::minutes(5));
                match store
                    .mark_claimed_outbox_publish_failed(
                        intent.event_id.as_str(),
                        claim_token.as_str(),
                        error.as_str(),
                        next_available_at,
                        MAX_OUTBOX_PUBLISH_ATTEMPTS,
                    )
                    .await
                {
                    Ok(Some(failure)) => warn!(
                        owner_service = owner.owner_service(),
                        event_id = intent.event_id.as_str(),
                        publish_attempts = failure.publish_attempts,
                        dead_lettered = failure.dead_lettered,
                        error = error.as_str(),
                        "Cloud Agent outbox event publish failed"
                    ),
                    Ok(None) => {}
                    Err(state_error) => state_errors.push(state_error),
                }
            }
        }
    }
    if !state_errors.is_empty() {
        return Err(format!(
            "failed to persist {} Cloud Agent outbox publish failures: {}",
            state_errors.len(),
            state_errors.join("; ")
        ));
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

include!("rabbitmq_driver_part01.rs");
