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
use crate::runtime::RuntimeInvocationStatus;
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

mod recovery;

#[cfg(test)]
use recovery::{live_batch_watchdog_action, LiveBatchWatchdogAction};
use recovery::{
    reconcile_expired_invocations, reconcile_live_batches,
    resume_terminal_invocation_with_session_fallback,
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
    let semaphore = Arc::new(Semaphore::new(topology.worker_concurrency));
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
                    let Ok(delivery) = delivery else {
                        break;
                    };

                    let event = match serde_json::from_slice::<InvocationReadyEvent>(&delivery.data)
                    {
                        Ok(event) => event,
                        Err(error) => {
                            warn!(
                                error = error.to_string().as_str(),
                                "invalid invocation-ready event"
                            );
                            if delivery.ack(BasicAckOptions::default()).await.is_err() {
                                break;
                            }
                            continue;
                        }
                    };

                    // Invocation execution can include a local plugin call and therefore may
                    // legitimately take much longer than RabbitMQ's delivery-ack timeout. The
                    // durable Runtime Tool Batch remains the source of truth, so consume the
                    // broker notification before starting the slow work. A process crash is
                    // recovered by the batch watchdog, which restores InvocationReady for a
                    // still-queued invocation.
                    if let Err(error) = delivery.ack(BasicAckOptions::default()).await {
                        warn!(
                            error = error.to_string().as_str(),
                            "acknowledge invocation-ready event before execution failed"
                        );
                        break;
                    }

                    if let Err(error) = state
                        .runtime_tool_batches
                        .acknowledge_pending_event(
                            event.batch_id.as_str(),
                            &crate::runtime::RuntimeToolBatchPendingEvent::InvocationReady {
                                call_index: event.call_index,
                            },
                        )
                        .await
                    {
                        warn!(
                            batch_id = event.batch_id.as_str(),
                            error = error.as_str(),
                            "acknowledge durable invocation-ready event failed"
                        );
                    }

                    let permit = match semaphore.clone().acquire_owned().await {
                        Ok(permit) => permit,
                        Err(_) => break,
                    };
                    let state = state.clone();
                    let topology = topology.clone();
                    let channel = channel.clone();
                    tokio::spawn(async move {
                        let outcome = crate::api::mcp::execute_tool_batch_invocation(
                            &state,
                            event.batch_id.as_str(),
                            event.call_index,
                        )
                        .await;
                        drop(permit);

                        match outcome {
                            Ok(batch) => {
                                if let Err(error) =
                                    publish_batch_pending_event(&state, &topology, &channel, &batch)
                                        .await
                                {
                                    // The resulting pending event is still durable. The terminal
                                    // consumer watchdog republishes it on a healthy channel.
                                    warn!(
                                        error = error.as_str(),
                                        "publish invocation continuation failed; durable watchdog will retry"
                                    );
                                }
                            }
                            Err(error) => {
                                warn!(
                                    error = error.as_str(),
                                    "execute invocation-ready event failed"
                                );
                                if !invocation_ready_error_is_stale(error.as_str()) {
                                    // Execution failed before producing a new durable state.
                                    // Restore the ready marker so the watchdog can retry without
                                    // depending on a long-lived unacked RabbitMQ delivery.
                                    if let Err(requeue_error) = state
                                        .runtime_tool_batches
                                        .ensure_invocation_ready_for_event(
                                            event.batch_id.as_str(),
                                            event.call_index,
                                        )
                                        .await
                                    {
                                        warn!(
                                            error = requeue_error.as_str(),
                                            "restore invocation-ready event after execution failure failed"
                                        );
                                    }
                                }
                            }
                        }
                    });
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
    let mut startup_recovery_completed = false;
    loop {
        match open_named_consumer(
            &topology,
            terminal_queue_name(&topology).as_str(),
            RABBITMQ_INVOCATION_TERMINAL_CONSUMER_TAG,
        )
        .await
        {
            Ok((_connection, channel, mut consumer)) => {
                if !startup_recovery_completed {
                    match reconcile_orphan_invocations(&state).await {
                        Ok(()) => startup_recovery_completed = true,
                        Err(error) => warn!(
                            error = error.as_str(),
                            "reconcile orphan MCP invocations failed"
                        ),
                    }
                }
                if let Err(error) = reconcile_pending_batches(&state, &topology, &channel).await {
                    warn!(
                        error = error.as_str(),
                        "reconcile pending MCP batches failed"
                    );
                }
                let mut watchdog = tokio::time::interval(std::time::Duration::from_secs(5));
                watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                // Consume the interval's immediate first tick because connection-time
                // reconciliation has just completed above.
                watchdog.tick().await;
                loop {
                    let delivery = tokio::select! {
                        delivery = consumer.next() => delivery,
                        _ = watchdog.tick() => {
                            if let Err(error) = reconcile_expired_invocations(&state).await {
                                warn!(
                                    error = error.as_str(),
                                    "recover expired MCP invocations failed"
                                );
                            }
                            if let Err(error) = reconcile_live_batches(&state, &topology, &channel).await {
                                warn!(
                                    error = error.as_str(),
                                    "periodic MCP batch watchdog reconciliation failed"
                                );
                            }
                            continue;
                        }
                    };
                    let Some(delivery) = delivery else { break };
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
                                    resume_terminal_invocation_with_session_fallback(
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
                            if invocation_terminal_error_is_stale(error.as_str()) {
                                // Terminal notifications are at-least-once. Once run
                                // finalization has closed the session, a duplicate or
                                // delayed terminal event cannot produce new state. Ack it
                                // so stale history cannot hot-loop and starve active runs.
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
            Err(error) => warn!(
                error = error.as_str(),
                "MCP invocation terminal consumer failed"
            ),
        }
        tokio::time::sleep(topology.rabbitmq_reconnect_delay).await;
    }
}

async fn reconcile_orphan_invocations(state: &AppState) -> Result<(), String> {
    use crate::runtime::RuntimeInvocationStatus;

    let active_invocations = state.runtime_invocations.list_active(10_000).await?;
    let active_batches = state.runtime_tool_batches.list_active(1_000).await?;
    let batched_invocation_ids = active_batches
        .iter()
        .flat_map(|batch| batch.invocation_ids.iter().cloned())
        .collect::<std::collections::HashSet<_>>();

    // First close durable invocations that have lost their batch entirely. They
    // cannot be replayed safely and otherwise retain quota forever.
    for invocation in active_invocations
        .iter()
        .filter(|record| !batched_invocation_ids.contains(record.invocation_id.as_str()))
    {
        state
            .runtime_invocations
            .recover_after_restart(invocation, false)
            .await?;
    }

    // A batch is the ordering barrier. Recover only its current call and let
    // normal FIFO progression expose the next call; this keeps one task/run
    // serial even when a model emitted a multi-tool batch.
    for batch in active_batches {
        let Some(call) = batch.command.calls.get(batch.next_call_index) else {
            continue;
        };
        let Some(invocation) = state
            .runtime_invocations
            .get_for_recovery(
                call.invocation_id.as_str(),
                batch.command.owner_service.as_str(),
            )
            .await?
        else {
            // The durable batch remains authoritative. Republishing its current
            // ready event lets the normal reducer persist a structured missing-
            // invocation failure and advance the FIFO instead of stalling.
            state
                .runtime_tool_batches
                .ensure_invocation_ready_for(call.invocation_id.as_str())
                .await?;
            continue;
        };
        let session_exists = state
            .runtime_sessions
            .get(batch.session_id.as_str())
            .await?
            .is_some();
        if !session_exists {
            if is_recoverable_terminal_invocation_status(invocation.status)
                || state
                    .runtime_invocations
                    .close_registered_invocation(
                        invocation.invocation_id.as_str(),
                        invocation.session_id.as_str(),
                    )
                    .await?
            {
                crate::api::mcp::persist_terminal_tool_batch_invocation_without_session(
                    state,
                    invocation.invocation_id.as_str(),
                )
                .await?;
            }
            continue;
        }
        match invocation.status {
            RuntimeInvocationStatus::Queued => {
                state
                    .runtime_tool_batches
                    .ensure_invocation_ready_for(invocation.invocation_id.as_str())
                    .await?;
            }
            RuntimeInvocationStatus::Running | RuntimeInvocationStatus::CancelRequested => {
                if state
                    .runtime_invocations
                    .recover_after_restart(&invocation, true)
                    .await?
                {
                    crate::api::mcp::resume_terminal_tool_batch_invocation(
                        state,
                        invocation.invocation_id.as_str(),
                    )
                    .await?;
                }
            }
            RuntimeInvocationStatus::WaitingForUser => {}
            RuntimeInvocationStatus::Completed
            | RuntimeInvocationStatus::Failed
            | RuntimeInvocationStatus::Cancelled
            | RuntimeInvocationStatus::UnknownExecutionState => {
                crate::api::mcp::resume_terminal_tool_batch_invocation(
                    state,
                    invocation.invocation_id.as_str(),
                )
                .await?;
            }
        }
    }
    Ok(())
}

fn is_recoverable_terminal_invocation_status(status: RuntimeInvocationStatus) -> bool {
    matches!(
        status,
        RuntimeInvocationStatus::Completed
            | RuntimeInvocationStatus::Failed
            | RuntimeInvocationStatus::Cancelled
            | RuntimeInvocationStatus::UnknownExecutionState
    )
}

include!("rabbitmq_part01.rs");
