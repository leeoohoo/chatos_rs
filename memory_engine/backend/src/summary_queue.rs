// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use futures_util::StreamExt;
use lapin::{
    message::Delivery,
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
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::models::now_rfc3339;
use crate::repositories::{control_plane, threads};
use crate::services::summary;
use crate::state::AppState;

const SUMMARY_QUEUE_TRIGGER: &str = "queue";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SummaryRequestedEnvelope {
    tenant_id: String,
    source_id: String,
    thread_id: String,
    version: i64,
    attempt: u32,
    requested_at: String,
}

impl SummaryRequestedEnvelope {
    fn from_outbox(event: &threads::SummaryDispatchOutbox) -> Self {
        Self {
            tenant_id: event.tenant_id.clone(),
            source_id: event.source_id.clone(),
            thread_id: event.thread_id.clone(),
            version: event.summary_dispatch_version,
            attempt: 0,
            requested_at: now_rfc3339(),
        }
    }

    fn as_outbox(&self) -> threads::SummaryDispatchOutbox {
        threads::SummaryDispatchOutbox {
            tenant_id: self.tenant_id.clone(),
            source_id: self.source_id.clone(),
            thread_id: self.thread_id.clone(),
            summary_dispatch_version: self.version,
            summary_dispatch_published_version: self.version,
            summary_dispatch_consumed_version: 0,
        }
    }
}

pub async fn publish_pending_summary_for_thread(
    state: &AppState,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
) -> Result<bool, String> {
    let Some(event) =
        threads::get_pending_summary_dispatch(&state.pool, tenant_id, source_id, thread_id).await?
    else {
        return Ok(false);
    };
    publish_outbox_event(&state.pool, &state.config, &event).await?;
    Ok(true)
}

pub async fn publish_rearmed_summary_dispatch(
    state: &AppState,
    event: &threads::SummaryDispatchOutbox,
) -> Result<(), String> {
    publish_outbox_event(&state.pool, &state.config, event).await
}

pub async fn archive_summary_dead_letter(
    config: &AppConfig,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    version: i64,
    scan_limit: usize,
) -> Result<bool, String> {
    let (_connection, channel) = open_publisher(config).await?;
    let mut unmatched = Vec::new();
    let mut matched = None;
    for _ in 0..scan_limit.clamp(1, 1_000) {
        let Some(delivery) = channel
            .basic_get(
                config.summary_dead_letter_queue.as_str(),
                BasicGetOptions::default(),
            )
            .await
            .map_err(|err| err.to_string())?
        else {
            break;
        };
        let is_match = serde_json::from_slice::<SummaryRequestedEnvelope>(&delivery.data)
            .is_ok_and(|envelope| {
                envelope.tenant_id == tenant_id
                    && envelope.source_id == source_id
                    && envelope.thread_id == thread_id
                    && envelope.version == version
            });
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

pub fn start(state: Arc<AppState>) {
    for consumer_index in 0..state.config.worker_summary_concurrency.max(1) {
        tokio::spawn(run_consumer(state.clone(), consumer_index));
    }
    tokio::spawn(run_outbox_reconciler(state));
}

async fn run_consumer(state: Arc<AppState>, consumer_index: usize) {
    let mut pressure = state.pressure.subscribe();
    loop {
        if wait_until_consumer_enabled(&mut pressure, consumer_index)
            .await
            .is_err()
        {
            return;
        }
        let mut paused_for_pressure = false;
        match open_consumer(&state.config, consumer_index).await {
            Ok((connection, channel, mut consumer)) => {
                let _connection = connection;
                info!(
                    queue = state.config.summary_queue.as_str(),
                    consumer_index, "Memory Engine summary consumer connected to RabbitMQ"
                );
                loop {
                    tokio::select! {
                        changed = pressure.changed() => {
                            if changed.is_err() {
                                return;
                            }
                            if !summary_consumer_enabled(&pressure.borrow(), consumer_index) {
                                paused_for_pressure = true;
                                info!(
                                    consumer_index,
                                    pressure_level = ?pressure.borrow().level,
                                    "Memory Engine summary consumer paused by platform pressure"
                                );
                                break;
                            }
                        }
                        delivery = consumer.next() => {
                            let Some(delivery) = delivery else {
                                break;
                            };
                            let delivery = match delivery {
                                Ok(delivery) => delivery,
                                Err(err) => {
                                    warn!(
                                        consumer_index,
                                        error = err.to_string().as_str(),
                                        "Memory Engine summary delivery failed"
                                    );
                                    break;
                                }
                            };
                            if let Err(err) = handle_delivery(&state, &channel, delivery).await {
                                warn!(
                                    consumer_index,
                                    error = err.as_str(),
                                    "Memory Engine summary consumer channel will reconnect"
                                );
                                break;
                            }
                        }
                    }
                }
            }
            Err(err) => warn!(
                consumer_index,
                error = err.as_str(),
                "Memory Engine summary consumer failed to connect to RabbitMQ"
            ),
        }
        if paused_for_pressure {
            continue;
        }
        tokio::time::sleep(state.config.rabbitmq_reconnect_delay).await;
    }
}

async fn wait_until_consumer_enabled(
    pressure: &mut tokio::sync::watch::Receiver<crate::pressure::MemoryEnginePressurePolicy>,
    consumer_index: usize,
) -> Result<(), ()> {
    while !summary_consumer_enabled(&pressure.borrow(), consumer_index) {
        pressure.changed().await.map_err(|_| ())?;
    }
    Ok(())
}

fn summary_consumer_enabled(
    policy: &crate::pressure::MemoryEnginePressurePolicy,
    consumer_index: usize,
) -> bool {
    consumer_index < policy.active_summary_concurrency
}

async fn handle_delivery(
    state: &Arc<AppState>,
    channel: &Channel,
    delivery: Delivery,
) -> Result<(), String> {
    let envelope = match serde_json::from_slice::<SummaryRequestedEnvelope>(&delivery.data) {
        Ok(envelope) => envelope,
        Err(err) => {
            warn!(
                error = err.to_string().as_str(),
                "discarded invalid Memory Engine summary event"
            );
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|ack_err| ack_err.to_string())?;
            return Ok(());
        }
    };

    match process_summary_event(state, &envelope).await {
        Ok(()) => delivery
            .ack(BasicAckOptions::default())
            .await
            .map_err(|err| err.to_string()),
        Err(error) if error == crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED => {
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|err| err.to_string())
        }
        Err(error) => {
            let event = envelope.as_outbox();
            let _ =
                threads::mark_summary_dispatch_failed(&state.pool, &event, error.as_str()).await;
            let next_attempt = envelope.attempt.saturating_add(1);
            if next_attempt >= state.config.summary_max_delivery_attempts {
                let mut dead = envelope.clone();
                dead.attempt = next_attempt;
                publish_envelope(
                    channel,
                    &state.config,
                    state.config.summary_dead_letter_queue.as_str(),
                    &dead,
                )
                .await?;
                threads::mark_summary_dispatch_dead_lettered(&state.pool, &event, error.as_str())
                    .await?;
                warn!(
                    thread_id = envelope.thread_id.as_str(),
                    version = envelope.version,
                    attempt = next_attempt,
                    error = error.as_str(),
                    dead_letter_queue = state.config.summary_dead_letter_queue.as_str(),
                    "Memory Engine summary event exhausted retries and entered the DLQ"
                );
            } else {
                let mut retry = envelope.clone();
                retry.attempt = next_attempt;
                publish_envelope(
                    channel,
                    &state.config,
                    state.config.summary_retry_queue.as_str(),
                    &retry,
                )
                .await?;
                warn!(
                    thread_id = envelope.thread_id.as_str(),
                    version = envelope.version,
                    attempt = next_attempt,
                    retry_delay_ms = state.config.summary_retry_delay.as_millis(),
                    error = error.as_str(),
                    "Memory Engine summary event failed and was deferred"
                );
            }
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|err| err.to_string())
        }
    }
}

async fn process_summary_event(
    state: &Arc<AppState>,
    envelope: &SummaryRequestedEnvelope,
) -> Result<(), String> {
    let Some(dispatch_state) = threads::get_summary_dispatch_state(
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        envelope.thread_id.as_str(),
    )
    .await?
    else {
        return Ok(());
    };
    if dispatch_state.summary_dispatch_consumed_version >= envelope.version {
        return Ok(());
    }

    let policy = control_plane::get_effective_job_policy(&state.pool, "summary").await?;
    let event = envelope.as_outbox();
    if !policy.enabled {
        threads::mark_summary_dispatch_consumed(&state.pool, &event).await?;
        return Ok(());
    }

    let token_threshold = policy.token_limit.unwrap_or(6000).max(128);
    let Some(thread) = threads::get_thread_by_id(
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        envelope.thread_id.as_str(),
    )
    .await?
    else {
        return Ok(());
    };
    if thread.pending_summary_tokens < token_threshold {
        threads::mark_summary_dispatch_consumed(&state.pool, &event).await?;
        return Ok(());
    }

    summary::run_thread_summary_with_thread(
        &state.config,
        &state.pool,
        thread,
        SUMMARY_QUEUE_TRIGGER,
    )
    .await?;
    threads::mark_summary_dispatch_consumed(&state.pool, &event).await?;
    let _ = threads::rearm_summary_dispatch_if_eligible(
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        envelope.thread_id.as_str(),
        token_threshold,
    )
    .await?;
    if let Err(err) = publish_pending_summary_for_thread(
        state,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        envelope.thread_id.as_str(),
    )
    .await
    {
        warn!(
            thread_id = envelope.thread_id.as_str(),
            error = err.as_str(),
            "Memory Engine left rearmed summary event in Outbox for recovery"
        );
    }
    Ok(())
}

async fn publish_outbox_event(
    db: &crate::db::Db,
    config: &AppConfig,
    event: &threads::SummaryDispatchOutbox,
) -> Result<(), String> {
    let connection = Connection::connect(
        config.rabbitmq_url.as_str(),
        ConnectionProperties::default(),
    )
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
    ensure_topology(&channel, config).await?;
    publish_envelope(
        &channel,
        config,
        config.summary_queue.as_str(),
        &SummaryRequestedEnvelope::from_outbox(event),
    )
    .await?;
    threads::mark_summary_dispatch_published(db, event).await?;
    Ok(())
}

async fn publish_envelope(
    channel: &Channel,
    config: &AppConfig,
    routing_key: &str,
    envelope: &SummaryRequestedEnvelope,
) -> Result<(), String> {
    let payload = serde_json::to_vec(envelope).map_err(|err| err.to_string())?;
    let confirmation = channel
        .basic_publish(
            config.rabbitmq_exchange.as_str(),
            routing_key,
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload.as_slice(),
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_message_id(
                    format!(
                        "memory-summary:{}:{}:{}",
                        envelope.thread_id, envelope.version, envelope.attempt
                    )
                    .into(),
                ),
        )
        .await
        .map_err(|err| err.to_string())?
        .await
        .map_err(|err| err.to_string())?;
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!(
            "RabbitMQ returned unroutable Memory Engine summary event for {routing_key}"
        )),
        Confirmation::Nack(_) => Err(format!(
            "RabbitMQ rejected Memory Engine summary event for {routing_key}"
        )),
        Confirmation::NotRequested => Err(
            "RabbitMQ publisher confirm was not enabled for Memory Engine summary event"
                .to_string(),
        ),
    }
}

async fn ensure_topology(channel: &Channel, config: &AppConfig) -> Result<(), String> {
    channel
        .exchange_declare(
            config.rabbitmq_exchange.as_str(),
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    declare_and_bind(
        channel,
        config,
        config.summary_queue.as_str(),
        FieldTable::default(),
    )
    .await?;

    let retry_delay_ms = u32::try_from(config.summary_retry_delay.as_millis())
        .map_err(|_| "Memory Engine summary retry delay exceeds RabbitMQ limit".to_string())?;
    let mut retry_arguments = FieldTable::default();
    retry_arguments.insert("x-message-ttl".into(), AMQPValue::LongUInt(retry_delay_ms));
    retry_arguments.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(config.rabbitmq_exchange.clone().into()),
    );
    retry_arguments.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(config.summary_queue.clone().into()),
    );
    declare_and_bind(
        channel,
        config,
        config.summary_retry_queue.as_str(),
        retry_arguments,
    )
    .await?;
    declare_and_bind(
        channel,
        config,
        config.summary_dead_letter_queue.as_str(),
        FieldTable::default(),
    )
    .await
}

async fn declare_and_bind(
    channel: &Channel,
    config: &AppConfig,
    queue: &str,
    arguments: FieldTable,
) -> Result<(), String> {
    channel
        .queue_declare(
            queue,
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            arguments,
        )
        .await
        .map_err(|err| err.to_string())?;
    channel
        .queue_bind(
            queue,
            config.rabbitmq_exchange.as_str(),
            queue,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

async fn open_publisher(config: &AppConfig) -> Result<(Connection, Channel), String> {
    let connection = Connection::connect(
        config.rabbitmq_url.as_str(),
        ConnectionProperties::default(),
    )
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
    ensure_topology(&channel, config).await?;
    Ok((connection, channel))
}

async fn open_consumer(
    config: &AppConfig,
    consumer_index: usize,
) -> Result<(Connection, Channel, lapin::Consumer), String> {
    let connection = Connection::connect(
        config.rabbitmq_url.as_str(),
        ConnectionProperties::default(),
    )
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
    ensure_topology(&channel, config).await?;
    channel
        .basic_qos(1, BasicQosOptions::default())
        .await
        .map_err(|err| err.to_string())?;
    let consumer = channel
        .basic_consume(
            config.summary_queue.as_str(),
            format!("memory-engine-summary-{consumer_index}").as_str(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok((connection, channel, consumer))
}

async fn run_outbox_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(state.config.summary_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match publish_pending_outbox_batch(&state).await {
            Ok(count) if count > 0 => info!(
                published_count = count,
                "Memory Engine reconciled pending summary Outbox events"
            ),
            Ok(_) => {}
            Err(err) => warn!(
                error = err.as_str(),
                "Memory Engine failed to reconcile summary Outbox events"
            ),
        }
    }
}

async fn publish_pending_outbox_batch(state: &AppState) -> Result<usize, String> {
    let events = threads::list_pending_summary_dispatches(
        &state.pool,
        state.config.summary_outbox_batch_size,
    )
    .await?;
    let mut published = 0usize;
    for event in events {
        publish_outbox_event(&state.pool, &state.config, &event).await?;
        published += 1;
    }
    Ok(published)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{summary_consumer_enabled, wait_until_consumer_enabled, SummaryRequestedEnvelope};
    use crate::pressure::{MemoryEnginePressurePolicy, PlatformPressureLevel};
    use crate::repositories::threads::SummaryDispatchOutbox;

    #[test]
    fn outbox_event_contains_only_scope_ids_and_version() {
        let event = SummaryDispatchOutbox {
            tenant_id: "tenant-1".to_string(),
            source_id: "source-1".to_string(),
            thread_id: "thread-1".to_string(),
            summary_dispatch_version: 7,
            summary_dispatch_published_version: 6,
            summary_dispatch_consumed_version: 5,
        };

        let envelope = SummaryRequestedEnvelope::from_outbox(&event);

        assert_eq!(envelope.thread_id, "thread-1");
        assert_eq!(envelope.version, 7);
        assert_eq!(envelope.attempt, 0);
    }

    #[test]
    fn pressure_policy_enables_only_the_target_number_of_consumers() {
        let policy = MemoryEnginePressurePolicy {
            level: PlatformPressureLevel::Elevated,
            active_summary_concurrency: 2,
            reconcile_paused: false,
            refresh_interval: Duration::from_secs(5),
            queue_elevated_messages: 100,
            queue_critical_messages: 1_000,
        };

        assert!(summary_consumer_enabled(&policy, 0));
        assert!(summary_consumer_enabled(&policy, 1));
        assert!(!summary_consumer_enabled(&policy, 2));
        assert!(!summary_consumer_enabled(&policy, 3));
    }

    #[tokio::test]
    async fn paused_consumer_resumes_from_pressure_change_without_polling() {
        let elevated = MemoryEnginePressurePolicy {
            level: PlatformPressureLevel::Elevated,
            active_summary_concurrency: 1,
            reconcile_paused: false,
            refresh_interval: Duration::from_secs(5),
            queue_elevated_messages: 100,
            queue_critical_messages: 1_000,
        };
        let normal = MemoryEnginePressurePolicy {
            level: PlatformPressureLevel::Normal,
            active_summary_concurrency: 4,
            reconcile_paused: false,
            refresh_interval: Duration::from_secs(5),
            queue_elevated_messages: 100,
            queue_critical_messages: 1_000,
        };
        let (sender, mut receiver) = tokio::sync::watch::channel(elevated);
        let waiter =
            tokio::spawn(async move { wait_until_consumer_enabled(&mut receiver, 2).await });

        assert!(tokio::time::timeout(Duration::from_millis(20), async {
            while !waiter.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .is_err());
        sender.send_replace(normal);
        assert!(waiter.await.expect("consumer waiter task").is_ok());
    }
}
