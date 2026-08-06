// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::StreamExt;
use lapin::{
    message::Delivery,
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicGetOptions, BasicNackOptions,
        BasicPublishOptions, BasicQosOptions, BasicRejectOptions, ConfirmSelectOptions,
        ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
    },
    publisher_confirm::Confirmation,
    types::{AMQPValue, FieldTable},
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::api::{is_syncable_network_marketplace, run_queued_plugin_catalog_sync};
use crate::config::AppConfig;
use crate::models::PluginCatalogSyncOutboxEvent;
use crate::pressure::PlatformPressureLevel;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PluginCatalogSyncEnvelope {
    marketplace_id: String,
    event_version: i64,
    attempt: u32,
    requested_at: String,
    scheduled: bool,
}

impl PluginCatalogSyncEnvelope {
    fn from_outbox(event: &PluginCatalogSyncOutboxEvent) -> Self {
        Self {
            marketplace_id: event.marketplace_id.clone(),
            event_version: event.event_version,
            attempt: 0,
            requested_at: event.requested_at.clone(),
            scheduled: event.scheduled,
        }
    }
}

pub fn start(state: AppState) {
    if !state.config.plugin_catalog_sync_enabled {
        return;
    }
    for consumer_index in 0..state.config.plugin_catalog_consumer_concurrency.max(1) {
        tokio::spawn(run_consumer(state.clone(), consumer_index));
    }
    tokio::spawn(run_outbox_reconciler(state));
}

pub async fn publish_pending_marketplace(
    state: &AppState,
    marketplace_id: &str,
) -> Result<bool, String> {
    if !state.config.plugin_catalog_sync_enabled {
        return Ok(false);
    }
    let Some(event) = state
        .store
        .pending_plugin_catalog_sync_event(marketplace_id)
        .await?
    else {
        return Ok(false);
    };
    let (_connection, channel) = open_publisher(&state.config).await?;
    publish_outbox_event(state, &channel, &event).await?;
    Ok(true)
}

pub async fn replay_dead_lettered_marketplace(
    state: &AppState,
    marketplace_id: &str,
    dead_letter_version: i64,
) -> Result<Option<bool>, String> {
    let Some(event) = state
        .store
        .replay_dead_lettered_plugin_catalog_sync(marketplace_id, dead_letter_version)
        .await?
    else {
        return Ok(None);
    };
    let (_connection, channel) = open_publisher(&state.config).await?;
    publish_outbox_event(state, &channel, &event).await?;
    let archived = match archive_catalog_sync_dead_letter(
        &channel,
        &state.config,
        marketplace_id,
        dead_letter_version,
        1_000,
    )
    .await
    {
        Ok(archived) => archived,
        Err(error) => {
            warn!(
                marketplace_id,
                dead_letter_version,
                error = error.as_str(),
                "Plugin Catalog replay was published but old DLQ archival failed"
            );
            false
        }
    };
    Ok(Some(archived))
}

async fn archive_catalog_sync_dead_letter(
    channel: &Channel,
    config: &AppConfig,
    marketplace_id: &str,
    dead_letter_version: i64,
    scan_limit: usize,
) -> Result<bool, String> {
    let mut unmatched = Vec::new();
    let mut matched = None;
    for _ in 0..scan_limit.clamp(1, 1_000) {
        let Some(delivery) = channel
            .basic_get(
                config.plugin_catalog_dead_letter_queue.as_str(),
                BasicGetOptions::default(),
            )
            .await
            .map_err(|error| error.to_string())?
        else {
            break;
        };
        let is_match = catalog_sync_dead_letter_matches(
            delivery.data.as_slice(),
            marketplace_id,
            dead_letter_version,
        );
        if is_match {
            matched = Some(delivery);
            break;
        }
        unmatched.push(delivery);
    }

    let archived = matched.is_some();
    let mut first_error = None;
    if let Some(delivery) = matched {
        if let Err(error) = delivery.ack(BasicAckOptions::default()).await {
            first_error = Some(error.to_string());
        }
    }
    for delivery in unmatched {
        if let Err(error) = delivery
            .nack(BasicNackOptions {
                multiple: false,
                requeue: true,
            })
            .await
        {
            if first_error.is_none() {
                first_error = Some(error.to_string());
            }
        }
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    Ok(archived)
}

fn catalog_sync_dead_letter_matches(
    payload: &[u8],
    marketplace_id: &str,
    dead_letter_version: i64,
) -> bool {
    serde_json::from_slice::<PluginCatalogSyncEnvelope>(payload).is_ok_and(|envelope| {
        envelope.marketplace_id == marketplace_id && envelope.event_version == dead_letter_version
    })
}

async fn run_consumer(state: AppState, consumer_index: usize) {
    loop {
        match open_consumer(&state.config, consumer_index).await {
            Ok((connection, channel, mut consumer)) => {
                let _connection = connection;
                info!(
                    queue = state.config.plugin_catalog_queue.as_str(),
                    consumer_index, "Plugin Catalog sync consumer connected to RabbitMQ"
                );
                while let Some(delivery) = consumer.next().await {
                    let delivery = match delivery {
                        Ok(delivery) => delivery,
                        Err(error) => {
                            warn!(
                                consumer_index,
                                error = error.to_string().as_str(),
                                "Plugin Catalog sync delivery failed"
                            );
                            break;
                        }
                    };
                    if let Err(error) = handle_delivery(&state, &channel, delivery).await {
                        warn!(
                            consumer_index,
                            error = error.as_str(),
                            "Plugin Catalog sync consumer channel will reconnect"
                        );
                        break;
                    }
                }
            }
            Err(error) => warn!(
                consumer_index,
                error = error.as_str(),
                "Plugin Catalog sync consumer failed to connect to RabbitMQ"
            ),
        }
        tokio::time::sleep(state.config.plugin_catalog_rabbitmq_reconnect_delay).await;
    }
}

async fn handle_delivery(
    state: &AppState,
    channel: &Channel,
    delivery: Delivery,
) -> Result<(), String> {
    let envelope = match serde_json::from_slice::<PluginCatalogSyncEnvelope>(&delivery.data) {
        Ok(envelope) => envelope,
        Err(error) => {
            warn!(
                error = error.to_string().as_str(),
                "rejected invalid Plugin Catalog sync event into the DLQ"
            );
            delivery
                .reject(BasicRejectOptions { requeue: false })
                .await
                .map_err(|reject_error| reject_error.to_string())?;
            return Ok(());
        }
    };

    match process_event(state, channel, &envelope).await {
        Ok(()) => delivery
            .ack(BasicAckOptions::default())
            .await
            .map_err(|error| error.to_string()),
        Err(error) => {
            let mut next = envelope.clone();
            next.attempt = next.attempt.saturating_add(1);
            if next.attempt >= state.config.plugin_catalog_max_delivery_attempts {
                publish_envelope(
                    channel,
                    &state.config,
                    state.config.plugin_catalog_dead_letter_queue.as_str(),
                    &next,
                )
                .await?;
                state
                    .store
                    .mark_plugin_catalog_sync_event_dead_lettered(
                        &envelope.as_outbox(),
                        error.as_str(),
                    )
                    .await?;
                warn!(
                    marketplace_id = envelope.marketplace_id.as_str(),
                    event_version = envelope.event_version,
                    attempt = next.attempt,
                    error = error.as_str(),
                    "Plugin Catalog sync exhausted retries and entered the DLQ"
                );
            } else {
                publish_envelope(
                    channel,
                    &state.config,
                    state.config.plugin_catalog_retry_queue.as_str(),
                    &next,
                )
                .await?;
                warn!(
                    marketplace_id = envelope.marketplace_id.as_str(),
                    event_version = envelope.event_version,
                    attempt = next.attempt,
                    retry_delay_ms = state.config.plugin_catalog_retry_delay.as_millis(),
                    error = error.as_str(),
                    "Plugin Catalog sync failed and was deferred"
                );
            }
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|ack_error| ack_error.to_string())
        }
    }
}

async fn process_event(
    state: &AppState,
    channel: &Channel,
    envelope: &PluginCatalogSyncEnvelope,
) -> Result<(), String> {
    let Some(consumed) = state
        .store
        .plugin_catalog_sync_event_consumed(
            envelope.marketplace_id.as_str(),
            envelope.event_version,
        )
        .await?
    else {
        return Ok(());
    };
    if consumed {
        return Ok(());
    }

    let Some(marketplace) = state
        .store
        .get_plugin_marketplace(envelope.marketplace_id.as_str())
        .await?
    else {
        return Ok(());
    };
    if !is_syncable_network_marketplace(&marketplace) {
        state
            .store
            .complete_plugin_catalog_sync_event(&envelope.as_outbox(), false)
            .await?;
        return Ok(());
    }

    if should_defer_scheduled_sync(envelope, state.pressure.snapshot().level) {
        publish_envelope(
            channel,
            &state.config,
            state.config.plugin_catalog_schedule_queue.as_str(),
            envelope,
        )
        .await?;
        info!(
            marketplace_id = envelope.marketplace_id.as_str(),
            event_version = envelope.event_version,
            "Plugin Catalog scheduled sync deferred while platform pressure is critical"
        );
        return Ok(());
    }

    run_queued_plugin_catalog_sync(state, envelope.marketplace_id.as_str()).await?;
    complete_event_and_schedule_next(state, channel, envelope).await
}

async fn complete_event_and_schedule_next(
    state: &AppState,
    channel: &Channel,
    envelope: &PluginCatalogSyncEnvelope,
) -> Result<(), String> {
    let schedule_next = state
        .store
        .get_plugin_marketplace(envelope.marketplace_id.as_str())
        .await?
        .is_some_and(|marketplace| is_syncable_network_marketplace(&marketplace));
    let next = state
        .store
        .complete_plugin_catalog_sync_event(&envelope.as_outbox(), schedule_next)
        .await?;
    if let Some(next) = next {
        if let Err(error) = publish_outbox_event(state, channel, &next).await {
            warn!(
                marketplace_id = next.marketplace_id.as_str(),
                event_version = next.event_version,
                error = error.as_str(),
                "Plugin Management left next scheduled Catalog sync event in Outbox"
            );
        }
    }
    Ok(())
}

impl PluginCatalogSyncEnvelope {
    fn as_outbox(&self) -> PluginCatalogSyncOutboxEvent {
        PluginCatalogSyncOutboxEvent {
            marketplace_id: self.marketplace_id.clone(),
            event_version: self.event_version,
            requested_at: self.requested_at.clone(),
            scheduled: self.scheduled,
        }
    }
}

fn should_defer_scheduled_sync(
    envelope: &PluginCatalogSyncEnvelope,
    pressure_level: PlatformPressureLevel,
) -> bool {
    envelope.scheduled && pressure_level == PlatformPressureLevel::Critical
}

async fn publish_outbox_event(
    state: &AppState,
    channel: &Channel,
    event: &PluginCatalogSyncOutboxEvent,
) -> Result<(), String> {
    let routing_key = if event.scheduled {
        state.config.plugin_catalog_schedule_queue.as_str()
    } else {
        state.config.plugin_catalog_queue.as_str()
    };
    publish_envelope(
        channel,
        &state.config,
        routing_key,
        &PluginCatalogSyncEnvelope::from_outbox(event),
    )
    .await?;
    state
        .store
        .mark_plugin_catalog_sync_event_published(event)
        .await?;
    Ok(())
}

async fn publish_envelope(
    channel: &Channel,
    config: &AppConfig,
    routing_key: &str,
    envelope: &PluginCatalogSyncEnvelope,
) -> Result<(), String> {
    let payload = serde_json::to_vec(envelope).map_err(|error| error.to_string())?;
    let message_id = format!(
        "plugin-catalog:{}:{}:{}",
        envelope.marketplace_id, envelope.event_version, envelope.attempt
    );
    let mut headers = FieldTable::default();
    headers.insert(
        "message_id".into(),
        AMQPValue::LongString(message_id.clone().into()),
    );
    headers.insert(
        "correlation_id".into(),
        AMQPValue::LongString(
            format!(
                "plugin-catalog:{}:{}",
                envelope.marketplace_id, envelope.event_version
            )
            .into(),
        ),
    );
    let confirmation = channel
        .basic_publish(
            config.plugin_catalog_rabbitmq_exchange.as_str(),
            routing_key,
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload.as_slice(),
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_message_id(message_id.into())
                .with_correlation_id(
                    format!(
                        "plugin-catalog:{}:{}",
                        envelope.marketplace_id, envelope.event_version
                    )
                    .into(),
                )
                .with_headers(headers),
        )
        .await
        .map_err(|error| error.to_string())?
        .await
        .map_err(|error| error.to_string())?;
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!(
            "RabbitMQ returned unroutable Plugin Catalog sync event for {routing_key}"
        )),
        Confirmation::Nack(_) => Err(format!(
            "RabbitMQ rejected Plugin Catalog sync event for {routing_key}"
        )),
        Confirmation::NotRequested => Err(
            "RabbitMQ publisher confirm was not enabled for Plugin Catalog sync event".to_string(),
        ),
    }
}

async fn ensure_topology(channel: &Channel, config: &AppConfig) -> Result<(), String> {
    channel
        .exchange_declare(
            config.plugin_catalog_rabbitmq_exchange.as_str(),
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let mut main_arguments = FieldTable::default();
    main_arguments.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(config.plugin_catalog_rabbitmq_exchange.clone().into()),
    );
    main_arguments.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(config.plugin_catalog_dead_letter_queue.clone().into()),
    );
    declare_and_bind(
        channel,
        config,
        config.plugin_catalog_queue.as_str(),
        main_arguments,
    )
    .await?;

    let retry_delay = queue_ttl_ms(
        config.plugin_catalog_retry_delay,
        "Plugin Catalog retry delay",
    )?;
    let mut retry_arguments = delayed_queue_arguments(config, retry_delay);
    retry_arguments.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(config.plugin_catalog_queue.clone().into()),
    );
    declare_and_bind(
        channel,
        config,
        config.plugin_catalog_retry_queue.as_str(),
        retry_arguments,
    )
    .await?;

    let schedule_delay = queue_ttl_ms(
        config.plugin_catalog_sync_interval,
        "Plugin Catalog schedule interval",
    )?;
    let mut schedule_arguments = delayed_queue_arguments(config, schedule_delay);
    schedule_arguments.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(config.plugin_catalog_queue.clone().into()),
    );
    declare_and_bind(
        channel,
        config,
        config.plugin_catalog_schedule_queue.as_str(),
        schedule_arguments,
    )
    .await?;
    declare_and_bind(
        channel,
        config,
        config.plugin_catalog_dead_letter_queue.as_str(),
        FieldTable::default(),
    )
    .await
}

fn queue_ttl_ms(duration: std::time::Duration, label: &str) -> Result<u32, String> {
    u32::try_from(duration.as_millis())
        .map_err(|_| format!("{label} exceeds RabbitMQ x-message-ttl limit"))
}

fn delayed_queue_arguments(config: &AppConfig, delay_ms: u32) -> FieldTable {
    let mut arguments = FieldTable::default();
    arguments.insert("x-message-ttl".into(), AMQPValue::LongUInt(delay_ms));
    arguments.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(config.plugin_catalog_rabbitmq_exchange.clone().into()),
    );
    arguments
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
        .map_err(|error| error.to_string())?;
    channel
        .queue_bind(
            queue,
            config.plugin_catalog_rabbitmq_exchange.as_str(),
            queue,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn open_publisher(config: &AppConfig) -> Result<(Connection, Channel), String> {
    let connection = Connection::connect(
        config.plugin_catalog_rabbitmq_url.as_str(),
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
    ensure_topology(&channel, config).await?;
    Ok((connection, channel))
}

async fn open_consumer(
    config: &AppConfig,
    consumer_index: usize,
) -> Result<(Connection, Channel, lapin::Consumer), String> {
    let (connection, channel) = open_publisher(config).await?;
    channel
        .basic_qos(1, BasicQosOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    let consumer = channel
        .basic_consume(
            config.plugin_catalog_queue.as_str(),
            format!("plugin-catalog-sync-{consumer_index}").as_str(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok((connection, channel, consumer))
}

async fn run_outbox_reconciler(state: AppState) {
    let mut interval = tokio::time::interval(state.config.plugin_catalog_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match reconcile_outbox(&state).await {
            Ok(published) if published > 0 => info!(
                published_count = published,
                "Plugin Management reconciled pending Catalog sync Outbox events"
            ),
            Ok(_) => {}
            Err(error) => warn!(
                error = error.as_str(),
                "Plugin Management failed to reconcile Catalog sync Outbox events"
            ),
        }
    }
}

async fn reconcile_outbox(state: &AppState) -> Result<usize, String> {
    let recovered = state
        .store
        .recover_plugin_catalog_sync_events(state.config.plugin_catalog_outbox_batch_size)
        .await?;
    let events = state
        .store
        .list_pending_plugin_catalog_sync_events(state.config.plugin_catalog_outbox_batch_size)
        .await?;
    if events.is_empty() {
        return Ok(recovered as usize);
    }
    let (_connection, channel) = open_publisher(&state.config).await?;
    let mut published = 0_usize;
    for event in events {
        publish_outbox_event(state, &channel, &event).await?;
        published += 1;
    }
    Ok(published)
}

#[cfg(test)]
mod tests {
    use super::{
        catalog_sync_dead_letter_matches, should_defer_scheduled_sync, PluginCatalogSyncEnvelope,
    };
    use crate::models::PluginCatalogSyncOutboxEvent;
    use crate::pressure::PlatformPressureLevel;

    #[test]
    fn catalog_sync_event_preserves_scheduled_source() {
        let envelope = PluginCatalogSyncEnvelope::from_outbox(&PluginCatalogSyncOutboxEvent {
            marketplace_id: "marketplace-1".to_string(),
            event_version: 4,
            requested_at: "2026-08-05T00:00:00Z".to_string(),
            scheduled: true,
        });
        assert_eq!(envelope.marketplace_id, "marketplace-1");
        assert_eq!(envelope.event_version, 4);
        assert_eq!(envelope.attempt, 0);
        assert!(envelope.scheduled);
    }

    #[test]
    fn dead_letter_match_requires_exact_marketplace_and_old_version() {
        let payload = serde_json::to_vec(&PluginCatalogSyncEnvelope {
            marketplace_id: "marketplace-1".to_string(),
            event_version: 4,
            attempt: 3,
            requested_at: "2026-08-05T00:00:00Z".to_string(),
            scheduled: false,
        })
        .unwrap();
        assert!(catalog_sync_dead_letter_matches(
            payload.as_slice(),
            "marketplace-1",
            4
        ));
        assert!(!catalog_sync_dead_letter_matches(
            payload.as_slice(),
            "marketplace-2",
            4
        ));
        assert!(!catalog_sync_dead_letter_matches(
            payload.as_slice(),
            "marketplace-1",
            5
        ));
        assert!(!catalog_sync_dead_letter_matches(
            b"not-json",
            "marketplace-1",
            4
        ));
    }

    #[test]
    fn critical_pressure_defers_only_scheduled_catalog_sync() {
        let scheduled = PluginCatalogSyncEnvelope {
            marketplace_id: "marketplace-1".to_string(),
            event_version: 4,
            attempt: 0,
            requested_at: "2026-08-05T00:00:00Z".to_string(),
            scheduled: true,
        };
        let explicit = PluginCatalogSyncEnvelope {
            scheduled: false,
            ..scheduled.clone()
        };

        assert!(should_defer_scheduled_sync(
            &scheduled,
            PlatformPressureLevel::Critical
        ));
        assert!(!should_defer_scheduled_sync(
            &scheduled,
            PlatformPressureLevel::Elevated
        ));
        assert!(!should_defer_scheduled_sync(
            &explicit,
            PlatformPressureLevel::Critical
        ));
    }
}
