// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use lapin::{
    options::{BasicPublishOptions, ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions},
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};

use crate::models::TaskRunEventRecord;
use crate::platform_queue::TaskQueueTopology;

pub(crate) async fn publish_run_event_if_configured(
    event: &TaskRunEventRecord,
) -> Result<bool, String> {
    let task_queue_topology = TaskQueueTopology::from_managed_env()?;
    if !task_queue_topology.publishes_run_events_to_rabbitmq() {
        return Ok(false);
    }
    let rabbitmq_url = task_queue_topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "TASK_RUNNER_RABBITMQ_URL is required when run events use RabbitMQ".to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|err| err.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|err| err.to_string())?;
    ensure_run_event_topology(&channel, &task_queue_topology).await?;
    let payload = serde_json::to_vec(event).map_err(|err| err.to_string())?;
    channel
        .basic_publish(
            task_queue_topology.rabbitmq_exchange.as_str(),
            task_queue_topology.run_events_queue.as_str(),
            BasicPublishOptions::default(),
            payload.as_slice(),
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2),
        )
        .await
        .map_err(|err| err.to_string())?
        .await
        .map_err(|err| err.to_string())?;
    Ok(true)
}

async fn ensure_run_event_topology(
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
            task_queue_topology.run_events_queue.as_str(),
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
            task_queue_topology.run_events_queue.as_str(),
            task_queue_topology.rabbitmq_exchange.as_str(),
            task_queue_topology.run_events_queue.as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}
