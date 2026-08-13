use std::sync::Arc;

use chatos_queue_observability::RabbitMqQueueRuntimeStats;
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
use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::{info, warn};

use crate::config::AsyncToolDispatchTopology;
use crate::state::AppState;
use chatos_mcp_service::{
    McpToolCallCommand, McpToolCallResult, McpToolCallResultItem, McpToolCallResultStatus,
    MCP_ERROR_INTERNAL,
};

use super::{
    AsyncToolEnqueueError, InvocationCancellationEvent, RABBITMQ_CANCELLATION_CONSUMER_TAG,
    RABBITMQ_CONSUMER_TAG, RABBITMQ_INVOCATION_CONSUMER_TAG,
    RABBITMQ_INVOCATION_TERMINAL_CONSUMER_TAG,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InvocationReadyEvent {
    event_id: String,
    batch_id: String,
    call_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InvocationTerminalEvent {
    event_id: String,
    invocation_id: String,
    prompt_id: Option<String>,
}

pub(super) async fn publish_invocation_terminal_event(
    channel: &Channel,
    topology: &AsyncToolDispatchTopology,
    invocation_id: &str,
    prompt_id: Option<&str>,
) -> Result<(), AsyncToolEnqueueError> {
    let event = InvocationTerminalEvent {
        event_id: prompt_id
            .map(|prompt_id| format!("mcp_prompt_terminal_{prompt_id}"))
            .unwrap_or_else(|| format!("mcp_invocation_terminal_{invocation_id}")),
        invocation_id: invocation_id.to_string(),
        prompt_id: prompt_id.map(ToOwned::to_owned),
    };
    let payload = serde_json::to_vec(&event)
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
    publish_payload(
        channel,
        topology.rabbitmq_exchange.as_deref().unwrap_or_default(),
        terminal_queue_name(topology).as_str(),
        payload.as_slice(),
    )
    .await
}

fn invocation_queue_name(topology: &AsyncToolDispatchTopology) -> String {
    format!(
        "{}.invocations",
        topology.queue_name.as_deref().unwrap_or_default()
    )
}

fn terminal_queue_name(topology: &AsyncToolDispatchTopology) -> String {
    format!(
        "{}.terminals",
        topology.queue_name.as_deref().unwrap_or_default()
    )
}

pub(super) struct RabbitMqPublisher {
    pub(super) _connection: Connection,
    pub(super) channel: Channel,
    pub(super) cancellation_exchange: String,
}

pub(super) async fn run_rabbitmq_consumer_loop(
    state: AppState,
    topology: AsyncToolDispatchTopology,
) {
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
                                if let Err(error) = handle_tool_call_command_delivery(
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

async fn handle_tool_call_command_delivery(
    state: AppState,
    topology: AsyncToolDispatchTopology,
    channel: Channel,
    delivery: lapin::message::Delivery,
    permit: OwnedSemaphorePermit,
) -> Result<(), String> {
    let command = match serde_json::from_slice::<McpToolCallCommand>(&delivery.data) {
        Ok(command) => command.normalize_delivery_attempt(),
        Err(error) => {
            publish_payload(
                &channel,
                topology.rabbitmq_exchange.as_deref().unwrap_or_default(),
                topology
                    .dead_letter_queue_name
                    .as_deref()
                    .unwrap_or_default(),
                delivery.data.as_slice(),
            )
            .await
            .map_err(|publish_error| {
                format!("publish invalid MCP tool call command to DLQ failed: {publish_error}")
            })?;
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|ack_error| ack_error.to_string())?;
            return Err(format!("invalid MCP tool call command: {error}"));
        }
    };
    let result = crate::api::mcp::register_tool_call_command(&state, &command).await;
    drop(permit);
    match result {
        Ok(registered) => {
            if let Err(error) =
                publish_batch_pending_event(&state, &topology, &channel, &registered.record).await
            {
                delivery
                    .nack(BasicNackOptions {
                        multiple: false,
                        requeue: true,
                    })
                    .await
                    .map_err(|nack_error| nack_error.to_string())?;
                return Err(format!(
                    "publish MCP tool batch continuation failed: {error}"
                ));
            }
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|error| error.to_string())
        }
        Err(error) => {
            if let Some(retry) = command.next_retry(topology.max_delivery_attempts) {
                publish_command_to_queue(
                    &channel,
                    topology.rabbitmq_exchange.as_deref().unwrap_or_default(),
                    topology.retry_queue_name.as_deref().unwrap_or_default(),
                    &retry,
                )
                .await?;
                delivery
                    .ack(BasicAckOptions::default())
                    .await
                    .map_err(|ack_error| ack_error.to_string())
            } else {
                let result = exhausted_tool_call_result(&command, error.as_str());
                if let Err(publish_error) = publish_tool_call_result(
                    &channel,
                    "",
                    command.result_routing_key.as_str(),
                    &result,
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
                        "publish exhausted MCP tool call result failed: {publish_error}"
                    ));
                }
                delivery
                    .ack(BasicAckOptions::default())
                    .await
                    .map_err(|ack_error| ack_error.to_string())
            }
        }
    }
}

async fn publish_batch_pending_event(
    state: &AppState,
    topology: &AsyncToolDispatchTopology,
    channel: &Channel,
    batch: &crate::runtime::RuntimeToolBatchRecord,
) -> Result<(), String> {
    let Some(event) = batch.pending_event.clone() else {
        return Ok(());
    };
    match event.clone() {
        crate::runtime::RuntimeToolBatchPendingEvent::InvocationReady { call_index } => {
            let ready = InvocationReadyEvent {
                event_id: format!("mcp_invocation_ready_{}_{}", batch.batch_id, call_index),
                batch_id: batch.batch_id.clone(),
                call_index,
            };
            let payload = serde_json::to_vec(&ready).map_err(|error| error.to_string())?;
            publish_payload(
                channel,
                topology.rabbitmq_exchange.as_deref().unwrap_or_default(),
                invocation_queue_name(topology).as_str(),
                payload.as_slice(),
            )
            .await
            .map_err(|error| error.to_string())?;
        }
        crate::runtime::RuntimeToolBatchPendingEvent::AggregateResult => {
            let result = batch.aggregate_result().ok_or_else(|| {
                "completed Runtime Tool Batch has no aggregate result".to_string()
            })?;
            publish_tool_call_result(
                channel,
                "",
                batch.command.result_routing_key.as_str(),
                &result,
            )
            .await?;
        }
    }
    state
        .runtime_tool_batches
        .acknowledge_pending_event(batch.batch_id.as_str(), &event)
        .await
}

pub(super) async fn run_rabbitmq_invocation_consumer_loop(
    state: AppState,
    topology: AsyncToolDispatchTopology,
) {
    loop {
        match open_named_consumer(
            &topology,
            invocation_queue_name(&topology).as_str(),
            RABBITMQ_INVOCATION_CONSUMER_TAG,
        )
        .await
        {
            Ok((_connection, channel, mut consumer)) => {
                while let Some(delivery) = consumer.next().await {
                    let Ok(delivery) = delivery else { break };
                    let outcome = match serde_json::from_slice::<InvocationReadyEvent>(
                        &delivery.data,
                    ) {
                        Ok(event) => {
                            let outcome = crate::api::mcp::execute_tool_batch_invocation(
                                &state,
                                event.batch_id.as_str(),
                                event.call_index,
                            )
                            .await;
                            let _ = state
                                    .runtime_tool_batches
                                    .acknowledge_pending_event(
                                        event.batch_id.as_str(),
                                        &crate::runtime::RuntimeToolBatchPendingEvent::InvocationReady {
                                            call_index: event.call_index,
                                        },
                                    )
                                    .await;
                            outcome
                        }
                        Err(error) => Err(format!("invalid invocation-ready event: {error}")),
                    };
                    match outcome {
                        Ok(batch) => {
                            if let Err(error) =
                                publish_batch_pending_event(&state, &topology, &channel, &batch)
                                    .await
                            {
                                warn!(
                                    error = error.as_str(),
                                    "publish invocation continuation failed"
                                );
                                let _ = delivery
                                    .nack(BasicNackOptions {
                                        multiple: false,
                                        requeue: true,
                                    })
                                    .await;
                                continue;
                            }
                            let _ = delivery.ack(BasicAckOptions::default()).await;
                        }
                        Err(error) => {
                            warn!(
                                error = error.as_str(),
                                "execute invocation-ready event failed"
                            );
                            if invocation_ready_error_is_stale(error.as_str()) {
                                // The durable batch already expired or was removed. Requeueing
                                // cannot recreate it and only creates a hot loop that starves
                                // current runs, so consume the stale notification.
                                let _ = delivery.ack(BasicAckOptions::default()).await;
                            } else {
                                let _ = delivery
                                    .nack(BasicNackOptions {
                                        multiple: false,
                                        requeue: true,
                                    })
                                    .await;
                            }
                        }
                    }
                }
            }
            Err(error) => warn!(error = error.as_str(), "MCP invocation consumer failed"),
        }
        tokio::time::sleep(topology.rabbitmq_reconnect_delay).await;
    }
}

fn invocation_ready_error_is_stale(error: &str) -> bool {
    error == "Runtime Tool Batch was not found"
}

#[cfg(test)]
mod invocation_ready_tests {
    use super::invocation_ready_error_is_stale;

    #[test]
    fn missing_batch_is_consumed_instead_of_requeued() {
        assert!(invocation_ready_error_is_stale(
            "Runtime Tool Batch was not found"
        ));
        assert!(!invocation_ready_error_is_stale(
            "Runtime Tool Batch CAS conflict limit was exceeded"
        ));
    }
}

pub(super) async fn run_rabbitmq_terminal_consumer_loop(
    state: AppState,
    topology: AsyncToolDispatchTopology,
) {
    loop {
        match open_named_consumer(
            &topology,
            terminal_queue_name(&topology).as_str(),
            RABBITMQ_INVOCATION_TERMINAL_CONSUMER_TAG,
        )
        .await
        {
            Ok((_connection, channel, mut consumer)) => {
                if let Err(error) = reconcile_pending_batches(&state, &topology, &channel).await {
                    warn!(
                        error = error.as_str(),
                        "reconcile pending MCP batches failed"
                    );
                }
                while let Some(delivery) = consumer.next().await {
                    let Ok(delivery) = delivery else { break };
                    let outcome =
                        match serde_json::from_slice::<InvocationTerminalEvent>(&delivery.data) {
                            Ok(event) => {
                                if let Some(prompt_id) = event.prompt_id.as_deref() {
                                    crate::api::mcp::resolve_waiting_user_tool_invocation(
                                        &state, prompt_id,
                                    )
                                    .await
                                } else {
                                    crate::api::mcp::resume_terminal_tool_batch_invocation(
                                        &state,
                                        event.invocation_id.as_str(),
                                    )
                                    .await
                                }
                            }
                            Err(error) => {
                                Err(format!("invalid invocation-terminal event: {error}"))
                            }
                        };
                    match outcome {
                        Ok(Some(batch)) => {
                            if let Err(error) =
                                publish_batch_pending_event(&state, &topology, &channel, &batch)
                                    .await
                            {
                                warn!(
                                    error = error.as_str(),
                                    "publish terminal continuation failed"
                                );
                                let _ = delivery
                                    .nack(BasicNackOptions {
                                        multiple: false,
                                        requeue: true,
                                    })
                                    .await;
                                continue;
                            }
                            let _ = delivery.ack(BasicAckOptions::default()).await;
                        }
                        Ok(None) => {
                            let _ = delivery.ack(BasicAckOptions::default()).await;
                        }
                        Err(error) => {
                            warn!(
                                error = error.as_str(),
                                "reduce invocation-terminal event failed"
                            );
                            let _ = delivery
                                .nack(BasicNackOptions {
                                    multiple: false,
                                    requeue: true,
                                })
                                .await;
                        }
                    }
                }
            }
            Err(error) => warn!(
                error = error.as_str(),
                "MCP invocation terminal consumer failed"
            ),
        }
        tokio::time::sleep(topology.rabbitmq_reconnect_delay).await;
    }
}

async fn reconcile_pending_batches(
    state: &AppState,
    topology: &AsyncToolDispatchTopology,
    channel: &Channel,
) -> Result<(), String> {
    for batch in state.runtime_tool_batches.list_pending(1_000).await? {
        publish_batch_pending_event(state, topology, channel, &batch).await?;
    }
    Ok(())
}

async fn publish_command_to_queue(
    channel: &Channel,
    exchange: &str,
    queue_name: &str,
    command: &McpToolCallCommand,
) -> Result<(), String> {
    let payload = serde_json::to_vec(command).map_err(|error| error.to_string())?;
    publish_payload(channel, exchange, queue_name, payload.as_slice())
        .await
        .map_err(|error| error.to_string())
}

async fn publish_tool_call_result(
    channel: &Channel,
    exchange: &str,
    result_routing_key: &str,
    result: &McpToolCallResult,
) -> Result<(), String> {
    let payload = serde_json::to_vec(result).map_err(|error| error.to_string())?;
    let confirmation = channel
        .basic_publish(
            exchange,
            result_routing_key,
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload.as_slice(),
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_message_id(result.event_id.clone().into())
                .with_correlation_id(result.batch_id.clone().into()),
        )
        .await
        .map_err(|error| error.to_string())?
        .await
        .map_err(|error| error.to_string())?;
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!(
            "RabbitMQ returned unroutable MCP tool call result for {result_routing_key}"
        )),
        Confirmation::Nack(_) => Err("RabbitMQ rejected MCP tool call result".to_string()),
        Confirmation::NotRequested => {
            Err("RabbitMQ publisher confirm is not enabled for MCP tool call results".to_string())
        }
    }
}

fn exhausted_tool_call_result(command: &McpToolCallCommand, error: &str) -> McpToolCallResult {
    McpToolCallResult {
        event_id: format!("mcp_batch_result_{}", command.batch_id),
        owner_service: command.owner_service.clone(),
        agent_run_id: command.agent_run_id.clone(),
        agent_key: command.agent_key.clone(),
        ordering_lane_key: command.ordering_lane_key.clone(),
        lane_seq: command.lane_seq,
        generation: command.generation,
        source_step_seq: command.source_step_seq,
        batch_id: command.batch_id.clone(),
        session_id: command.mcp_runtime_session_ref.clone(),
        items: command
            .calls
            .iter()
            .map(|call| McpToolCallResultItem {
                invocation_id: call.invocation_id.clone(),
                tool_call_id: call.tool_call_id.clone(),
                call_index: call.call_index,
                name: call.name.clone(),
                status: McpToolCallResultStatus::Failed,
                result: None,
                error_code: Some(MCP_ERROR_INTERNAL),
                error: Some(format!(
                    "MCP tool call command failed after {} attempts: {error}",
                    command.delivery_attempt.max(1)
                )),
            })
            .collect(),
    }
}

pub(super) async fn run_cancellation_consumer_loop(
    state: AppState,
    topology: AsyncToolDispatchTopology,
) {
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

pub(super) fn unavailable_rabbitmq_queue_stats() -> RabbitMqQueueRuntimeStats {
    RabbitMqQueueRuntimeStats {
        enabled: true,
        available: false,
        queues: Vec::new(),
        error: Some("rabbitmq_queue_inspection_unavailable".to_string()),
    }
}

pub(super) async fn open_rabbitmq_publisher(
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
    let cancellation_exchange = topology.cancellation_exchange.clone().ok_or_else(|| {
        AsyncToolEnqueueError::Unavailable(
            "MCP_MANAGEMENT_INVOCATION_CANCELLATION_EXCHANGE is required for RabbitMQ dispatch"
                .to_string(),
        )
    })?;
    Ok(RabbitMqPublisher {
        _connection: connection,
        channel,
        cancellation_exchange,
    })
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

pub(super) fn ensure_publish_confirmed(
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

pub(super) async fn open_rabbitmq_consumer(
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
    for internal_queue in [
        invocation_queue_name(topology),
        terminal_queue_name(topology),
    ] {
        channel
            .queue_declare(
                internal_queue.as_str(),
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
                internal_queue.as_str(),
                exchange,
                internal_queue.as_str(),
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|error| error.to_string())?;
    }
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

async fn open_named_consumer(
    topology: &AsyncToolDispatchTopology,
    queue_name: &str,
    consumer_tag: &str,
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
            queue_name,
            consumer_tag,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok((connection, channel, consumer))
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

pub(super) fn dispatch_queue_arguments(topology: &AsyncToolDispatchTopology) -> FieldTable {
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
