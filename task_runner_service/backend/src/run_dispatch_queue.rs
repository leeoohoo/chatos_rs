// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use lapin::{
    options::{
        BasicPublishOptions, ConfirmSelectOptions, ExchangeDeclareOptions, QueueBindOptions,
        QueueDeclareOptions,
    },
    publisher_confirm::Confirmation,
    types::{AMQPValue, FieldTable},
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::models::now_rfc3339;
use crate::platform_queue::TaskQueueTopology;
use crate::services::RunService;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct QueuedRunDispatchEnvelope {
    pub(crate) run_id: String,
    pub(crate) queued_at: String,
}

pub(crate) async fn enqueue_run_dispatch(
    task_queue_topology: &TaskQueueTopology,
    run_id: &str,
) -> Result<(), String> {
    let rabbitmq_url = task_queue_topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "TASK_RUNNER_RABBITMQ_URL is required when run dispatch uses RabbitMQ".to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|err| err.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|err| err.to_string())?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|err| err.to_string())?;
    ensure_run_dispatch_topology(&channel, task_queue_topology).await?;
    let payload = serde_json::to_vec(&QueuedRunDispatchEnvelope {
        run_id: run_id.to_string(),
        queued_at: now_rfc3339(),
    })
    .map_err(|err| err.to_string())?;
    let confirmation = channel
        .basic_publish(
            task_queue_topology.rabbitmq_exchange.as_str(),
            task_queue_topology.run_dispatch_queue.as_str(),
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
        .map_err(|err| err.to_string())?
        .await
        .map_err(|err| err.to_string())?;
    ensure_run_dispatch_publish_confirmed(
        task_queue_topology.run_dispatch_queue.as_str(),
        confirmation,
    )
}

pub(crate) async fn ensure_run_dispatch_topology(
    channel: &Channel,
    task_queue_topology: &TaskQueueTopology,
) -> Result<(), String> {
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
        .map_err(|err| err.to_string())?;
    channel
        .queue_declare(
            task_queue_topology.run_dispatch_queue.as_str(),
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    channel
        .queue_bind(
            task_queue_topology.run_dispatch_queue.as_str(),
            task_queue_topology.rabbitmq_exchange.as_str(),
            task_queue_topology.run_dispatch_queue.as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    let retry_delay_ms = u32::try_from(task_queue_topology.run_dispatch_retry_delay.as_millis())
        .map_err(|_| {
            "Task Runner run dispatch retry delay is too large for RabbitMQ".to_string()
        })?;
    let mut retry_arguments = FieldTable::default();
    retry_arguments.insert("x-message-ttl".into(), AMQPValue::LongUInt(retry_delay_ms));
    retry_arguments.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(task_queue_topology.rabbitmq_exchange.clone().into()),
    );
    retry_arguments.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(task_queue_topology.run_dispatch_queue.clone().into()),
    );
    channel
        .queue_declare(
            task_queue_topology.run_dispatch_retry_queue.as_str(),
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            retry_arguments,
        )
        .await
        .map_err(|err| err.to_string())?;
    channel
        .queue_bind(
            task_queue_topology.run_dispatch_retry_queue.as_str(),
            task_queue_topology.rabbitmq_exchange.as_str(),
            task_queue_topology.run_dispatch_retry_queue.as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub(crate) async fn defer_run_dispatch(
    channel: &Channel,
    task_queue_topology: &TaskQueueTopology,
    payload: &[u8],
) -> Result<(), String> {
    let confirmation = channel
        .basic_publish(
            task_queue_topology.rabbitmq_exchange.as_str(),
            task_queue_topology.run_dispatch_retry_queue.as_str(),
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
        .map_err(|err| err.to_string())?
        .await
        .map_err(|err| err.to_string())?;
    ensure_run_dispatch_publish_confirmed(
        task_queue_topology.run_dispatch_retry_queue.as_str(),
        confirmation,
    )
}

fn ensure_run_dispatch_publish_confirmed(
    routing_key: &str,
    confirmation: Confirmation,
) -> Result<(), String> {
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!(
            "RabbitMQ returned unroutable Task Runner Run dispatch event for {routing_key}"
        )),
        Confirmation::Nack(_) => Err(format!(
            "RabbitMQ rejected Task Runner Run dispatch event for {routing_key}"
        )),
        Confirmation::NotRequested => Err(
            "RabbitMQ publisher confirm was not enabled for Task Runner Run dispatch event"
                .to_string(),
        ),
    }
}

pub fn spawn_run_dispatch_outbox_reconciler(
    task_queue_topology: TaskQueueTopology,
    run_service: RunService,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(task_queue_topology.run_dispatch_outbox_reconcile_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match run_service
                .publish_pending_run_dispatches(task_queue_topology.run_dispatch_outbox_batch_size)
                .await
            {
                Ok(count) if count > 0 => info!(
                    published_count = count,
                    "task runner reconciled pending run dispatch outbox events"
                ),
                Ok(_) => {}
                Err(err) => warn!(
                    error = err.as_str(),
                    "task runner failed to reconcile pending run dispatch outbox events"
                ),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_dispatch_outbox_requires_confirmed_routing() {
        assert!(ensure_run_dispatch_publish_confirmed(
            "task_runner.run.dispatch",
            Confirmation::Ack(None),
        )
        .is_ok());
        assert!(ensure_run_dispatch_publish_confirmed(
            "task_runner.run.dispatch",
            Confirmation::Nack(None),
        )
        .is_err());
        assert!(ensure_run_dispatch_publish_confirmed(
            "task_runner.run.dispatch",
            Confirmation::NotRequested,
        )
        .is_err());
    }
}
