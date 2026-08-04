// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::delivery::ChatosCallbackDeliveryError;
use super::*;
use crate::platform_queue::TaskQueueMode;
use futures_util::StreamExt;
use lapin::{
    message::Delivery,
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, ExchangeDeclareOptions,
        QueueBindOptions, QueueDeclareOptions,
    },
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};
use std::time::Duration;

const CALLBACK_QUEUE_CONSUMER_RETRY_DELAY: Duration = Duration::from_secs(3);
const CALLBACK_QUEUE_CONSUMER_TAG: &str = "task-runner-chatos-callbacks";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct QueuedChatosCallbackEnvelope {
    pub run_id: Option<String>,
    pub payload: ChatosTaskCallbackPayload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CallbackPublishOutcome {
    InlineDelivered,
    RabbitMqEnqueued,
}

pub(super) async fn publish_chatos_task_callback(
    config: AppConfig,
    task_queue_topology: &crate::platform_queue::TaskQueueTopology,
    payload: ChatosTaskCallbackPayload,
) -> Result<CallbackPublishOutcome, ChatosCallbackDeliveryError> {
    match task_queue_topology.callback_delivery_mode {
        TaskQueueMode::Inline => {
            super::delivery::send_chatos_task_callback(config, payload).await?;
            Ok(CallbackPublishOutcome::InlineDelivered)
        }
        TaskQueueMode::RabbitMq => {
            publish_callback_envelope(
                task_queue_topology,
                QueuedChatosCallbackEnvelope {
                    run_id: payload.run_id.clone(),
                    payload,
                },
            )
            .await?;
            Ok(CallbackPublishOutcome::RabbitMqEnqueued)
        }
    }
}

pub(super) fn spawn_chatos_callback_queue_consumer(
    config: AppConfig,
    task_queue_topology: crate::platform_queue::TaskQueueTopology,
    run_service: RunService,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!(
            queue = task_queue_topology.callback_delivery_queue.as_str(),
            exchange = task_queue_topology.rabbitmq_exchange.as_str(),
            "task callback queue consumer started"
        );
        loop {
            if let Err(err) = consume_callback_queue_once(
                config.clone(),
                task_queue_topology.clone(),
                run_service.clone(),
            )
            .await
            {
                warn!(
                    error = err.as_str(),
                    queue = task_queue_topology.callback_delivery_queue.as_str(),
                    "task callback queue consumer failed; reconnecting"
                );
                tokio::time::sleep(CALLBACK_QUEUE_CONSUMER_RETRY_DELAY).await;
            }
        }
    })
}

async fn publish_callback_envelope(
    task_queue_topology: &crate::platform_queue::TaskQueueTopology,
    envelope: QueuedChatosCallbackEnvelope,
) -> Result<(), ChatosCallbackDeliveryError> {
    let channel = open_callback_queue_channel(task_queue_topology).await?;
    let payload = serde_json::to_vec(&envelope).map_err(|err| {
        ChatosCallbackDeliveryError::permanent(format!(
            "serialize callback queue envelope failed: {err}"
        ))
    })?;
    channel
        .basic_publish(
            task_queue_topology.rabbitmq_exchange.as_str(),
            task_queue_topology.callback_delivery_queue.as_str(),
            BasicPublishOptions::default(),
            payload.as_slice(),
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2),
        )
        .await
        .map_err(|err| ChatosCallbackDeliveryError::retryable(err.to_string()))?
        .await
        .map_err(|err| ChatosCallbackDeliveryError::retryable(err.to_string()))?;
    Ok(())
}

async fn consume_callback_queue_once(
    config: AppConfig,
    task_queue_topology: crate::platform_queue::TaskQueueTopology,
    run_service: RunService,
) -> Result<(), String> {
    let channel = open_callback_queue_channel(&task_queue_topology)
        .await
        .map_err(|err| err.to_string())?;
    let mut consumer = channel
        .basic_consume(
            task_queue_topology.callback_delivery_queue.as_str(),
            CALLBACK_QUEUE_CONSUMER_TAG,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    while let Some(delivery) = consumer.next().await {
        match delivery {
            Ok(delivery) => {
                if let Err(err) =
                    handle_callback_delivery(config.clone(), run_service.clone(), delivery).await
                {
                    warn!(
                        error = err.as_str(),
                        "task callback queue delivery handler failed"
                    );
                }
            }
            Err(err) => return Err(err.to_string()),
        }
    }
    Err("task callback queue consumer stream closed".to_string())
}

async fn handle_callback_delivery(
    config: AppConfig,
    run_service: RunService,
    delivery: Delivery,
) -> Result<(), String> {
    let envelope = serde_json::from_slice::<QueuedChatosCallbackEnvelope>(&delivery.data)
        .map_err(|err| format!("decode callback queue envelope failed: {err}"))?;
    let queue_event = envelope.payload.event.clone();
    let queue_run_id = callback_event_tracks_delivery_state(queue_event.as_str())
        .then(|| envelope.run_id.clone())
        .flatten();
    let send_result = super::delivery::send_chatos_task_callback(config, envelope.payload).await;
    match send_result {
        Ok(()) => {
            if let Some(run_id) = queue_run_id.as_deref() {
                if let Err(err) = run_service
                    .record_callback_delivery_result(run_id, true, None)
                    .await
                {
                    warn!(
                        run_id,
                        event = queue_event.as_str(),
                        error = err.as_str(),
                        "callback queue delivery succeeded but run state update failed"
                    );
                }
            }
        }
        Err(err) => {
            let error_message = err.to_string();
            if let Some(run_id) = queue_run_id.as_deref() {
                if let Err(persist_err) = run_service
                    .record_callback_delivery_result(
                        run_id,
                        false,
                        Some((error_message.as_str(), err.is_retryable())),
                    )
                    .await
                {
                    warn!(
                        run_id,
                        event = queue_event.as_str(),
                        error = persist_err.as_str(),
                        "callback queue delivery failed and run state update also failed"
                    );
                }
            } else {
                warn!(
                    event = queue_event.as_str(),
                    retryable = err.is_retryable(),
                    error = error_message.as_str(),
                    "callback queue delivery failed for callback without run outbox state"
                );
            }
        }
    }
    delivery
        .ack(BasicAckOptions::default())
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn callback_event_tracks_delivery_state(event: &str) -> bool {
    matches!(
        event,
        "task.completed" | "task.failed" | "task.cancelled" | "task.blocked"
    )
}

async fn open_callback_queue_channel(
    task_queue_topology: &crate::platform_queue::TaskQueueTopology,
) -> Result<Channel, ChatosCallbackDeliveryError> {
    let rabbitmq_url = task_queue_topology.rabbitmq_url.as_deref().ok_or_else(|| {
        ChatosCallbackDeliveryError::permanent(
            "TASK_RUNNER_RABBITMQ_URL is required when callback delivery uses RabbitMQ",
        )
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|err| ChatosCallbackDeliveryError::retryable(err.to_string()))?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|err| ChatosCallbackDeliveryError::retryable(err.to_string()))?;
    ensure_callback_queue_topology(&channel, task_queue_topology).await?;
    Ok(channel)
}

#[cfg(test)]
mod tests {
    use super::callback_event_tracks_delivery_state;

    #[test]
    fn only_terminal_run_callbacks_track_delivery_state() {
        for event in [
            "task.completed",
            "task.failed",
            "task.cancelled",
            "task.blocked",
        ] {
            assert!(callback_event_tracks_delivery_state(event));
        }
        assert!(!callback_event_tracks_delivery_state("task.run.started"));
    }
}

async fn ensure_callback_queue_topology(
    channel: &Channel,
    task_queue_topology: &crate::platform_queue::TaskQueueTopology,
) -> Result<(), ChatosCallbackDeliveryError> {
    channel
        .exchange_declare(
            task_queue_topology.rabbitmq_exchange.as_str(),
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|err| ChatosCallbackDeliveryError::retryable(err.to_string()))?;
    channel
        .queue_declare(
            task_queue_topology.callback_delivery_queue.as_str(),
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|err| ChatosCallbackDeliveryError::retryable(err.to_string()))?;
    channel
        .queue_bind(
            task_queue_topology.callback_delivery_queue.as_str(),
            task_queue_topology.rabbitmq_exchange.as_str(),
            task_queue_topology.callback_delivery_queue.as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| ChatosCallbackDeliveryError::retryable(err.to_string()))?;
    Ok(())
}
