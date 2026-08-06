// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use chatos_mcp_management_sdk::{RuntimeInvocationResultEvent, RuntimeInvocationStatus};
use futures_util::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicConsumeOptions, BasicQosOptions, QueueDeclareOptions},
    types::FieldTable,
    Connection, ConnectionProperties,
};
use serde_json::Value;
use tokio::sync::oneshot;
use tracing::{info, warn};

const RESULT_CONSUMER_RECONNECT_DELAY: Duration = Duration::from_secs(3);
const RESULT_CONSUMER_TAG: &str = "chatos-mcp-invocation-results";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpInvocationResultQueueConfig {
    pub rabbitmq_url: String,
    pub queue_name: String,
}

impl McpInvocationResultQueueConfig {
    fn validate(&self) -> Result<(), String> {
        for (label, value) in [
            ("rabbitmq_url", self.rabbitmq_url.as_str()),
            ("queue_name", self.queue_name.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!(
                    "MCP invocation result queue {label} must not be empty"
                ));
            }
        }
        Ok(())
    }
}

struct ResultBus {
    config: McpInvocationResultQueueConfig,
    waiters: Arc<Mutex<HashMap<String, oneshot::Sender<RuntimeInvocationResultEvent>>>>,
}

static RESULT_BUS: OnceLock<ResultBus> = OnceLock::new();

pub async fn initialize_mcp_invocation_result_queue(
    config: McpInvocationResultQueueConfig,
) -> Result<(), String> {
    config.validate()?;
    if RESULT_BUS.get().is_some() {
        return Err("MCP invocation result queue is already initialized".to_string());
    }
    let (connection, consumer) = open_consumer(&config).await?;
    let waiters = Arc::new(Mutex::new(HashMap::new()));
    RESULT_BUS
        .set(ResultBus {
            config: config.clone(),
            waiters: Arc::clone(&waiters),
        })
        .map_err(|_| "MCP invocation result queue is already initialized".to_string())?;
    tokio::spawn(run_consumer(config, waiters, Some((connection, consumer))));
    Ok(())
}

pub(crate) fn prepare_result_waiter(
    correlation_id: String,
) -> Result<McpInvocationResultWaiter, String> {
    let bus = RESULT_BUS
        .get()
        .ok_or_else(|| "MCP invocation result queue is not initialized".to_string())?;
    let (sender, receiver) = oneshot::channel();
    let mut waiters = bus
        .waiters
        .lock()
        .map_err(|_| "MCP invocation result waiter registry is poisoned".to_string())?;
    if waiters.insert(correlation_id.clone(), sender).is_some() {
        return Err("MCP invocation result correlation id is already active".to_string());
    }
    Ok(McpInvocationResultWaiter {
        correlation_id,
        reply_to: bus.config.queue_name.clone(),
        receiver: Some(receiver),
        waiters: Arc::clone(&bus.waiters),
    })
}

pub(crate) struct McpInvocationResultWaiter {
    correlation_id: String,
    reply_to: String,
    receiver: Option<oneshot::Receiver<RuntimeInvocationResultEvent>>,
    waiters: Arc<Mutex<HashMap<String, oneshot::Sender<RuntimeInvocationResultEvent>>>>,
}

impl McpInvocationResultWaiter {
    pub(crate) fn reply_to(&self) -> &str {
        self.reply_to.as_str()
    }

    pub(crate) async fn wait(mut self, timeout: Duration) -> Result<Value, String> {
        let receiver = self
            .receiver
            .take()
            .ok_or_else(|| "MCP invocation result waiter was already consumed".to_string())?;
        let event = tokio::time::timeout(timeout, receiver)
            .await
            .map_err(|_| {
                format!(
                    "MCP invocation result event exceeded timeout={}s",
                    timeout.as_secs()
                )
            })?
            .map_err(|_| "MCP invocation result consumer stopped before delivery".to_string())?;
        terminal_event_result(event)
    }
}

impl Drop for McpInvocationResultWaiter {
    fn drop(&mut self) {
        if let Ok(mut waiters) = self.waiters.lock() {
            waiters.remove(self.correlation_id.as_str());
        }
    }
}

fn terminal_event_result(event: RuntimeInvocationResultEvent) -> Result<Value, String> {
    match event.status {
        RuntimeInvocationStatus::Completed => event.terminal_result.ok_or_else(|| {
            format!(
                "accepted MCP invocation {} completed without a terminal result",
                event.invocation_id
            )
        }),
        RuntimeInvocationStatus::Failed => Err(format_terminal_error("failed", &event)),
        RuntimeInvocationStatus::Cancelled => Err(format_terminal_error("cancelled", &event)),
        RuntimeInvocationStatus::UnknownExecutionState => {
            Err(format_terminal_error("unknown_execution_state", &event))
        }
        status => Err(format!(
            "MCP invocation result event {} has non-terminal status {status:?}",
            event.event_id
        )),
    }
}

fn format_terminal_error(outcome: &str, event: &RuntimeInvocationResultEvent) -> String {
    let mut message = format!(
        "accepted MCP invocation {} ended with status {outcome}",
        event.invocation_id
    );
    if let Some(code) = event.terminal_error_code {
        message.push_str(format!(" (code={code})").as_str());
    }
    if let Some(detail) = event
        .terminal_error_message
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        message.push_str(format!(": {detail}").as_str());
    }
    message
}

async fn run_consumer(
    config: McpInvocationResultQueueConfig,
    waiters: Arc<Mutex<HashMap<String, oneshot::Sender<RuntimeInvocationResultEvent>>>>,
    mut initial: Option<(Connection, lapin::Consumer)>,
) {
    loop {
        let opened = match initial.take() {
            Some(opened) => Ok(opened),
            None => open_consumer(&config).await,
        };
        match opened {
            Ok((connection, consumer)) => {
                let _connection = connection;
                info!(
                    queue = config.queue_name.as_str(),
                    "MCP invocation result consumer connected to rabbitmq"
                );
                consumer
                    .for_each_concurrent(1, |delivery| {
                        let waiters = Arc::clone(&waiters);
                        async move {
                            match delivery {
                                Ok(delivery) => {
                                    let event = serde_json::from_slice::<RuntimeInvocationResultEvent>(
                                        delivery.data.as_slice(),
                                    );
                                    match event {
                                        Ok(event) => {
                                            let sender = waiters
                                                .lock()
                                                .ok()
                                                .and_then(|mut waiters| {
                                                    waiters.remove(event.correlation_id.as_str())
                                                });
                                            if let Some(sender) = sender {
                                                let _ = sender.send(event);
                                            } else {
                                                warn!(
                                                    correlation_id = event.correlation_id.as_str(),
                                                    invocation_id = event.invocation_id.as_str(),
                                                    "MCP invocation result event has no active waiter"
                                                );
                                            }
                                        }
                                        Err(error) => warn!(
                                            error = error.to_string().as_str(),
                                            "invalid MCP invocation result event"
                                        ),
                                    }
                                    if let Err(error) = delivery.ack(BasicAckOptions::default()).await {
                                        warn!(
                                            error = error.to_string().as_str(),
                                            "acknowledge MCP invocation result event failed"
                                        );
                                    }
                                }
                                Err(error) => warn!(
                                    error = error.to_string().as_str(),
                                    "MCP invocation result consumer delivery failed"
                                ),
                            }
                        }
                    })
                    .await;
            }
            Err(error) => warn!(
                error = error.as_str(),
                "MCP invocation result consumer failed to connect to rabbitmq"
            ),
        }
        tokio::time::sleep(RESULT_CONSUMER_RECONNECT_DELAY).await;
    }
}

async fn open_consumer(
    config: &McpInvocationResultQueueConfig,
) -> Result<(Connection, lapin::Consumer), String> {
    let connection = Connection::connect(
        config.rabbitmq_url.as_str(),
        ConnectionProperties::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_declare(
            config.queue_name.as_str(),
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .basic_qos(1, BasicQosOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    let consumer = channel
        .basic_consume(
            config.queue_name.as_str(),
            RESULT_CONSUMER_TAG,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok((connection, consumer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_result_event_preserves_original_tool_result() {
        let result = terminal_event_result(RuntimeInvocationResultEvent {
            event_id: "event-1".to_string(),
            correlation_id: "request-1".to_string(),
            invocation_id: "invocation-1".to_string(),
            session_id: "session-1".to_string(),
            caller_service: "task-runner".to_string(),
            resource_id: "resource-1".to_string(),
            exposed_tool_name: "demo".to_string(),
            status: RuntimeInvocationStatus::Completed,
            occurred_at_unix_ms: 1,
            terminal_result: Some(
                serde_json::json!({"content": [{"type": "text", "text": "done"}]}),
            ),
            terminal_error_code: None,
            terminal_error_message: None,
        })
        .unwrap();
        assert_eq!(result["content"][0]["text"], "done");
    }
}
