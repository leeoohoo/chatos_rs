// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use chatos_mcp_service::{McpToolCallCommand, McpToolCallResult};
use futures_util::StreamExt;
use lapin::{
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, BasicQosOptions,
        ConfirmSelectOptions, QueueDeclareOptions,
    },
    publisher_confirm::Confirmation,
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties,
};
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
    _publisher_connection: Connection,
    publisher_channel: Channel,
    waiters: Arc<Mutex<HashMap<String, oneshot::Sender<McpToolCallResult>>>>,
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
    let (publisher_connection, publisher_channel) = open_publisher(&config).await?;
    let waiters = Arc::new(Mutex::new(HashMap::new()));
    RESULT_BUS
        .set(ResultBus {
            config: config.clone(),
            _publisher_connection: publisher_connection,
            publisher_channel,
            waiters: Arc::clone(&waiters),
        })
        .map_err(|_| "MCP invocation result queue is already initialized".to_string())?;
    tokio::spawn(run_consumer(config, waiters, Some((connection, consumer))));
    Ok(())
}

pub(crate) fn prepare_result_waiter(batch_id: String) -> Result<McpToolCallResultWaiter, String> {
    let bus = RESULT_BUS
        .get()
        .ok_or_else(|| "MCP invocation result queue is not initialized".to_string())?;
    let (sender, receiver) = oneshot::channel();
    let mut waiters = bus
        .waiters
        .lock()
        .map_err(|_| "MCP invocation result waiter registry is poisoned".to_string())?;
    if waiters.insert(batch_id.clone(), sender).is_some() {
        return Err("MCP tool call batch id is already active".to_string());
    }
    Ok(McpToolCallResultWaiter {
        batch_id,
        reply_to: bus.config.queue_name.clone(),
        receiver: Some(receiver),
        waiters: Arc::clone(&bus.waiters),
    })
}

pub(crate) struct McpToolCallResultWaiter {
    batch_id: String,
    reply_to: String,
    receiver: Option<oneshot::Receiver<McpToolCallResult>>,
    waiters: Arc<Mutex<HashMap<String, oneshot::Sender<McpToolCallResult>>>>,
}

impl McpToolCallResultWaiter {
    pub(crate) fn reply_to(&self) -> &str {
        self.reply_to.as_str()
    }

    pub(crate) async fn wait(mut self, timeout: Duration) -> Result<McpToolCallResult, String> {
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
        Ok(event)
    }
}

impl Drop for McpToolCallResultWaiter {
    fn drop(&mut self) {
        if let Ok(mut waiters) = self.waiters.lock() {
            waiters.remove(self.batch_id.as_str());
        }
    }
}

pub(crate) async fn publish_tool_call_command(
    command_queue: &str,
    command: &McpToolCallCommand,
) -> Result<(), String> {
    let bus = RESULT_BUS
        .get()
        .ok_or_else(|| "MCP tool call queue is not initialized".to_string())?;
    let payload = serde_json::to_vec(command).map_err(|error| error.to_string())?;
    let confirmation = bus
        .publisher_channel
        .basic_publish(
            "",
            command_queue,
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload.as_slice(),
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_message_id(command.batch_id.clone().into())
                .with_correlation_id(command.batch_id.clone().into())
                .with_reply_to(command.reply_to.clone().into()),
        )
        .await
        .map_err(|error| error.to_string())?
        .await
        .map_err(|error| error.to_string())?;
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!(
            "RabbitMQ returned unroutable MCP tool call command for {command_queue}"
        )),
        Confirmation::Nack(_) => Err("RabbitMQ rejected MCP tool call command".to_string()),
        Confirmation::NotRequested => {
            Err("RabbitMQ publisher confirm is not enabled for MCP tool calls".to_string())
        }
    }
}

async fn run_consumer(
    config: McpInvocationResultQueueConfig,
    waiters: Arc<Mutex<HashMap<String, oneshot::Sender<McpToolCallResult>>>>,
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
                                    let event = serde_json::from_slice::<McpToolCallResult>(
                                        delivery.data.as_slice(),
                                    );
                                    match event {
                                        Ok(event) => {
                                            let sender =
                                                waiters.lock().ok().and_then(|mut waiters| {
                                                    waiters.remove(event.batch_id.as_str())
                                                });
                                            if let Some(sender) = sender {
                                                let _ = sender.send(event);
                                            } else {
                                                warn!(
                                                    batch_id = event.batch_id.as_str(),
                                                    "MCP tool call result has no active waiter"
                                                );
                                            }
                                        }
                                        Err(error) => warn!(
                                            error = error.to_string().as_str(),
                                            "invalid MCP tool call result"
                                        ),
                                    }
                                    if let Err(error) =
                                        delivery.ack(BasicAckOptions::default()).await
                                    {
                                        warn!(
                                            error = error.to_string().as_str(),
                                            "acknowledge MCP tool call result failed"
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

async fn open_publisher(
    config: &McpInvocationResultQueueConfig,
) -> Result<(Connection, Channel), String> {
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
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    Ok((connection, channel))
}

#[cfg(test)]
mod tests {
    use chatos_mcp_service::McpToolCallResultStatus;

    #[test]
    fn tool_call_result_status_is_wire_stable() {
        assert_eq!(
            serde_json::to_string(&McpToolCallResultStatus::UnknownExecutionState).unwrap(),
            "\"unknown_execution_state\""
        );
    }
}
