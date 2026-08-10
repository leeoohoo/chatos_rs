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
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::{info, warn};

use crate::config::AsyncToolDispatchTopology;
use crate::state::AppState;

use super::{
    fail_async_invocation, AsyncToolEnqueueError, InvocationCancellationEvent, ProcessOutcome,
    QueuedAsyncToolCallEnvelope, INITIAL_DELIVERY_ATTEMPT, RABBITMQ_CANCELLATION_CONSUMER_TAG,
    RABBITMQ_CONSUMER_TAG,
};

pub(super) struct RabbitMqPublisher {
    pub(super) _connection: Connection,
    pub(super) channel: Channel,
    pub(super) exchange: String,
    pub(super) queue_name: String,
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
    let outcome = super::process_envelope(state.clone(), &envelope).await;
    drop(permit);
    settle_rabbitmq_delivery(&state, &topology, &channel, delivery, envelope, outcome).await
}

pub(super) async fn settle_rabbitmq_delivery(
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
                channel,
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

pub(super) async fn publish_envelope_to_queue(
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
