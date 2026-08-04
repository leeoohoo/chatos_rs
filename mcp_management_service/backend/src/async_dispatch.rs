// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use lapin::{
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
        ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
    },
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::api::mcp::execute_async_tool_call;
use crate::config::{AsyncToolDispatchMode, AsyncToolDispatchTopology};
use crate::state::AppState;

const RABBITMQ_CONSUMER_RECONNECT_DELAY: Duration = Duration::from_secs(3);
const RABBITMQ_CONSUMER_TAG: &str = "mcp-management-async-tool-dispatch";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedAsyncToolCallEnvelope {
    pub invocation_id: String,
    pub session_id: String,
    pub resource_id: String,
    pub exposed_tool_name: String,
    pub arguments: Value,
    pub mutation_may_have_started: bool,
}

#[derive(Clone)]
pub struct AsyncToolDispatch {
    topology: AsyncToolDispatchTopology,
    local_sender: Arc<Mutex<Option<mpsc::Sender<QueuedAsyncToolCallEnvelope>>>>,
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
        }
    }

    pub fn topology(&self) -> &AsyncToolDispatchTopology {
        &self.topology
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
        spawn_local_worker(state, receiver, self.topology.worker_concurrency);
        *guard = Some(sender);
        Ok(())
    }

    pub async fn enqueue(&self, envelope: QueuedAsyncToolCallEnvelope) -> Result<(), String> {
        match self.topology.mode {
            AsyncToolDispatchMode::LocalQueue => {
                let sender =
                    self.local_sender.lock().await.clone().ok_or_else(|| {
                        "local async tool dispatch worker is not started".to_string()
                    })?;
                sender
                    .send(envelope)
                    .await
                    .map_err(|_| "local async tool dispatch queue is closed".to_string())
            }
            AsyncToolDispatchMode::RabbitMq => publish_rabbitmq(&self.topology, &envelope).await,
        }
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
}

fn spawn_local_worker(
    state: AppState,
    mut receiver: mpsc::Receiver<QueuedAsyncToolCallEnvelope>,
    worker_concurrency: usize,
) {
    tokio::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(worker_concurrency));
        while let Some(envelope) = receiver.recv().await {
            let permit = match semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => break,
            };
            let state = state.clone();
            tokio::spawn(async move {
                let _permit = permit;
                if let ProcessOutcome::Retry(error) = process_envelope(state, envelope).await {
                    warn!(
                        error = error.as_str(),
                        "local async tool dispatch worker hit a retryable error; invocation remains queued"
                    );
                }
            });
        }
    });
}

async fn run_rabbitmq_consumer_loop(state: AppState, topology: AsyncToolDispatchTopology) {
    let semaphore = Arc::new(Semaphore::new(topology.worker_concurrency));
    loop {
        match open_rabbitmq_consumer(&topology).await {
            Ok((connection, mut consumer)) => {
                let _connection = connection;
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
                            if let Err(error) =
                                handle_rabbitmq_delivery(state.clone(), delivery, permit).await
                            {
                                warn!(
                                    error = error.as_str(),
                                    "mcp management async tool dispatch delivery handling failed"
                                );
                            }
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
            }
            Err(error) => {
                warn!(
                    error = error.as_str(),
                    "mcp management async tool dispatch worker failed to connect to rabbitmq"
                );
            }
        }
        tokio::time::sleep(RABBITMQ_CONSUMER_RECONNECT_DELAY).await;
    }
}

async fn handle_rabbitmq_delivery(
    state: AppState,
    delivery: lapin::message::Delivery,
    permit: OwnedSemaphorePermit,
) -> Result<(), String> {
    let envelope = match serde_json::from_slice::<QueuedAsyncToolCallEnvelope>(&delivery.data) {
        Ok(envelope) => envelope,
        Err(error) => {
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|ack_error| ack_error.to_string())?;
            return Err(format!("invalid async tool dispatch envelope: {error}"));
        }
    };
    let outcome = process_envelope(state, envelope).await;
    drop(permit);
    match outcome {
        ProcessOutcome::Ack => delivery
            .ack(BasicAckOptions::default())
            .await
            .map_err(|error| error.to_string()),
        ProcessOutcome::Retry(error) => {
            delivery
                .nack(BasicNackOptions {
                    multiple: false,
                    requeue: true,
                })
                .await
                .map_err(|nack_error| nack_error.to_string())?;
            Err(error)
        }
    }
}

async fn process_envelope(
    state: AppState,
    envelope: QueuedAsyncToolCallEnvelope,
) -> ProcessOutcome {
    let snapshot = match state
        .runtime_sessions
        .get(envelope.session_id.as_str())
        .await
    {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            fail_async_invocation(
                &state,
                envelope.invocation_id.as_str(),
                "runtime session was not found or has expired",
            )
            .await;
            return ProcessOutcome::Ack;
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
        fail_async_invocation(
            &state,
            envelope.invocation_id.as_str(),
            "runtime route snapshot is missing for queued async invocation",
        )
        .await;
        return ProcessOutcome::Ack;
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
        fail_async_invocation(
            &state,
            envelope.invocation_id.as_str(),
            "runtime tool snapshot is missing for queued async invocation",
        )
        .await;
        return ProcessOutcome::Ack;
    };
    execute_async_tool_call(
        state,
        snapshot,
        route,
        tool,
        envelope.arguments,
        envelope.invocation_id,
        envelope.mutation_may_have_started,
    )
    .await;
    ProcessOutcome::Ack
}

async fn fail_async_invocation(state: &AppState, invocation_id: &str, message: &str) {
    if let Err(error) = state
        .runtime_invocations
        .fail(
            invocation_id,
            chatos_mcp_service::MCP_ERROR_INTERNAL,
            message.to_string(),
        )
        .await
    {
        warn!(
            invocation_id,
            error = error.as_str(),
            "persist async invocation failure failed"
        );
    }
}

async fn publish_rabbitmq(
    topology: &AsyncToolDispatchTopology,
    envelope: &QueuedAsyncToolCallEnvelope,
) -> Result<(), String> {
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
    ensure_rabbitmq_topology(&channel, topology).await?;
    let exchange = topology.rabbitmq_exchange.as_deref().unwrap_or_default();
    let queue_name = topology.queue_name.as_deref().unwrap_or_default();
    let payload = serde_json::to_vec(envelope).map_err(|error| error.to_string())?;
    channel
        .basic_publish(
            exchange,
            queue_name,
            BasicPublishOptions::default(),
            payload.as_slice(),
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2),
        )
        .await
        .map_err(|error| error.to_string())?
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn open_rabbitmq_consumer(
    topology: &AsyncToolDispatchTopology,
) -> Result<(Connection, lapin::Consumer), String> {
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
    ensure_rabbitmq_topology(&channel, topology).await?;
    let consumer = channel
        .basic_consume(
            topology.queue_name.as_deref().unwrap_or_default(),
            RABBITMQ_CONSUMER_TAG,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok((connection, consumer))
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
    channel
        .queue_declare(
            queue_name,
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
            queue_name,
            exchange,
            queue_name,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}
