// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{Arc, OnceLock};

use futures_util::StreamExt;
use lapin::{
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
        BasicQosOptions, ConfirmSelectOptions, ExchangeDeclareOptions, QueueBindOptions,
        QueueDeclareOptions,
    },
    publisher_confirm::Confirmation,
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{broadcast, Mutex},
    task::JoinHandle,
};
use tracing::{info, warn};

use crate::models::TaskRunEventRecord;
use crate::platform_queue::{TaskQueueMode, TaskQueueTopology};
use crate::services::RunService;

const RUN_EVENT_CONSUMER_PREFETCH: u16 = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunEventNotification {
    event_id: String,
    run_id: String,
    created_at: String,
}

impl From<&TaskRunEventRecord> for RunEventNotification {
    fn from(event: &TaskRunEventRecord) -> Self {
        Self {
            event_id: event.id.clone(),
            run_id: event.run_id.clone(),
            created_at: event.created_at.clone(),
        }
    }
}

struct RunEventBus {
    topology: TaskQueueTopology,
    publisher: Mutex<Option<Arc<RabbitMqPublisher>>>,
    resync_sender: broadcast::Sender<()>,
}

struct RabbitMqPublisher {
    _connection: Connection,
    channel: Channel,
}

static RUN_EVENT_BUS: OnceLock<RunEventBus> = OnceLock::new();

pub(crate) fn initialize_run_event_bus(
    topology: TaskQueueTopology,
    resync_sender: broadcast::Sender<()>,
) -> Result<(), String> {
    if topology.run_events_publish_mode != TaskQueueMode::RabbitMq {
        return Err("Task Runner Run events require RabbitMQ publish mode".to_string());
    }
    if topology.rabbitmq_url.is_none() {
        return Err("Task Runner Run events require the managed RabbitMQ URL".to_string());
    }
    RUN_EVENT_BUS
        .set(RunEventBus {
            topology,
            publisher: Mutex::new(None),
            resync_sender,
        })
        .map_err(|_| "Task Runner Run event bus is already initialized".to_string())
}

pub(crate) async fn publish_run_event(event: &TaskRunEventRecord) -> Result<(), String> {
    let bus = run_event_bus()?;
    let publisher = match rabbitmq_publisher(bus).await {
        Ok(publisher) => publisher,
        Err(err) => {
            let _ = bus.resync_sender.send(());
            return Err(err);
        }
    };
    let payload =
        serde_json::to_vec(&RunEventNotification::from(event)).map_err(|err| err.to_string())?;
    let publish_result = async {
        let confirmation = publisher
            .channel
            .basic_publish(
                bus.topology.rabbitmq_exchange.as_str(),
                bus.topology.run_events_routing_key.as_str(),
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
        ensure_run_event_publish_confirmed(
            bus.topology.run_events_routing_key.as_str(),
            confirmation,
        )
    }
    .await;
    if publish_result.is_err() {
        *bus.publisher.lock().await = None;
        let _ = bus.resync_sender.send(());
    }
    publish_result
}

pub fn spawn_run_event_consumer(
    instance_id: String,
    topology: TaskQueueTopology,
    run_service: RunService,
    resync_sender: broadcast::Sender<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match open_run_event_consumer(&instance_id, &topology).await {
                Ok((connection, queue_name, mut consumer)) => {
                    let _connection = connection;
                    run_service
                        .runtime_stats()
                        .set_run_event_consumer_connected(true);
                    let _ = resync_sender.send(());
                    info!(
                        instance_id = instance_id.as_str(),
                        queue = queue_name.as_str(),
                        routing_key = topology.run_events_routing_key.as_str(),
                        "task runner Run event consumer connected to rabbitmq"
                    );
                    while let Some(delivery) = consumer.next().await {
                        match delivery {
                            Ok(delivery) => {
                                let notification = match serde_json::from_slice::<
                                    RunEventNotification,
                                >(
                                    &delivery.data
                                ) {
                                    Ok(notification) => notification,
                                    Err(err) => {
                                        warn!(
                                            instance_id = instance_id.as_str(),
                                            error = err.to_string().as_str(),
                                            "task runner ignored invalid Run event notification"
                                        );
                                        if let Err(err) =
                                            delivery.ack(BasicAckOptions::default()).await
                                        {
                                            warn!(
                                                instance_id = instance_id.as_str(),
                                                error = err.to_string().as_str(),
                                                "task runner failed to acknowledge invalid Run event notification"
                                            );
                                            break;
                                        }
                                        continue;
                                    }
                                };
                                match run_service
                                    .get_run_event(
                                        notification.run_id.as_str(),
                                        notification.event_id.as_str(),
                                    )
                                    .await
                                {
                                    Ok(Some(event)) => {
                                        run_service.runtime_stats().record_run_event_consumed();
                                        run_service.broadcast_run_event(event);
                                    }
                                    Ok(None) => warn!(
                                        instance_id = instance_id.as_str(),
                                        run_id = notification.run_id.as_str(),
                                        event_id = notification.event_id.as_str(),
                                        "task runner Run event notification referenced a missing persisted event"
                                    ),
                                    Err(err) => {
                                        warn!(
                                            instance_id = instance_id.as_str(),
                                            run_id = notification.run_id.as_str(),
                                            event_id = notification.event_id.as_str(),
                                            error = err.as_str(),
                                            "task runner failed to load persisted Run event; requeueing notification"
                                        );
                                        if let Err(nack_err) = delivery
                                            .nack(BasicNackOptions {
                                                requeue: true,
                                                ..BasicNackOptions::default()
                                            })
                                            .await
                                        {
                                            warn!(
                                                instance_id = instance_id.as_str(),
                                                error = nack_err.to_string().as_str(),
                                                "task runner failed to requeue Run event notification"
                                            );
                                        }
                                        break;
                                    }
                                }
                                if let Err(err) = delivery.ack(BasicAckOptions::default()).await {
                                    warn!(
                                        instance_id = instance_id.as_str(),
                                        error = err.to_string().as_str(),
                                        "task runner failed to acknowledge Run event"
                                    );
                                    break;
                                }
                            }
                            Err(err) => {
                                warn!(
                                    instance_id = instance_id.as_str(),
                                    error = err.to_string().as_str(),
                                    "task runner Run event consumer delivery failed"
                                );
                                break;
                            }
                        }
                    }
                }
                Err(err) => warn!(
                    instance_id = instance_id.as_str(),
                    error = err.as_str(),
                    "task runner Run event consumer failed to connect to rabbitmq"
                ),
            }
            run_service
                .runtime_stats()
                .set_run_event_consumer_connected(false);
            run_service
                .runtime_stats()
                .record_run_event_consumer_reconnect();
            run_service
                .runtime_stats()
                .record_rabbitmq_consumer_reconnect();
            tokio::time::sleep(topology.rabbitmq_reconnect_delay).await;
        }
    })
}

fn run_event_bus() -> Result<&'static RunEventBus, String> {
    RUN_EVENT_BUS
        .get()
        .ok_or_else(|| "Task Runner Run event bus is not initialized".to_string())
}

async fn rabbitmq_publisher(bus: &RunEventBus) -> Result<Arc<RabbitMqPublisher>, String> {
    let mut guard = bus.publisher.lock().await;
    if let Some(publisher) = guard.as_ref() {
        return Ok(Arc::clone(publisher));
    }
    let rabbitmq_url = bus
        .topology
        .rabbitmq_url
        .as_deref()
        .ok_or_else(|| "Task Runner Run events require the managed RabbitMQ URL".to_string())?;
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
    ensure_run_event_exchange(&channel, &bus.topology).await?;
    let publisher = Arc::new(RabbitMqPublisher {
        _connection: connection,
        channel,
    });
    *guard = Some(Arc::clone(&publisher));
    Ok(publisher)
}

async fn open_run_event_consumer(
    instance_id: &str,
    topology: &TaskQueueTopology,
) -> Result<(Connection, String, lapin::Consumer), String> {
    let rabbitmq_url = topology
        .rabbitmq_url
        .as_deref()
        .ok_or_else(|| "Task Runner Run events require the managed RabbitMQ URL".to_string())?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|err| err.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|err| err.to_string())?;
    ensure_run_event_exchange(&channel, topology).await?;
    let queue = channel
        .queue_declare(
            "",
            QueueDeclareOptions {
                durable: false,
                exclusive: true,
                auto_delete: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    let queue_name = queue.name().as_str().to_string();
    channel
        .queue_bind(
            queue_name.as_str(),
            topology.rabbitmq_exchange.as_str(),
            topology.run_events_routing_key.as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    channel
        .basic_qos(RUN_EVENT_CONSUMER_PREFETCH, BasicQosOptions::default())
        .await
        .map_err(|err| err.to_string())?;
    let consumer = channel
        .basic_consume(
            queue_name.as_str(),
            format!("task-runner-run-events-{instance_id}").as_str(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok((connection, queue_name, consumer))
}

async fn ensure_run_event_exchange(
    channel: &Channel,
    topology: &TaskQueueTopology,
) -> Result<(), String> {
    channel
        .exchange_declare(
            topology.rabbitmq_exchange.as_str(),
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn ensure_run_event_publish_confirmed(
    routing_key: &str,
    confirmation: Confirmation,
) -> Result<(), String> {
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!(
            "RabbitMQ returned unroutable Task Runner Run event for {routing_key}"
        )),
        Confirmation::Nack(_) => Err(format!(
            "RabbitMQ rejected Task Runner Run event for {routing_key}"
        )),
        Confirmation::NotRequested => {
            Err("RabbitMQ publisher confirm was not enabled for Task Runner Run event".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_event_publish_requires_confirmed_routing() {
        assert!(ensure_run_event_publish_confirmed(
            "task_runner.run.events.broadcast",
            Confirmation::Ack(None),
        )
        .is_ok());
        assert!(ensure_run_event_publish_confirmed(
            "task_runner.run.events.broadcast",
            Confirmation::Nack(None),
        )
        .is_err());
        assert!(ensure_run_event_publish_confirmed(
            "task_runner.run.events.broadcast",
            Confirmation::NotRequested,
        )
        .is_err());
    }

    #[test]
    fn run_event_notification_excludes_large_event_content() {
        let event = TaskRunEventRecord {
            id: "event-1".to_string(),
            run_id: "run-1".to_string(),
            event_type: "tool.output".to_string(),
            message: Some("large message".repeat(1_000)),
            payload: Some(serde_json::json!({ "content": "large payload".repeat(1_000) })),
            created_at: "2026-08-05T12:00:00Z".to_string(),
        };

        let value = serde_json::to_value(RunEventNotification::from(&event))
            .expect("serialize notification");

        assert_eq!(value["event_id"], "event-1");
        assert_eq!(value["run_id"], "run-1");
        assert!(value.get("message").is_none());
        assert!(value.get("payload").is_none());
        assert!(value.get("event_type").is_none());
    }
}
