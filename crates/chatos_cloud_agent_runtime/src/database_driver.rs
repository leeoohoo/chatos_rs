// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use async_trait::async_trait;
use chatos_cloud_agent_protocol::{CloudAgentRunPhase, CloudAgentRunStatus};
use tokio::task::{JoinHandle, JoinSet};
use tracing::{info, warn};

use crate::{
    consume_cloud_agent_single_step, CloudAgentConsumeDisposition, CloudAgentConsumeInput,
    CloudAgentModelTrigger, CloudAgentOutboxIntent, CloudAgentProfileRegistry, CloudAgentRunStore,
    CloudAgentSingleStepExecutor, CloudAgentStateStore,
};

const MAX_PROCESSING_ATTEMPTS: u32 = 8;
const OUTBOX_CLAIM_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct CloudAgentDatabaseWorkerConfig {
    pub poll_interval: Duration,
    pub batch_size: i64,
    pub worker_concurrency: usize,
    pub conflict_retry_delay: Duration,
}

impl CloudAgentDatabaseWorkerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.poll_interval.is_zero()
            || self.batch_size <= 0
            || self.worker_concurrency == 0
            || self.conflict_retry_delay.is_zero()
        {
            return Err(
                "Cloud Agent database worker intervals, batch size and concurrency must be positive"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[async_trait]
pub trait CloudAgentWorkerOwner: Clone + Send + Sync + 'static {
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
impl<A> CloudAgentWorkerOwner for CloudAgentServiceRuntime<A>
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

pub fn spawn_cloud_agent_database_worker<O>(
    config: CloudAgentDatabaseWorkerConfig,
    owner: O,
) -> JoinHandle<()>
where
    O: CloudAgentWorkerOwner,
{
    tokio::spawn(async move {
        if let Err(error) = config.validate() {
            warn!(
                error,
                "Cloud Agent database worker configuration is invalid"
            );
            return;
        }
        let mut interval = tokio::time::interval(config.poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match process_ready_outbox(&config, &owner).await {
                Ok(count) if count > 0 => info!(
                    owner_service = owner.owner_service(),
                    processed_count = count,
                    "processed Cloud Agent database outbox events"
                ),
                Ok(_) => {}
                Err(error) => warn!(
                    owner_service = owner.owner_service(),
                    error, "Cloud Agent database worker failed"
                ),
            }
        }
    })
}

async fn process_ready_outbox<O>(
    config: &CloudAgentDatabaseWorkerConfig,
    owner: &O,
) -> Result<usize, String>
where
    O: CloudAgentWorkerOwner,
{
    let store = owner.cloud_agent_store();
    let claim_token = uuid::Uuid::new_v4().to_string();
    let claim_until = chrono::Utc::now()
        + chrono::Duration::from_std(OUTBOX_CLAIM_TTL)
            .map_err(|error| format!("invalid Cloud Agent outbox claim TTL: {error}"))?;
    let mut pending = store
        .claim_ready_outbox_with_attempts(config.batch_size, claim_token.as_str(), claim_until)
        .await?;
    pending.sort_by(|left, right| {
        left.intent
            .available_at
            .cmp(&right.intent.available_at)
            .then_with(|| left.intent.event_id.cmp(&right.intent.event_id))
    });

    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(config.worker_concurrency));
    let mut jobs = JoinSet::new();
    for record in pending {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "Cloud Agent database worker concurrency gate closed".to_string())?;
        let owner = owner.clone();
        let store = store.clone();
        let claim_token = claim_token.clone();
        let conflict_retry_delay = config.conflict_retry_delay;
        jobs.spawn(async move {
            let _permit = permit;
            process_claimed_intent(
                &owner,
                &store,
                claim_token.as_str(),
                record.intent,
                record.publish_attempts,
                conflict_retry_delay,
            )
            .await
        });
    }

    let mut processed = 0usize;
    let mut errors = Vec::new();
    while let Some(result) = jobs.join_next().await {
        match result {
            Ok(Ok(done)) => processed = processed.saturating_add(usize::from(done)),
            Ok(Err(error)) => errors.push(error),
            Err(error) => errors.push(format!("Cloud Agent database worker task failed: {error}")),
        }
    }
    if errors.is_empty() {
        Ok(processed)
    } else {
        Err(errors.join("; "))
    }
}

async fn process_claimed_intent<O>(
    owner: &O,
    store: &CloudAgentStateStore,
    claim_token: &str,
    intent: CloudAgentOutboxIntent,
    attempts: u32,
    conflict_retry_delay: Duration,
) -> Result<bool, String>
where
    O: CloudAgentWorkerOwner,
{
    let result = consume_intent(owner, &intent).await;
    match result {
        Ok(
            CloudAgentConsumeDisposition::Committed
            | CloudAgentConsumeDisposition::Duplicate
            | CloudAgentConsumeDisposition::Terminal,
        ) => {
            store
                .mark_claimed_outbox_published(intent.event_id.as_str(), claim_token)
                .await
        }
        Ok(CloudAgentConsumeDisposition::OutOfOrder | CloudAgentConsumeDisposition::Conflict) => {
            requeue_claimed_intent(
                store,
                claim_token,
                &intent,
                attempts,
                "Cloud Agent ordering conflict",
                conflict_retry_delay,
                u32::MAX,
            )
            .await?;
            Ok(false)
        }
        Err(error) if delivery_error_is_stale(error.as_str()) => {
            store
                .mark_claimed_outbox_published(intent.event_id.as_str(), claim_token)
                .await
        }
        Err(error) => {
            requeue_claimed_intent(
                store,
                claim_token,
                &intent,
                attempts,
                error.as_str(),
                processing_retry_delay(attempts.saturating_add(1)),
                MAX_PROCESSING_ATTEMPTS,
            )
            .await?;
            warn!(
                owner_service = owner.owner_service(),
                event_id = intent.event_id,
                error,
                "Cloud Agent database outbox event failed"
            );
            Ok(false)
        }
    }
}

async fn requeue_claimed_intent(
    store: &CloudAgentStateStore,
    claim_token: &str,
    intent: &CloudAgentOutboxIntent,
    attempts: u32,
    error: &str,
    delay: Duration,
    max_attempts: u32,
) -> Result<(), String> {
    let next_available_at = chrono::Utc::now()
        + chrono::Duration::from_std(delay).unwrap_or_else(|_| chrono::Duration::minutes(5));
    if let Some(failure) = store
        .mark_claimed_outbox_publish_failed(
            intent.event_id.as_str(),
            claim_token,
            error,
            next_available_at,
            max_attempts,
        )
        .await?
    {
        warn!(
            event_id = intent.event_id,
            processing_attempts = attempts.saturating_add(1),
            dead_lettered = failure.dead_lettered,
            error,
            "Cloud Agent database outbox event was deferred"
        );
    }
    Ok(())
}

async fn consume_intent<O>(
    owner: &O,
    intent: &CloudAgentOutboxIntent,
) -> Result<CloudAgentConsumeDisposition, String>
where
    O: CloudAgentWorkerOwner,
{
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
        "mcp_tool_call_command" => {
            return Err(
                "server Cloud Agent profiles cannot dispatch MCP tools after MQ removal"
                    .to_string(),
            )
        }
        _ => return Ok(CloudAgentConsumeDisposition::Duplicate),
    };
    owner
        .consume_cloud_agent_event(
            intent.event_id.clone(),
            intent.ordering.agent_run_id.clone(),
            trigger,
            expected_status,
            expected_phase,
        )
        .await
}

fn delivery_error_is_stale(error: &str) -> bool {
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

fn processing_retry_delay(attempt: u32) -> Duration {
    const MAX_RETRY_DELAY: Duration = Duration::from_secs(5 * 60);
    let exponent = attempt.saturating_sub(1).min(9);
    Duration::from_secs(1_u64 << exponent).min(MAX_RETRY_DELAY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_database_worker_configuration() {
        let mut config = CloudAgentDatabaseWorkerConfig {
            poll_interval: Duration::from_secs(1),
            batch_size: 16,
            worker_concurrency: 4,
            conflict_retry_delay: Duration::from_secs(1),
        };
        assert!(config.validate().is_ok());
        config.worker_concurrency = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn processing_retry_is_bounded() {
        assert_eq!(processing_retry_delay(1), Duration::from_secs(1));
        assert_eq!(processing_retry_delay(99), Duration::from_secs(300));
    }
}
