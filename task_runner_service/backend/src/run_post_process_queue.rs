// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::StreamExt;
use lapin::{
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicGetOptions, BasicNackOptions,
        BasicPublishOptions, BasicQosOptions, ConfirmSelectOptions, ExchangeDeclareOptions,
        QueueBindOptions, QueueDeclareOptions,
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

const RUN_POST_PROCESS_CONSUMER_TAG: &str = "task-runner-run-post-process";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RunPostProcessEnvelope {
    pub(crate) run_id: String,
    pub(crate) requested_at: String,
}

pub(crate) async fn enqueue_run_post_process(
    topology: &TaskQueueTopology,
    run_id: &str,
) -> Result<(), String> {
    let rabbitmq_url = topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "TASK_RUNNER_RABBITMQ_URL is required for Run post-processing".to_string()
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
    ensure_run_post_process_topology(&channel, topology).await?;
    let envelope = RunPostProcessEnvelope {
        run_id: run_id.to_string(),
        requested_at: now_rfc3339(),
    };
    let payload = serde_json::to_vec(&envelope).map_err(|err| err.to_string())?;
    publish_envelope(
        &channel,
        topology.rabbitmq_exchange.as_str(),
        topology.run_post_process_queue.as_str(),
        payload.as_slice(),
        format!("run-post-process:{run_id}"),
    )
    .await
}

pub(crate) async fn archive_run_post_process_dead_letter(
    topology: &TaskQueueTopology,
    run_id: &str,
    scan_limit: usize,
) -> Result<bool, String> {
    let rabbitmq_url = topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "TASK_RUNNER_RABBITMQ_URL is required for Run post-process DLQ archival".to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|err| err.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|err| err.to_string())?;
    ensure_run_post_process_topology(&channel, topology).await?;
    let mut unmatched = Vec::new();
    let mut matched = None;
    for _ in 0..scan_limit.clamp(1, 1_000) {
        let Some(delivery) = channel
            .basic_get(
                topology.run_post_process_dead_letter_queue.as_str(),
                BasicGetOptions::default(),
            )
            .await
            .map_err(|err| err.to_string())?
        else {
            break;
        };
        let is_match = serde_json::from_slice::<RunPostProcessEnvelope>(&delivery.data)
            .is_ok_and(|envelope| envelope.run_id == run_id);
        if is_match {
            matched = Some(delivery);
            break;
        }
        unmatched.push(delivery);
    }
    let archived = matched.is_some();
    if let Some(delivery) = matched {
        delivery
            .ack(BasicAckOptions::default())
            .await
            .map_err(|err| err.to_string())?;
    }
    for delivery in unmatched {
        delivery
            .nack(BasicNackOptions {
                multiple: false,
                requeue: true,
            })
            .await
            .map_err(|err| err.to_string())?;
    }
    Ok(archived)
}

async fn defer_run_post_process(
    channel: &Channel,
    topology: &TaskQueueTopology,
    payload: &[u8],
    run_id: &str,
) -> Result<(), String> {
    publish_envelope(
        channel,
        topology.rabbitmq_exchange.as_str(),
        topology.run_post_process_retry_queue.as_str(),
        payload,
        format!("run-post-process-retry:{run_id}:{}", now_rfc3339()),
    )
    .await
}

async fn dead_letter_run_post_process(
    channel: &Channel,
    topology: &TaskQueueTopology,
    payload: &[u8],
    run_id: &str,
) -> Result<(), String> {
    publish_envelope(
        channel,
        topology.rabbitmq_exchange.as_str(),
        topology.run_post_process_dead_letter_queue.as_str(),
        payload,
        format!("run-post-process-dead:{run_id}"),
    )
    .await
}

async fn publish_envelope(
    channel: &Channel,
    exchange: &str,
    routing_key: &str,
    payload: &[u8],
    message_id: String,
) -> Result<(), String> {
    let confirmation = channel
        .basic_publish(
            exchange,
            routing_key,
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload,
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_message_id(message_id.into()),
        )
        .await
        .map_err(|err| err.to_string())?
        .await
        .map_err(|err| err.to_string())?;
    ensure_run_post_process_publish_confirmed(routing_key, confirmation)
}

fn ensure_run_post_process_publish_confirmed(
    routing_key: &str,
    confirmation: Confirmation,
) -> Result<(), String> {
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!(
            "RabbitMQ returned unroutable Task Runner Run post-process event for {routing_key}"
        )),
        Confirmation::Nack(_) => Err(format!(
            "RabbitMQ rejected Task Runner Run post-process event for {routing_key}"
        )),
        Confirmation::NotRequested => Err(
            "RabbitMQ publisher confirm was not enabled for Task Runner Run post-process event"
                .to_string(),
        ),
    }
}

pub(crate) async fn ensure_run_post_process_topology(
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
    channel
        .queue_declare(
            topology.run_post_process_queue.as_str(),
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
            topology.run_post_process_queue.as_str(),
            topology.rabbitmq_exchange.as_str(),
            topology.run_post_process_queue.as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;

    let retry_delay_ms = u32::try_from(topology.run_post_process_retry_delay.as_millis())
        .map_err(|_| "Run post-process retry delay exceeds RabbitMQ limit".to_string())?;
    let mut retry_arguments = FieldTable::default();
    retry_arguments.insert("x-message-ttl".into(), AMQPValue::LongUInt(retry_delay_ms));
    retry_arguments.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(topology.rabbitmq_exchange.clone().into()),
    );
    retry_arguments.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(topology.run_post_process_queue.clone().into()),
    );
    channel
        .queue_declare(
            topology.run_post_process_retry_queue.as_str(),
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
            topology.run_post_process_retry_queue.as_str(),
            topology.rabbitmq_exchange.as_str(),
            topology.run_post_process_retry_queue.as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    channel
        .queue_declare(
            topology.run_post_process_dead_letter_queue.as_str(),
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
            topology.run_post_process_dead_letter_queue.as_str(),
            topology.rabbitmq_exchange.as_str(),
            topology.run_post_process_dead_letter_queue.as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

pub fn spawn_run_post_process_consumer(
    topology: TaskQueueTopology,
    run_service: RunService,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match open_run_post_process_consumer(&topology).await {
                Ok((connection, channel, mut consumer)) => {
                    let _connection = connection;
                    run_service
                        .runtime_stats()
                        .set_run_post_process_consumer_connected(true);
                    info!(
                        queue = topology.run_post_process_queue.as_str(),
                        "task runner Run post-process consumer connected to rabbitmq"
                    );
                    while let Some(delivery) = consumer.next().await {
                        let delivery = match delivery {
                            Ok(delivery) => delivery,
                            Err(err) => {
                                warn!(
                                    error = err.to_string().as_str(),
                                    "task runner Run post-process delivery failed"
                                );
                                break;
                            }
                        };
                        let envelope = match serde_json::from_slice::<RunPostProcessEnvelope>(
                            delivery.data.as_slice(),
                        ) {
                            Ok(envelope) => envelope,
                            Err(err) => {
                                warn!(
                                    error = err.to_string().as_str(),
                                    "task runner discarded invalid Run post-process event"
                                );
                                if delivery.ack(BasicAckOptions::default()).await.is_err() {
                                    break;
                                }
                                continue;
                            }
                        };
                        match run_service
                            .process_run_post_process(envelope.run_id.as_str())
                            .await
                        {
                            Ok(()) => {
                                if let Err(err) = delivery.ack(BasicAckOptions::default()).await {
                                    warn!(
                                        run_id = envelope.run_id.as_str(),
                                        error = err.to_string().as_str(),
                                        "failed to acknowledge completed Run post-process event"
                                    );
                                    break;
                                }
                            }
                            Err(err) => {
                                let attempt = match run_service
                                    .record_run_post_process_failure(
                                        envelope.run_id.as_str(),
                                        err.as_str(),
                                    )
                                    .await
                                {
                                    Ok(attempt) => attempt,
                                    Err(store_err) => {
                                        warn!(
                                            run_id = envelope.run_id.as_str(),
                                            error = store_err.as_str(),
                                            "failed to persist Run post-process failure"
                                        );
                                        0
                                    }
                                };
                                let lifecycle_retry = err.starts_with(
                                    crate::services::MCP_RUN_FINALIZATION_ERROR_PREFIX,
                                ) || err.starts_with(
                                    crate::services::WORKSPACE_INTEGRATION_RETRY_PREFIX,
                                );
                                if attempt >= topology.run_post_process_max_delivery_attempts
                                    && !lifecycle_retry
                                {
                                    if let Err(publish_err) = dead_letter_run_post_process(
                                        &channel,
                                        &topology,
                                        delivery.data.as_slice(),
                                        envelope.run_id.as_str(),
                                    )
                                    .await
                                    {
                                        warn!(
                                            run_id = envelope.run_id.as_str(),
                                            error = publish_err.as_str(),
                                            "failed to dead-letter Run post-process event"
                                        );
                                        break;
                                    }
                                    if let Err(store_err) = run_service
                                        .mark_run_post_process_dead_lettered(
                                            envelope.run_id.as_str(),
                                            err.as_str(),
                                        )
                                        .await
                                    {
                                        warn!(
                                            run_id = envelope.run_id.as_str(),
                                            error = store_err.as_str(),
                                            "Run post-process event reached the DLQ but state persistence failed"
                                        );
                                        break;
                                    }
                                    warn!(
                                        run_id = envelope.run_id.as_str(),
                                        error = err.as_str(),
                                        attempt,
                                        dead_letter_queue =
                                            topology.run_post_process_dead_letter_queue.as_str(),
                                        "Run post-processing exhausted retries and entered the DLQ"
                                    );
                                } else {
                                    if let Err(publish_err) = defer_run_post_process(
                                        &channel,
                                        &topology,
                                        delivery.data.as_slice(),
                                        envelope.run_id.as_str(),
                                    )
                                    .await
                                    {
                                        warn!(
                                            run_id = envelope.run_id.as_str(),
                                            error = publish_err.as_str(),
                                            "failed to defer Run post-process retry"
                                        );
                                        break;
                                    }
                                    warn!(
                                        run_id = envelope.run_id.as_str(),
                                        error = err.as_str(),
                                        attempt,
                                        retry_delay_ms =
                                            topology.run_post_process_retry_delay.as_millis(),
                                        "Run post-processing failed and was deferred for retry"
                                    );
                                }
                                if delivery.ack(BasicAckOptions::default()).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(err) => warn!(
                    error = err.as_str(),
                    "task runner Run post-process consumer failed to connect to rabbitmq"
                ),
            }
            run_service
                .runtime_stats()
                .set_run_post_process_consumer_connected(false);
            run_service
                .runtime_stats()
                .record_rabbitmq_consumer_reconnect();
            tokio::time::sleep(topology.rabbitmq_reconnect_delay).await;
        }
    })
}

async fn open_run_post_process_consumer(
    topology: &TaskQueueTopology,
) -> Result<(Connection, Channel, lapin::Consumer), String> {
    let rabbitmq_url = topology.rabbitmq_url.as_deref().ok_or_else(|| {
        "TASK_RUNNER_RABBITMQ_URL is required for Run post-processing".to_string()
    })?;
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|err| err.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|err| err.to_string())?;
    ensure_run_post_process_topology(&channel, topology).await?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|err| err.to_string())?;
    channel
        .basic_qos(1, BasicQosOptions::default())
        .await
        .map_err(|err| err.to_string())?;
    let consumer = channel
        .basic_consume(
            topology.run_post_process_queue.as_str(),
            RUN_POST_PROCESS_CONSUMER_TAG,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok((connection, channel, consumer))
}

pub fn spawn_run_post_process_outbox_reconciler(
    topology: TaskQueueTopology,
    run_service: RunService,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(topology.run_post_process_outbox_reconcile_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match run_service
                .publish_pending_run_post_processes(topology.run_post_process_outbox_batch_size)
                .await
            {
                Ok(count) if count > 0 => info!(
                    published_count = count,
                    "task runner reconciled pending Run post-process events"
                ),
                Ok(_) => {}
                Err(err) => warn!(
                    error = err.as_str(),
                    "task runner failed to reconcile pending Run post-process events"
                ),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_post_process_outbox_requires_confirmed_routing() {
        assert!(ensure_run_post_process_publish_confirmed(
            "task_runner.run.post_process",
            Confirmation::Ack(None),
        )
        .is_ok());
        assert!(ensure_run_post_process_publish_confirmed(
            "task_runner.run.post_process",
            Confirmation::Nack(None),
        )
        .is_err());
        assert!(ensure_run_post_process_publish_confirmed(
            "task_runner.run.post_process",
            Confirmation::NotRequested,
        )
        .is_err());
    }
}
