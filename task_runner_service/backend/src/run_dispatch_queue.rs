// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use lapin::{
    options::{BasicPublishOptions, ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions},
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};
use serde::{Deserialize, Serialize};

use crate::models::now_rfc3339;
use crate::platform_queue::TaskQueueTopology;

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
    ensure_run_dispatch_topology(&channel, task_queue_topology).await?;
    let payload = serde_json::to_vec(&QueuedRunDispatchEnvelope {
        run_id: run_id.to_string(),
        queued_at: now_rfc3339(),
    })
    .map_err(|err| err.to_string())?;
    channel
        .basic_publish(
            task_queue_topology.rabbitmq_exchange.as_str(),
            task_queue_topology.run_dispatch_queue.as_str(),
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
    Ok(())
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
    Ok(())
}
