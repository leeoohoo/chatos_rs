// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use lapin::{
    options::{BasicPublishOptions, ConfirmSelectOptions},
    publisher_confirm::Confirmation,
    BasicProperties, Channel, Connection, ConnectionProperties,
};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::config::{AsyncToolDispatchMode, AsyncToolDispatchTopology};
use crate::state::AppState;

pub fn spawn_result_event_publisher(state: AppState) -> Option<JoinHandle<()>> {
    if state.config.async_tool_dispatch_topology.mode != AsyncToolDispatchMode::RabbitMq {
        return None;
    }
    Some(tokio::spawn(async move {
        run_result_event_publisher(state).await;
    }))
}

async fn run_result_event_publisher(state: AppState) {
    loop {
        match open_result_event_channel(&state.config.async_tool_dispatch_topology).await {
            Ok((connection, channel)) => {
                let _connection = connection;
                state
                    .async_tool_dispatch
                    .set_result_publisher_connected(true);
                info!("mcp invocation result event publisher connected to rabbitmq");
                if let Err(error) = publish_until_channel_failure(&state, &channel).await {
                    warn!(
                        error = error.as_str(),
                        "mcp invocation result event publisher channel failed"
                    );
                }
            }
            Err(error) => {
                warn!(
                    error = error.as_str(),
                    "mcp invocation result event publisher failed to connect to rabbitmq"
                );
            }
        }
        state
            .async_tool_dispatch
            .set_result_publisher_connected(false);
        tokio::time::sleep(
            state
                .config
                .async_tool_dispatch_topology
                .rabbitmq_reconnect_delay,
        )
        .await;
    }
}

async fn publish_until_channel_failure(state: &AppState, channel: &Channel) -> Result<(), String> {
    loop {
        let pending = state
            .runtime_invocations
            .pending_result_events(
                state
                    .config
                    .async_tool_dispatch_topology
                    .result_outbox_batch_size,
            )
            .await?;
        if pending.is_empty() {
            tokio::select! {
                _ = state.runtime_invocations.wait_for_result_event_signal() => {}
                _ = tokio::time::sleep(
                    state
                        .config
                        .async_tool_dispatch_topology
                        .result_outbox_reconcile_interval,
                ) => {}
            }
            continue;
        }
        for pending_event in pending {
            let payload = serde_json::to_vec(&pending_event.event).map_err(|error| {
                format!("serialize MCP invocation result event failed: {error}")
            })?;
            channel
                .basic_publish(
                    "",
                    pending_event.reply_to.as_str(),
                    BasicPublishOptions {
                        mandatory: true,
                        ..BasicPublishOptions::default()
                    },
                    payload.as_slice(),
                    BasicProperties::default()
                        .with_content_type("application/json".into())
                        .with_delivery_mode(2)
                        .with_message_id(pending_event.event.event_id.clone().into())
                        .with_correlation_id(pending_event.event.correlation_id.clone().into()),
                )
                .await
                .map_err(|error| error.to_string())?
                .await
                .map_err(|error| error.to_string())
                .and_then(|confirmation| {
                    ensure_result_event_publish_confirmed(
                        pending_event.reply_to.as_str(),
                        confirmation,
                    )
                })?;
            state
                .runtime_invocations
                .acknowledge_result_event(
                    pending_event.event.invocation_id.as_str(),
                    pending_event.event.event_id.as_str(),
                )
                .await?;
        }
    }
}

async fn open_result_event_channel(
    topology: &AsyncToolDispatchTopology,
) -> Result<(Connection, Channel), String> {
    let rabbitmq_url = topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL is required for result events".to_string()
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
    Ok((connection, channel))
}

fn ensure_result_event_publish_confirmed(
    reply_to: &str,
    confirmation: Confirmation,
) -> Result<(), String> {
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!(
            "RabbitMQ returned unroutable MCP result event for reply target {reply_to}"
        )),
        Confirmation::Nack(_) => Err(format!(
            "RabbitMQ rejected MCP result event for reply target {reply_to}"
        )),
        Confirmation::NotRequested => {
            Err("RabbitMQ publisher confirm was not enabled for MCP result event".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_event_outbox_is_not_acknowledged_without_broker_confirmation() {
        assert!(ensure_result_event_publish_confirmed(
            "task-runner.mcp.results.instance-1",
            Confirmation::Ack(None),
        )
        .is_ok());
        assert!(ensure_result_event_publish_confirmed(
            "task-runner.mcp.results.instance-1",
            Confirmation::Nack(None),
        )
        .is_err());
        assert!(ensure_result_event_publish_confirmed(
            "task-runner.mcp.results.instance-1",
            Confirmation::NotRequested,
        )
        .is_err());
    }
}
