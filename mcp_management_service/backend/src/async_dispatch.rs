// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::{error::Error, fmt};

use chatos_queue_observability::{
    RabbitMqQueueInspector, RabbitMqQueueRuntimeStats, RabbitMqQueueSpec,
};
use lapin::{
    options::{BasicAckOptions, BasicGetOptions, BasicNackOptions, BasicPublishOptions},
    publisher_confirm::Confirmation,
    BasicProperties,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex, Semaphore};
use tokio::task::JoinHandle;
use tracing::warn;

use crate::api::mcp::execute_async_tool_call;
use crate::config::{AsyncToolDispatchMode, AsyncToolDispatchTopology};
use crate::runtime::RuntimeInvocationRecord;
use crate::state::AppState;

const RABBITMQ_CONSUMER_TAG: &str = "mcp-management-async-tool-dispatch";
const RABBITMQ_CANCELLATION_CONSUMER_TAG: &str = "mcp-management-invocation-cancellations";
const INITIAL_DELIVERY_ATTEMPT: u32 = 1;

mod rabbitmq;
#[cfg(test)]
mod tests;

#[cfg(test)]
use rabbitmq::{
    dispatch_queue_arguments, ensure_publish_confirmed, open_rabbitmq_consumer,
    settle_rabbitmq_delivery,
};
use rabbitmq::{
    open_rabbitmq_publisher, publish_envelope_to_queue, run_cancellation_consumer_loop,
    run_rabbitmq_consumer_loop, unavailable_rabbitmq_queue_stats, RabbitMqPublisher,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedAsyncToolCallEnvelope {
    pub invocation_id: String,
    pub session_id: String,
    pub resource_id: String,
    pub exposed_tool_name: String,
    pub arguments: Value,
    pub mutation_may_have_started: bool,
    #[serde(default = "initial_delivery_attempt")]
    pub delivery_attempt: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct InvocationCancellationEvent {
    invocation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncToolEnqueueError {
    CapacityExhausted,
    Unavailable(String),
}

impl fmt::Display for AsyncToolEnqueueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapacityExhausted => formatter.write_str(
                "RabbitMQ rejected the MCP async tool event because queue capacity is exhausted",
            ),
            Self::Unavailable(message) => formatter.write_str(message),
        }
    }
}

impl Error for AsyncToolEnqueueError {}

#[derive(Debug, Clone, Serialize)]
pub struct AsyncToolDispatchRuntimeStats {
    pub enqueue_accepted_total: u64,
    pub enqueue_capacity_rejected_total: u64,
    pub enqueue_unavailable_total: u64,
    pub publisher_connected: bool,
    pub consumer_connected: bool,
    pub cancellation_consumer_connected: bool,
    pub result_publisher_connected: bool,
}

#[derive(Default)]
struct AsyncToolDispatchMetrics {
    enqueue_accepted_total: AtomicU64,
    enqueue_capacity_rejected_total: AtomicU64,
    enqueue_unavailable_total: AtomicU64,
    publisher_connected: AtomicBool,
    consumer_connected: AtomicBool,
    cancellation_consumer_connected: AtomicBool,
    result_publisher_connected: AtomicBool,
}

fn initial_delivery_attempt() -> u32 {
    INITIAL_DELIVERY_ATTEMPT
}

impl QueuedAsyncToolCallEnvelope {
    fn normalize_delivery_attempt(mut self) -> Self {
        self.delivery_attempt = self.delivery_attempt.max(INITIAL_DELIVERY_ATTEMPT);
        self
    }

    fn next_retry(&self, max_delivery_attempts: u32) -> Option<Self> {
        let current = self.delivery_attempt.max(INITIAL_DELIVERY_ATTEMPT);
        if current >= max_delivery_attempts {
            return None;
        }
        let mut next = self.clone();
        next.delivery_attempt = current.saturating_add(1);
        Some(next)
    }
}

#[derive(Clone)]
pub struct AsyncToolDispatch {
    topology: AsyncToolDispatchTopology,
    local_sender: Arc<Mutex<Option<mpsc::Sender<QueuedAsyncToolCallEnvelope>>>>,
    rabbitmq_publisher: Arc<Mutex<Option<Arc<RabbitMqPublisher>>>>,
    rabbitmq_inspector: Arc<Mutex<Option<Arc<RabbitMqQueueInspector>>>>,
    metrics: Arc<AsyncToolDispatchMetrics>,
}

enum ProcessOutcome {
    Ack,
    Retry(String),
}

impl AsyncToolDispatch {
    pub fn new(topology: AsyncToolDispatchTopology) -> Self {
        Self {
            topology,
            local_sender: Arc::new(Mutex::new(None)),
            rabbitmq_publisher: Arc::new(Mutex::new(None)),
            rabbitmq_inspector: Arc::new(Mutex::new(None)),
            metrics: Arc::new(AsyncToolDispatchMetrics::default()),
        }
    }

    pub fn topology(&self) -> &AsyncToolDispatchTopology {
        &self.topology
    }

    pub fn runtime_stats(&self) -> AsyncToolDispatchRuntimeStats {
        AsyncToolDispatchRuntimeStats {
            enqueue_accepted_total: self.metrics.enqueue_accepted_total.load(Ordering::Relaxed),
            enqueue_capacity_rejected_total: self
                .metrics
                .enqueue_capacity_rejected_total
                .load(Ordering::Relaxed),
            enqueue_unavailable_total: self
                .metrics
                .enqueue_unavailable_total
                .load(Ordering::Relaxed),
            publisher_connected: self.metrics.publisher_connected.load(Ordering::Relaxed),
            consumer_connected: self.metrics.consumer_connected.load(Ordering::Relaxed),
            cancellation_consumer_connected: self
                .metrics
                .cancellation_consumer_connected
                .load(Ordering::Relaxed),
            result_publisher_connected: self
                .metrics
                .result_publisher_connected
                .load(Ordering::Relaxed),
        }
    }

    pub async fn rabbitmq_queue_stats(&self) -> RabbitMqQueueRuntimeStats {
        if self.topology.mode != AsyncToolDispatchMode::RabbitMq {
            return RabbitMqQueueRuntimeStats::disabled();
        }
        let inspector = match self.rabbitmq_inspector().await {
            Ok(inspector) => inspector,
            Err(()) => return unavailable_rabbitmq_queue_stats(),
        };
        let Some(dispatch_queue) = self.topology.queue_name.as_ref() else {
            return unavailable_rabbitmq_queue_stats();
        };
        let Some(retry_queue) = self.topology.retry_queue_name.as_ref() else {
            return unavailable_rabbitmq_queue_stats();
        };
        let Some(dead_letter_queue) = self.topology.dead_letter_queue_name.as_ref() else {
            return unavailable_rabbitmq_queue_stats();
        };
        inspector
            .inspect(&[
                RabbitMqQueueSpec::new("dispatch", dispatch_queue),
                RabbitMqQueueSpec::new("retry", retry_queue),
                RabbitMqQueueSpec::new("dead_letter", dead_letter_queue),
            ])
            .await
    }

    async fn rabbitmq_inspector(&self) -> Result<Arc<RabbitMqQueueInspector>, ()> {
        let mut guard = self.rabbitmq_inspector.lock().await;
        if let Some(inspector) = guard.as_ref() {
            return Ok(inspector.clone());
        }
        let rabbitmq_url = self.topology.rabbitmq_url.as_deref().ok_or(())?;
        let inspector = Arc::new(RabbitMqQueueInspector::new(rabbitmq_url).map_err(|_| ())?);
        *guard = Some(inspector.clone());
        Ok(inspector)
    }

    pub(crate) fn set_consumer_connected(&self, connected: bool) {
        self.metrics
            .consumer_connected
            .store(connected, Ordering::Relaxed);
    }

    pub(crate) fn set_result_publisher_connected(&self, connected: bool) {
        self.metrics
            .result_publisher_connected
            .store(connected, Ordering::Relaxed);
    }

    fn set_cancellation_consumer_connected(&self, connected: bool) {
        self.metrics
            .cancellation_consumer_connected
            .store(connected, Ordering::Relaxed);
    }

    pub async fn start_local_worker(&self, state: AppState) -> Result<(), String> {
        if self.topology.mode != AsyncToolDispatchMode::LocalQueue {
            return Ok(());
        }
        let mut guard = self.local_sender.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        let (sender, receiver) = mpsc::channel(self.topology.local_queue_buffer);
        spawn_local_worker(state, receiver, sender.clone(), self.topology.clone());
        *guard = Some(sender);
        Ok(())
    }

    pub async fn enqueue(
        &self,
        envelope: QueuedAsyncToolCallEnvelope,
    ) -> Result<(), AsyncToolEnqueueError> {
        let envelope = envelope.normalize_delivery_attempt();
        let result = match self.topology.mode {
            AsyncToolDispatchMode::LocalQueue => {
                let sender = self.local_sender.lock().await.clone().ok_or_else(|| {
                    AsyncToolEnqueueError::Unavailable(
                        "local async tool dispatch worker is not started".to_string(),
                    )
                })?;
                sender.send(envelope).await.map_err(|_| {
                    AsyncToolEnqueueError::Unavailable(
                        "local async tool dispatch queue is closed".to_string(),
                    )
                })
            }
            AsyncToolDispatchMode::RabbitMq => self.publish_rabbitmq(&envelope).await,
        };
        self.record_enqueue_result(&result);
        result
    }

    fn record_enqueue_result(&self, result: &Result<(), AsyncToolEnqueueError>) {
        match result {
            Ok(()) => {
                self.metrics
                    .enqueue_accepted_total
                    .fetch_add(1, Ordering::Relaxed);
            }
            Err(AsyncToolEnqueueError::CapacityExhausted) => {
                self.metrics
                    .enqueue_capacity_rejected_total
                    .fetch_add(1, Ordering::Relaxed);
            }
            Err(AsyncToolEnqueueError::Unavailable(_)) => {
                self.metrics
                    .enqueue_unavailable_total
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    async fn publish_rabbitmq(
        &self,
        envelope: &QueuedAsyncToolCallEnvelope,
    ) -> Result<(), AsyncToolEnqueueError> {
        let publisher = self.rabbitmq_publisher().await?;
        let result = publish_envelope_to_queue(
            &publisher.channel,
            publisher.exchange.as_str(),
            publisher.queue_name.as_str(),
            envelope,
        )
        .await;
        if matches!(result, Err(AsyncToolEnqueueError::Unavailable(_))) {
            self.metrics
                .publisher_connected
                .store(false, Ordering::Relaxed);
            let mut guard = self.rabbitmq_publisher.lock().await;
            if guard
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &publisher))
            {
                *guard = None;
            }
        }
        result
    }

    async fn rabbitmq_publisher(&self) -> Result<Arc<RabbitMqPublisher>, AsyncToolEnqueueError> {
        let mut guard = self.rabbitmq_publisher.lock().await;
        if let Some(publisher) = guard.as_ref() {
            return Ok(publisher.clone());
        }
        let publisher = Arc::new(open_rabbitmq_publisher(&self.topology).await?);
        self.metrics
            .publisher_connected
            .store(true, Ordering::Relaxed);
        *guard = Some(publisher.clone());
        Ok(publisher)
    }

    pub fn spawn_rabbitmq_consumer(&self, state: AppState) -> Option<JoinHandle<()>> {
        if self.topology.mode != AsyncToolDispatchMode::RabbitMq {
            return None;
        }
        let topology = self.topology.clone();
        Some(tokio::spawn(async move {
            run_rabbitmq_consumer_loop(state, topology).await;
        }))
    }

    pub fn spawn_cancellation_consumer(&self, state: AppState) -> Option<JoinHandle<()>> {
        if self.topology.mode != AsyncToolDispatchMode::RabbitMq {
            return None;
        }
        let topology = self.topology.clone();
        Some(tokio::spawn(async move {
            run_cancellation_consumer_loop(state, topology).await;
        }))
    }

    pub async fn publish_cancellation(
        &self,
        invocation_id: &str,
    ) -> Result<(), AsyncToolEnqueueError> {
        if self.topology.mode != AsyncToolDispatchMode::RabbitMq {
            return Ok(());
        }
        let publisher = self.rabbitmq_publisher().await?;
        let payload = serde_json::to_vec(&InvocationCancellationEvent {
            invocation_id: invocation_id.to_string(),
        })
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
        let confirmation = publisher
            .channel
            .basic_publish(
                publisher.cancellation_exchange.as_str(),
                "",
                BasicPublishOptions {
                    mandatory: true,
                    ..BasicPublishOptions::default()
                },
                payload.as_slice(),
                BasicProperties::default()
                    .with_content_type("application/json".into())
                    .with_delivery_mode(2),
            )
            .await
            .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?
            .await
            .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
        match confirmation {
            Confirmation::Ack(None) => Ok(()),
            Confirmation::Ack(Some(_)) => Err(AsyncToolEnqueueError::Unavailable(
                "RabbitMQ returned unroutable MCP cancellation event".to_string(),
            )),
            Confirmation::Nack(_) => Err(AsyncToolEnqueueError::Unavailable(
                "RabbitMQ rejected MCP cancellation event".to_string(),
            )),
            Confirmation::NotRequested => Err(AsyncToolEnqueueError::Unavailable(
                "RabbitMQ publisher confirm was not enabled for MCP cancellation event".to_string(),
            )),
        }
    }

    pub async fn archive_dead_lettered_invocation(
        &self,
        record: &RuntimeInvocationRecord,
        scan_limit: usize,
    ) -> Result<bool, String> {
        if self.topology.mode != AsyncToolDispatchMode::RabbitMq {
            return Err("MCP async tool DLQ archival requires RabbitMQ dispatch".to_string());
        }
        let publisher = open_rabbitmq_publisher(&self.topology)
            .await
            .map_err(|error| error.to_string())?;
        let dead_letter_queue = self
            .topology
            .dead_letter_queue_name
            .as_deref()
            .ok_or_else(|| "MCP async tool dead-letter queue is not configured".to_string())?;
        let mut matched = Vec::new();
        let mut unmatched = Vec::new();
        for _ in 0..scan_limit.clamp(1, 1_000) {
            let Some(delivery) = publisher
                .channel
                .basic_get(dead_letter_queue, BasicGetOptions::default())
                .await
                .map_err(|error| error.to_string())?
            else {
                break;
            };
            if async_tool_dead_letter_matches(
                delivery.data.as_slice(),
                record,
                self.topology.max_delivery_attempts,
            ) {
                matched.push(delivery);
            } else {
                unmatched.push(delivery);
            }
        }

        let archived = !matched.is_empty();
        let mut first_error = None;
        for delivery in unmatched {
            if let Err(error) = delivery
                .nack(BasicNackOptions {
                    multiple: false,
                    requeue: true,
                })
                .await
            {
                if first_error.is_none() {
                    first_error = Some(error.to_string());
                }
            }
        }
        if first_error.is_none() {
            for delivery in matched {
                if let Err(error) = delivery.ack(BasicAckOptions::default()).await {
                    if first_error.is_none() {
                        first_error = Some(error.to_string());
                    }
                }
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(archived)
    }
}

fn async_tool_dead_letter_matches(
    payload: &[u8],
    record: &RuntimeInvocationRecord,
    max_delivery_attempts: u32,
) -> bool {
    serde_json::from_slice::<QueuedAsyncToolCallEnvelope>(payload).is_ok_and(|envelope| {
        envelope.invocation_id == record.invocation_id
            && envelope.session_id == record.session_id
            && envelope.resource_id == record.resource_id
            && envelope.exposed_tool_name == record.exposed_tool_name
            && envelope.mutation_may_have_started == record.mutation_may_have_started
            && envelope.delivery_attempt >= max_delivery_attempts
    })
}

fn spawn_local_worker(
    state: AppState,
    mut receiver: mpsc::Receiver<QueuedAsyncToolCallEnvelope>,
    sender: mpsc::Sender<QueuedAsyncToolCallEnvelope>,
    topology: AsyncToolDispatchTopology,
) {
    tokio::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(topology.worker_concurrency));
        while let Some(envelope) = receiver.recv().await {
            let permit = match semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => break,
            };
            let state = state.clone();
            let sender = sender.clone();
            let topology = topology.clone();
            tokio::spawn(async move {
                let outcome = process_envelope(state.clone(), &envelope).await;
                drop(permit);
                if let ProcessOutcome::Retry(error) = outcome {
                    retry_local_envelope(state, sender, topology, envelope, error).await;
                }
            });
        }
    });
}

async fn retry_local_envelope(
    state: AppState,
    sender: mpsc::Sender<QueuedAsyncToolCallEnvelope>,
    topology: AsyncToolDispatchTopology,
    envelope: QueuedAsyncToolCallEnvelope,
    error: String,
) {
    let Some(retry) = envelope.next_retry(topology.max_delivery_attempts) else {
        let message = format!(
            "async tool dispatch failed after {} attempts: {error}",
            envelope.delivery_attempt.max(INITIAL_DELIVERY_ATTEMPT)
        );
        if let Err(persist_error) =
            fail_async_invocation(&state, envelope.invocation_id.as_str(), message.as_str()).await
        {
            warn!(
                invocation_id = envelope.invocation_id.as_str(),
                error = persist_error.as_str(),
                retry_delay_ms = topology.retry_delay.as_millis(),
                "persist exhausted local async invocation failure failed; retrying terminal persistence"
            );
            tokio::time::sleep(topology.retry_delay).await;
            if sender.send(envelope).await.is_err() {
                warn!("local async dispatch queue closed while retrying terminal persistence");
            }
        }
        return;
    };
    warn!(
        invocation_id = envelope.invocation_id.as_str(),
        delivery_attempt = retry.delivery_attempt,
        max_delivery_attempts = topology.max_delivery_attempts,
        retry_delay_ms = topology.retry_delay.as_millis(),
        error = error.as_str(),
        "local async tool dispatch worker scheduled a retry"
    );
    tokio::time::sleep(topology.retry_delay).await;
    if sender.send(retry).await.is_err() {
        let message = "local async tool dispatch retry queue is closed";
        if let Err(persist_error) =
            fail_async_invocation(&state, envelope.invocation_id.as_str(), message).await
        {
            warn!(
                invocation_id = envelope.invocation_id.as_str(),
                error = persist_error.as_str(),
                "persist local async retry queue failure failed"
            );
        }
    }
}
async fn process_envelope(
    state: AppState,
    envelope: &QueuedAsyncToolCallEnvelope,
) -> ProcessOutcome {
    let snapshot = match state
        .runtime_sessions
        .get(envelope.session_id.as_str())
        .await
    {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return match fail_async_invocation(
                &state,
                envelope.invocation_id.as_str(),
                "runtime session was not found or has expired",
            )
            .await
            {
                Ok(()) => ProcessOutcome::Ack,
                Err(error) => ProcessOutcome::Retry(error),
            };
        }
        Err(error) => {
            return ProcessOutcome::Retry(format!(
                "load Runtime Session Snapshot for async dispatch failed: {error}"
            ));
        }
    };
    let Some(route) = snapshot
        .routes
        .iter()
        .find(|route| route.resource_id == envelope.resource_id)
        .cloned()
    else {
        return match fail_async_invocation(
            &state,
            envelope.invocation_id.as_str(),
            "runtime route snapshot is missing for queued async invocation",
        )
        .await
        {
            Ok(()) => ProcessOutcome::Ack,
            Err(error) => ProcessOutcome::Retry(error),
        };
    };
    let Some(tool) = snapshot
        .tools
        .iter()
        .find(|tool| {
            tool.resource_id == envelope.resource_id
                && tool.exposed_name == envelope.exposed_tool_name
        })
        .cloned()
    else {
        return match fail_async_invocation(
            &state,
            envelope.invocation_id.as_str(),
            "runtime tool snapshot is missing for queued async invocation",
        )
        .await
        {
            Ok(()) => ProcessOutcome::Ack,
            Err(error) => ProcessOutcome::Retry(error),
        };
    };
    match execute_async_tool_call(
        state,
        snapshot,
        route,
        tool,
        envelope.arguments.clone(),
        envelope.invocation_id.clone(),
        envelope.mutation_may_have_started,
    )
    .await
    {
        Ok(()) => ProcessOutcome::Ack,
        Err(error) => ProcessOutcome::Retry(error),
    }
}

async fn fail_async_invocation(
    state: &AppState,
    invocation_id: &str,
    message: &str,
) -> Result<(), String> {
    state
        .runtime_invocations
        .fail(
            invocation_id,
            chatos_mcp_service::MCP_ERROR_INTERNAL,
            message.to_string(),
        )
        .await
        .map(|_| ())
}
