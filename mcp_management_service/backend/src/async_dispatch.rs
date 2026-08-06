// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::{error::Error, fmt};

use chatos_queue_observability::{
    RabbitMqQueueInspector, RabbitMqQueueRuntimeStats, RabbitMqQueueSpec,
};
use futures_util::StreamExt;
use lapin::{
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicGetOptions, BasicNackOptions,
        BasicPublishOptions, BasicQosOptions, ConfirmSelectOptions, ExchangeDeclareOptions,
        QueueBindOptions, QueueDeclareOptions,
    },
    publisher_confirm::Confirmation,
    types::{AMQPValue, FieldTable},
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::api::mcp::execute_async_tool_call;
use crate::config::{AsyncToolDispatchMode, AsyncToolDispatchTopology};
use crate::runtime::RuntimeInvocationRecord;
use crate::state::AppState;

const RABBITMQ_CONSUMER_TAG: &str = "mcp-management-async-tool-dispatch";
const RABBITMQ_CANCELLATION_CONSUMER_TAG: &str = "mcp-management-invocation-cancellations";
const INITIAL_DELIVERY_ATTEMPT: u32 = 1;

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

struct RabbitMqPublisher {
    _connection: Connection,
    channel: Channel,
    exchange: String,
    queue_name: String,
    cancellation_exchange: String,
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

async fn run_rabbitmq_consumer_loop(state: AppState, topology: AsyncToolDispatchTopology) {
    let semaphore = Arc::new(Semaphore::new(topology.worker_concurrency));
    loop {
        match open_rabbitmq_consumer(&topology).await {
            Ok((connection, channel, mut consumer)) => {
                let _connection = connection;
                state.async_tool_dispatch.set_consumer_connected(true);
                info!(
                    queue = topology.queue_name.as_deref().unwrap_or_default(),
                    exchange = topology.rabbitmq_exchange.as_deref().unwrap_or_default(),
                    "mcp management async tool dispatch worker connected to rabbitmq"
                );
                while let Some(delivery) = consumer.next().await {
                    match delivery {
                        Ok(delivery) => {
                            let permit = match semaphore.clone().acquire_owned().await {
                                Ok(permit) => permit,
                                Err(_) => break,
                            };
                            let state = state.clone();
                            let topology = topology.clone();
                            let channel = channel.clone();
                            tokio::spawn(async move {
                                if let Err(error) = handle_rabbitmq_delivery(
                                    state, topology, channel, delivery, permit,
                                )
                                .await
                                {
                                    warn!(
                                        error = error.as_str(),
                                        "mcp management async tool dispatch delivery handling failed"
                                    );
                                }
                            });
                        }
                        Err(error) => {
                            warn!(
                                error = error.to_string().as_str(),
                                "mcp management async tool dispatch consumer stream failed"
                            );
                            break;
                        }
                    }
                }
                state.async_tool_dispatch.set_consumer_connected(false);
            }
            Err(error) => {
                state.async_tool_dispatch.set_consumer_connected(false);
                warn!(
                    error = error.as_str(),
                    "mcp management async tool dispatch worker failed to connect to rabbitmq"
                );
            }
        }
        tokio::time::sleep(topology.rabbitmq_reconnect_delay).await;
    }
}

async fn run_cancellation_consumer_loop(state: AppState, topology: AsyncToolDispatchTopology) {
    loop {
        match open_cancellation_consumer(&topology).await {
            Ok((connection, mut consumer)) => {
                let _connection = connection;
                state
                    .async_tool_dispatch
                    .set_cancellation_consumer_connected(true);
                if let Err(error) = state
                    .runtime_invocations
                    .reconcile_cancellation_waiters()
                    .await
                {
                    warn!(
                        error = error.as_str(),
                        "reconcile MCP invocation cancellation waiters failed"
                    );
                }
                while let Some(delivery) = consumer.next().await {
                    match delivery {
                        Ok(delivery) => {
                            match serde_json::from_slice::<InvocationCancellationEvent>(
                                delivery.data.as_slice(),
                            ) {
                                Ok(event) => {
                                    if let Err(error) = state
                                        .runtime_invocations
                                        .signal_cancellation(event.invocation_id.as_str())
                                    {
                                        warn!(
                                            invocation_id = event.invocation_id.as_str(),
                                            error = error.as_str(),
                                            "signal MCP invocation cancellation failed"
                                        );
                                    }
                                }
                                Err(error) => warn!(
                                    error = error.to_string().as_str(),
                                    "invalid MCP invocation cancellation event"
                                ),
                            }
                            if let Err(error) = delivery.ack(BasicAckOptions::default()).await {
                                warn!(
                                    error = error.to_string().as_str(),
                                    "acknowledge MCP invocation cancellation event failed"
                                );
                            }
                        }
                        Err(error) => {
                            warn!(
                                error = error.to_string().as_str(),
                                "MCP invocation cancellation consumer stream failed"
                            );
                            break;
                        }
                    }
                }
                state
                    .async_tool_dispatch
                    .set_cancellation_consumer_connected(false);
            }
            Err(error) => {
                state
                    .async_tool_dispatch
                    .set_cancellation_consumer_connected(false);
                warn!(
                    error = error.as_str(),
                    "MCP invocation cancellation consumer failed to connect to rabbitmq"
                );
            }
        }
        tokio::time::sleep(topology.rabbitmq_reconnect_delay).await;
    }
}

async fn handle_rabbitmq_delivery(
    state: AppState,
    topology: AsyncToolDispatchTopology,
    channel: Channel,
    delivery: lapin::message::Delivery,
    permit: OwnedSemaphorePermit,
) -> Result<(), String> {
    let envelope = match serde_json::from_slice::<QueuedAsyncToolCallEnvelope>(&delivery.data) {
        Ok(envelope) => envelope.normalize_delivery_attempt(),
        Err(error) => {
            let dead_letter_queue = topology
                .dead_letter_queue_name
                .as_deref()
                .unwrap_or_default();
            if let Err(publish_error) = publish_payload(
                &channel,
                topology.rabbitmq_exchange.as_deref().unwrap_or_default(),
                dead_letter_queue,
                delivery.data.as_slice(),
            )
            .await
            {
                delivery
                    .nack(BasicNackOptions {
                        multiple: false,
                        requeue: true,
                    })
                    .await
                    .map_err(|nack_error| nack_error.to_string())?;
                return Err(format!(
                    "publish invalid async envelope to DLQ failed: {publish_error}"
                ));
            }
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|ack_error| ack_error.to_string())?;
            return Err(format!("invalid async tool dispatch envelope: {error}"));
        }
    };
    let outcome = process_envelope(state.clone(), &envelope).await;
    drop(permit);
    settle_rabbitmq_delivery(&state, &topology, &channel, delivery, envelope, outcome).await
}

async fn settle_rabbitmq_delivery(
    state: &AppState,
    topology: &AsyncToolDispatchTopology,
    channel: &Channel,
    delivery: lapin::message::Delivery,
    envelope: QueuedAsyncToolCallEnvelope,
    outcome: ProcessOutcome,
) -> Result<(), String> {
    match outcome {
        ProcessOutcome::Ack => delivery
            .ack(BasicAckOptions::default())
            .await
            .map_err(|error| error.to_string()),
        ProcessOutcome::Retry(error) => {
            let (target_queue, retry_envelope, exhausted_message) =
                if let Some(retry) = envelope.next_retry(topology.max_delivery_attempts) {
                    warn!(
                        invocation_id = envelope.invocation_id.as_str(),
                        delivery_attempt = retry.delivery_attempt,
                        max_delivery_attempts = topology.max_delivery_attempts,
                        retry_delay_ms = topology.retry_delay.as_millis(),
                        error = error.as_str(),
                        "rabbitmq async tool dispatch scheduled a retry"
                    );
                    (
                        topology.retry_queue_name.as_deref().unwrap_or_default(),
                        retry,
                        None,
                    )
                } else {
                    let message = format!(
                        "async tool dispatch failed after {} attempts: {error}",
                        envelope.delivery_attempt.max(INITIAL_DELIVERY_ATTEMPT)
                    );
                    (
                        topology
                            .dead_letter_queue_name
                            .as_deref()
                            .unwrap_or_default(),
                        envelope.clone(),
                        Some(message),
                    )
                };
            if let Err(publish_error) = publish_envelope_to_queue(
                &channel,
                topology.rabbitmq_exchange.as_deref().unwrap_or_default(),
                target_queue,
                &retry_envelope,
            )
            .await
            {
                delivery
                    .nack(BasicNackOptions {
                        multiple: false,
                        requeue: true,
                    })
                    .await
                    .map_err(|nack_error| nack_error.to_string())?;
                return Err(format!(
                    "republish async tool dispatch failed: {publish_error}"
                ));
            }
            if let Some(message) = exhausted_message {
                if let Err(persist_error) =
                    fail_async_invocation(state, envelope.invocation_id.as_str(), message.as_str())
                        .await
                {
                    delivery
                        .nack(BasicNackOptions {
                            multiple: false,
                            requeue: true,
                        })
                        .await
                        .map_err(|nack_error| nack_error.to_string())?;
                    return Err(format!(
                        "persist exhausted async invocation failure failed: {persist_error}"
                    ));
                }
            }
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|ack_error| ack_error.to_string())
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

fn unavailable_rabbitmq_queue_stats() -> RabbitMqQueueRuntimeStats {
    RabbitMqQueueRuntimeStats {
        enabled: true,
        available: false,
        queues: Vec::new(),
        error: Some("rabbitmq_queue_inspection_unavailable".to_string()),
    }
}

async fn open_rabbitmq_publisher(
    topology: &AsyncToolDispatchTopology,
) -> Result<RabbitMqPublisher, AsyncToolEnqueueError> {
    let rabbitmq_url = topology.rabbitmq_url.as_deref().ok_or_else(|| {
        AsyncToolEnqueueError::Unavailable(
            "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL is required for RabbitMQ dispatch".to_string(),
        )
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
    ensure_rabbitmq_topology(&channel, topology)
        .await
        .map_err(AsyncToolEnqueueError::Unavailable)?;
    let exchange = topology.rabbitmq_exchange.clone().ok_or_else(|| {
        AsyncToolEnqueueError::Unavailable(
            "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_EXCHANGE is required for RabbitMQ dispatch"
                .to_string(),
        )
    })?;
    let queue_name = topology.queue_name.clone().ok_or_else(|| {
        AsyncToolEnqueueError::Unavailable(
            "MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE is required for RabbitMQ dispatch"
                .to_string(),
        )
    })?;
    let cancellation_exchange = topology.cancellation_exchange.clone().ok_or_else(|| {
        AsyncToolEnqueueError::Unavailable(
            "MCP_MANAGEMENT_INVOCATION_CANCELLATION_EXCHANGE is required for RabbitMQ dispatch"
                .to_string(),
        )
    })?;
    Ok(RabbitMqPublisher {
        _connection: connection,
        channel,
        exchange,
        queue_name,
        cancellation_exchange,
    })
}

async fn publish_envelope_to_queue(
    channel: &Channel,
    exchange: &str,
    queue_name: &str,
    envelope: &QueuedAsyncToolCallEnvelope,
) -> Result<(), AsyncToolEnqueueError> {
    let payload = serde_json::to_vec(envelope)
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
    publish_payload(channel, exchange, queue_name, payload.as_slice()).await
}

async fn publish_payload(
    channel: &Channel,
    exchange: &str,
    queue_name: &str,
    payload: &[u8],
) -> Result<(), AsyncToolEnqueueError> {
    let confirmation = channel
        .basic_publish(
            exchange,
            queue_name,
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload,
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2),
        )
        .await
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?
        .await
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
    ensure_publish_confirmed(queue_name, confirmation)
}

fn ensure_publish_confirmed(
    queue_name: &str,
    confirmation: Confirmation,
) -> Result<(), AsyncToolEnqueueError> {
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(AsyncToolEnqueueError::Unavailable(format!(
            "RabbitMQ returned unroutable MCP async tool event for {queue_name}"
        ))),
        Confirmation::Nack(_) => Err(AsyncToolEnqueueError::CapacityExhausted),
        Confirmation::NotRequested => Err(AsyncToolEnqueueError::Unavailable(
            "RabbitMQ publisher confirm was not enabled for MCP async tool event".to_string(),
        )),
    }
}

async fn open_rabbitmq_consumer(
    topology: &AsyncToolDispatchTopology,
) -> Result<(Connection, Channel, lapin::Consumer), String> {
    let rabbitmq_url = topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL is required for RabbitMQ dispatch".to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|error| error.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| error.to_string())?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    ensure_rabbitmq_topology(&channel, topology).await?;
    let prefetch_count = u16::try_from(topology.worker_concurrency).map_err(|_| {
        "MCP async tool worker concurrency exceeds RabbitMQ prefetch range".to_string()
    })?;
    channel
        .basic_qos(prefetch_count, BasicQosOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    let consumer = channel
        .basic_consume(
            topology.queue_name.as_deref().unwrap_or_default(),
            RABBITMQ_CONSUMER_TAG,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok((connection, channel, consumer))
}

async fn ensure_rabbitmq_topology(
    channel: &Channel,
    topology: &AsyncToolDispatchTopology,
) -> Result<(), String> {
    let exchange = topology.rabbitmq_exchange.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_EXCHANGE is required for RabbitMQ dispatch".to_string()
    })?;
    let queue_name = topology.queue_name.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE is required for RabbitMQ dispatch".to_string()
    })?;
    let retry_queue_name = topology.retry_queue_name.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_RETRY_QUEUE is required for RabbitMQ dispatch".to_string()
    })?;
    let dead_letter_queue_name = topology.dead_letter_queue_name.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_DEAD_LETTER_QUEUE is required for RabbitMQ dispatch".to_string()
    })?;
    channel
        .exchange_declare(
            exchange,
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let cancellation_exchange = topology.cancellation_exchange.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_INVOCATION_CANCELLATION_EXCHANGE is required for RabbitMQ dispatch"
            .to_string()
    })?;
    channel
        .exchange_declare(
            cancellation_exchange,
            ExchangeKind::Fanout,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let dispatch_arguments = dispatch_queue_arguments(topology);

    channel
        .queue_declare(
            queue_name,
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            dispatch_arguments,
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_bind(
            queue_name,
            exchange,
            queue_name,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let retry_delay_ms = u32::try_from(topology.retry_delay.as_millis())
        .map_err(|_| "MCP async retry delay is too large for RabbitMQ".to_string())?;
    let mut retry_arguments = FieldTable::default();
    retry_arguments.insert("x-message-ttl".into(), AMQPValue::LongUInt(retry_delay_ms));
    retry_arguments.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(exchange.into()),
    );
    retry_arguments.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(queue_name.into()),
    );
    channel
        .queue_declare(
            retry_queue_name,
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
            retry_queue_name,
            exchange,
            retry_queue_name,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_declare(
            dead_letter_queue_name,
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
            dead_letter_queue_name,
            exchange,
            dead_letter_queue_name,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn open_cancellation_consumer(
    topology: &AsyncToolDispatchTopology,
) -> Result<(Connection, lapin::Consumer), String> {
    let rabbitmq_url = topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL is required for cancellation events".to_string()
    })?;
    let cancellation_exchange = topology.cancellation_exchange.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_INVOCATION_CANCELLATION_EXCHANGE is required for cancellation events"
            .to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|error| error.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| error.to_string())?;
    channel
        .exchange_declare(
            cancellation_exchange,
            ExchangeKind::Fanout,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let queue = channel
        .queue_declare(
            "",
            QueueDeclareOptions {
                exclusive: true,
                auto_delete: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let queue_name = queue.name().as_str();
    channel
        .queue_bind(
            queue_name,
            cancellation_exchange,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let prefetch_count = u16::try_from(topology.worker_concurrency)
        .map_err(|_| "MCP cancellation consumer prefetch exceeds RabbitMQ range".to_string())?;
    channel
        .basic_qos(prefetch_count, BasicQosOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    let consumer = channel
        .basic_consume(
            queue_name,
            RABBITMQ_CANCELLATION_CONSUMER_TAG,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok((connection, consumer))
}

fn dispatch_queue_arguments(topology: &AsyncToolDispatchTopology) -> FieldTable {
    let mut dispatch_arguments = FieldTable::default();
    dispatch_arguments.insert(
        "x-max-length".into(),
        AMQPValue::LongUInt(topology.queue_max_length),
    );
    dispatch_arguments.insert(
        "x-max-length-bytes".into(),
        AMQPValue::LongLongInt(topology.queue_max_bytes as i64),
    );
    dispatch_arguments.insert(
        "x-overflow".into(),
        AMQPValue::LongString("reject-publish".into()),
    );
    dispatch_arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_RABBITMQ_URL_ENV: &str = "CHATOS_MCP_MANAGEMENT_TEST_RABBITMQ_URL";

    fn envelope() -> QueuedAsyncToolCallEnvelope {
        QueuedAsyncToolCallEnvelope {
            invocation_id: "invocation-1".to_string(),
            session_id: "session-1".to_string(),
            resource_id: "resource-1".to_string(),
            exposed_tool_name: "tool-1".to_string(),
            arguments: serde_json::json!({}),
            mutation_may_have_started: false,
            delivery_attempt: INITIAL_DELIVERY_ATTEMPT,
        }
    }

    #[test]
    fn delivery_retry_is_bounded_and_monotonic() {
        let first = envelope();
        let second = first.next_retry(3).expect("second attempt");
        let third = second.next_retry(3).expect("third attempt");

        assert_eq!(second.delivery_attempt, 2);
        assert_eq!(third.delivery_attempt, 3);
        assert!(third.next_retry(3).is_none());
    }

    #[test]
    fn legacy_envelope_without_attempt_starts_at_one() {
        let value = serde_json::json!({
            "invocation_id": "invocation-1",
            "session_id": "session-1",
            "resource_id": "resource-1",
            "exposed_tool_name": "tool-1",
            "arguments": {},
            "mutation_may_have_started": false
        });
        let envelope = serde_json::from_value::<QueuedAsyncToolCallEnvelope>(value).unwrap();

        assert_eq!(envelope.delivery_attempt, INITIAL_DELIVERY_ATTEMPT);
    }

    #[test]
    fn dispatch_queue_arguments_apply_hard_backpressure_limits() {
        let topology = crate::config::AppConfig::test().async_tool_dispatch_topology;
        let arguments = dispatch_queue_arguments(&topology);

        assert_eq!(
            arguments.inner().get("x-max-length"),
            Some(&AMQPValue::LongUInt(topology.queue_max_length))
        );
        assert_eq!(
            arguments.inner().get("x-max-length-bytes"),
            Some(&AMQPValue::LongLongInt(topology.queue_max_bytes as i64))
        );
        assert_eq!(
            arguments.inner().get("x-overflow"),
            Some(&AMQPValue::LongString("reject-publish".into()))
        );
    }

    #[test]
    fn publisher_nack_is_reported_as_queue_backpressure() {
        let error = ensure_publish_confirmed("mcp.async", Confirmation::Nack(None))
            .expect_err("publisher nack must fail");
        assert_eq!(error, AsyncToolEnqueueError::CapacityExhausted);
    }

    #[test]
    fn enqueue_runtime_stats_distinguish_capacity_from_infrastructure_failure() {
        let dispatch =
            AsyncToolDispatch::new(crate::config::AppConfig::test().async_tool_dispatch_topology);
        dispatch.record_enqueue_result(&Ok(()));
        dispatch.record_enqueue_result(&Err(AsyncToolEnqueueError::CapacityExhausted));
        dispatch.record_enqueue_result(&Err(AsyncToolEnqueueError::Unavailable(
            "connection failed".to_string(),
        )));

        let stats = dispatch.runtime_stats();
        assert_eq!(stats.enqueue_accepted_total, 1);
        assert_eq!(stats.enqueue_capacity_rejected_total, 1);
        assert_eq!(stats.enqueue_unavailable_total, 1);
    }

    #[tokio::test]
    #[ignore = "requires CHATOS_MCP_MANAGEMENT_TEST_RABBITMQ_URL"]
    async fn rabbitmq_recovers_publisher_and_routes_retry_and_exhaustion_to_dlq() {
        let rabbitmq_url = std::env::var(TEST_RABBITMQ_URL_ENV).expect(TEST_RABBITMQ_URL_ENV);
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let mut topology = crate::config::AppConfig::test().async_tool_dispatch_topology;
        topology.mode = AsyncToolDispatchMode::RabbitMq;
        topology.worker_concurrency = 1;
        topology.rabbitmq_reconnect_delay = std::time::Duration::from_millis(50);
        topology.retry_delay = std::time::Duration::from_millis(150);
        topology.max_delivery_attempts = 2;
        topology.rabbitmq_url = Some(rabbitmq_url);
        topology.rabbitmq_exchange = Some(format!("mcp.test.{suffix}"));
        topology.cancellation_exchange = Some(format!("mcp.test.cancel.{suffix}"));
        topology.queue_name = Some(format!("mcp.test.dispatch.{suffix}"));
        topology.retry_queue_name = Some(format!("mcp.test.retry.{suffix}"));
        topology.dead_letter_queue_name = Some(format!("mcp.test.dlq.{suffix}"));

        let dispatch = AsyncToolDispatch::new(topology.clone());
        let original_publisher = dispatch
            .rabbitmq_publisher()
            .await
            .expect("open test publisher");
        let (_consumer_connection, consumer_channel, mut consumer) =
            open_rabbitmq_consumer(&topology)
                .await
                .expect("open test consumer");

        let mut first = envelope();
        first.invocation_id = format!("publisher-before-close-{suffix}");
        dispatch
            .enqueue(first.clone())
            .await
            .expect("publish first");
        let first_delivery =
            tokio::time::timeout(std::time::Duration::from_secs(3), consumer.next())
                .await
                .expect("first delivery timeout")
                .expect("first consumer ended")
                .expect("first delivery");
        assert_eq!(
            serde_json::from_slice::<QueuedAsyncToolCallEnvelope>(&first_delivery.data)
                .expect("decode first delivery")
                .invocation_id,
            first.invocation_id
        );
        first_delivery
            .ack(BasicAckOptions::default())
            .await
            .expect("ack first delivery");

        original_publisher
            ._connection
            .close(200, "test publisher disconnect")
            .await
            .expect("close test publisher connection");
        let mut after_disconnect = envelope();
        after_disconnect.invocation_id = format!("publisher-after-close-{suffix}");
        assert!(matches!(
            dispatch.enqueue(after_disconnect.clone()).await,
            Err(AsyncToolEnqueueError::Unavailable(_))
        ));
        assert!(!dispatch.runtime_stats().publisher_connected);

        dispatch
            .enqueue(after_disconnect.clone())
            .await
            .expect("publisher reconnects on next enqueue");
        let recovered_delivery =
            tokio::time::timeout(std::time::Duration::from_secs(3), consumer.next())
                .await
                .expect("recovered delivery timeout")
                .expect("recovered consumer ended")
                .expect("recovered delivery");
        assert_eq!(
            serde_json::from_slice::<QueuedAsyncToolCallEnvelope>(&recovered_delivery.data)
                .expect("decode recovered delivery")
                .invocation_id,
            after_disconnect.invocation_id
        );
        recovered_delivery
            .ack(BasicAckOptions::default())
            .await
            .expect("ack recovered delivery");
        assert!(dispatch.runtime_stats().publisher_connected);

        let recovered_publisher = dispatch
            .rabbitmq_publisher()
            .await
            .expect("load recovered publisher");
        let mut retry = envelope();
        retry.invocation_id = format!("retry-return-{suffix}");
        retry.delivery_attempt = 2;
        let retry_started = std::time::Instant::now();
        publish_envelope_to_queue(
            &recovered_publisher.channel,
            topology.rabbitmq_exchange.as_deref().unwrap(),
            topology.retry_queue_name.as_deref().unwrap(),
            &retry,
        )
        .await
        .expect("publish retry delivery");
        let retry_delivery =
            tokio::time::timeout(std::time::Duration::from_secs(3), consumer.next())
                .await
                .expect("retry delivery timeout")
                .expect("retry consumer ended")
                .expect("retry delivery");
        assert!(retry_started.elapsed() >= std::time::Duration::from_millis(100));
        assert_eq!(
            serde_json::from_slice::<QueuedAsyncToolCallEnvelope>(&retry_delivery.data)
                .expect("decode retry delivery")
                .invocation_id,
            retry.invocation_id
        );
        retry_delivery
            .ack(BasicAckOptions::default())
            .await
            .expect("ack retry delivery");

        let mut exhausted = envelope();
        exhausted.invocation_id = format!("retry-exhausted-{suffix}");
        exhausted.delivery_attempt = topology.max_delivery_attempts;
        publish_envelope_to_queue(
            &recovered_publisher.channel,
            topology.rabbitmq_exchange.as_deref().unwrap(),
            topology.queue_name.as_deref().unwrap(),
            &exhausted,
        )
        .await
        .expect("publish exhausted delivery");
        let exhausted_delivery =
            tokio::time::timeout(std::time::Duration::from_secs(3), consumer.next())
                .await
                .expect("exhausted delivery timeout")
                .expect("exhausted consumer ended")
                .expect("exhausted delivery");
        let state = AppState::new(crate::config::AppConfig::test())
            .await
            .expect("test state");
        settle_rabbitmq_delivery(
            &state,
            &topology,
            &consumer_channel,
            exhausted_delivery,
            exhausted.clone(),
            ProcessOutcome::Retry("forced acceptance failure".to_string()),
        )
        .await
        .expect("route exhausted delivery to DLQ");

        let queue_stats = dispatch.rabbitmq_queue_stats().await;
        assert!(queue_stats.enabled);
        assert!(queue_stats.available);
        assert_eq!(queue_stats.queues.len(), 3);
        assert!(queue_stats
            .queues
            .iter()
            .any(|queue| queue.role == "dispatch" && queue.consumers >= 1));
        assert!(queue_stats
            .queues
            .iter()
            .any(|queue| queue.role == "dead_letter" && queue.messages == 1));

        let mut dead_letter_consumer = consumer_channel
            .basic_consume(
                topology.dead_letter_queue_name.as_deref().unwrap(),
                format!("mcp-test-dlq-{suffix}").as_str(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .expect("open DLQ consumer");
        let dead_letter = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            dead_letter_consumer.next(),
        )
        .await
        .expect("DLQ delivery timeout")
        .expect("DLQ consumer ended")
        .expect("DLQ delivery");
        let dead_letter_envelope =
            serde_json::from_slice::<QueuedAsyncToolCallEnvelope>(&dead_letter.data)
                .expect("decode DLQ delivery");
        assert_eq!(dead_letter_envelope.invocation_id, exhausted.invocation_id);
        assert_eq!(
            dead_letter_envelope.delivery_attempt,
            topology.max_delivery_attempts
        );
        dead_letter
            .ack(BasicAckOptions::default())
            .await
            .expect("ack DLQ delivery");

        for queue in [
            topology.queue_name.as_deref().unwrap(),
            topology.retry_queue_name.as_deref().unwrap(),
            topology.dead_letter_queue_name.as_deref().unwrap(),
        ] {
            consumer_channel
                .queue_delete(queue, lapin::options::QueueDeleteOptions::default())
                .await
                .expect("delete test queue");
        }
        consumer_channel
            .exchange_delete(
                topology.cancellation_exchange.as_deref().unwrap(),
                lapin::options::ExchangeDeleteOptions::default(),
            )
            .await
            .expect("delete test cancellation exchange");
        consumer_channel
            .exchange_delete(
                topology.rabbitmq_exchange.as_deref().unwrap(),
                lapin::options::ExchangeDeleteOptions::default(),
            )
            .await
            .expect("delete test exchange");
    }

    #[test]
    fn dlq_archive_match_requires_full_invocation_identity_and_exhausted_attempt() {
        let mut record = RuntimeInvocationRecord {
            invocation_id: "invocation-1".to_string(),
            session_id: "session-1".to_string(),
            request_id_key: "request-1".to_string(),
            caller_service: "task-runner".to_string(),
            tenant_id: "tenant-1".to_string(),
            owner_user_id: "user-1".to_string(),
            project_id: "project-1".to_string(),
            device_id: None,
            resource_id: "resource-1".to_string(),
            exposed_tool_name: "tool-1".to_string(),
            original_tool_name: "tool-1".to_string(),
            mutation_may_have_started: false,
            cancel_supported: false,
            status: crate::runtime::RuntimeInvocationStatus::Failed,
            async_execution: true,
            created_at_unix_ms: 1,
            started_at_unix_ms: None,
            completed_at_unix_ms: Some(2),
            terminal_result: None,
            terminal_error_code: Some(-32603),
            terminal_error_message: Some("async tool dispatch failed after 5 attempts".to_string()),
            file_modification_outcome: None,
            result_reply_to: Some("mcp.results.test".to_string()),
            result_event_id: Some("event-1".to_string()),
            result_event_pending: false,
            expires_at: mongodb::bson::DateTime::from_millis(10_000),
            expires_at_unix: 10,
        };
        let payload = serde_json::to_vec(&QueuedAsyncToolCallEnvelope {
            delivery_attempt: 5,
            ..envelope()
        })
        .unwrap();
        assert!(async_tool_dead_letter_matches(&payload, &record, 5));
        record.resource_id = "resource-2".to_string();
        assert!(!async_tool_dead_letter_matches(&payload, &record, 5));
        assert!(!async_tool_dead_letter_matches(&payload, &record, 6));
    }
}
