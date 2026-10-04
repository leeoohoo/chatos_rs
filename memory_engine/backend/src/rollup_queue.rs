// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use futures_util::StreamExt;
use lapin::{
    message::Delivery,
    options::{BasicAckOptions, BasicGetOptions, BasicNackOptions},
    Channel,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::models::now_rfc3339;
use crate::rabbitmq_queue::{self, QueueTopology};
use crate::repositories::{control_plane, summaries};
use crate::services::{control_plane as cp_service, summary};
use crate::state::AppState;

const ROLLUP_QUEUE_TRIGGER: &str = "queue";

fn queue_topology(config: &AppConfig) -> QueueTopology<'_> {
    QueueTopology {
        exchange: config.rabbitmq_exchange.as_str(),
        queue: config.rollup_queue.as_str(),
        retry_queue: config.rollup_retry_queue.as_str(),
        dead_letter_queue: config.rollup_dead_letter_queue.as_str(),
        retry_delay: config.rollup_retry_delay,
        stream_name: "rollup",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct RollupRequestedEnvelope {
    tenant_id: String,
    source_id: String,
    thread_id: String,
    summary_id: String,
    version: i64,
    attempt: u32,
    requested_at: String,
}

impl RollupRequestedEnvelope {
    fn from_outbox(event: &summaries::RollupDispatchOutbox) -> Self {
        Self {
            tenant_id: event.tenant_id.clone(),
            source_id: event.source_id.clone(),
            thread_id: event.thread_id.clone(),
            summary_id: event.id.clone(),
            version: event.rollup_dispatch_version,
            attempt: 0,
            requested_at: now_rfc3339(),
        }
    }

    fn as_outbox(&self) -> summaries::RollupDispatchOutbox {
        summaries::RollupDispatchOutbox {
            id: self.summary_id.clone(),
            tenant_id: self.tenant_id.clone(),
            source_id: self.source_id.clone(),
            thread_id: self.thread_id.clone(),
            rollup_dispatch_version: self.version,
            rollup_dispatch_published_version: self.version,
            rollup_dispatch_consumed_version: 0,
            rollup_dispatch_pending: false,
        }
    }
}

pub async fn publish_pending_rollup_for_summary(
    config: &AppConfig,
    db: &crate::db::Db,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
) -> Result<bool, String> {
    let Some(event) =
        summaries::get_pending_rollup_dispatch(db, tenant_id, source_id, summary_id).await?
    else {
        return Ok(false);
    };
    publish_outbox_event(db, config, &event).await?;
    Ok(true)
}

pub async fn publish_rearmed_rollup_dispatch(
    state: &AppState,
    event: &summaries::RollupDispatchOutbox,
) -> Result<(), String> {
    publish_outbox_event(&state.pool, &state.config, event).await
}

pub async fn archive_rollup_dead_letter(
    config: &AppConfig,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
    version: i64,
    scan_limit: usize,
) -> Result<bool, String> {
    let topology = queue_topology(config);
    let (_connection, channel) =
        rabbitmq_queue::open_publisher(config.rabbitmq_url.as_str(), &topology).await?;
    let mut unmatched = Vec::new();
    let mut matched = None;
    for _ in 0..scan_limit.clamp(1, 1_000) {
        let Some(delivery) = channel
            .basic_get(
                config.rollup_dead_letter_queue.as_str(),
                BasicGetOptions::default(),
            )
            .await
            .map_err(|err| err.to_string())?
        else {
            break;
        };
        let is_match = serde_json::from_slice::<RollupRequestedEnvelope>(&delivery.data).is_ok_and(
            |envelope| {
                envelope.tenant_id == tenant_id
                    && envelope.source_id == source_id
                    && envelope.summary_id == summary_id
                    && envelope.version == version
            },
        );
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
    for consumer_index in 0..state.config.worker_rollup_concurrency.max(1) {
        tokio::spawn(run_consumer(state.clone(), consumer_index));
    }
    tokio::spawn(run_outbox_reconciler(state));
}

async fn run_consumer(state: Arc<AppState>, consumer_index: usize) {
    loop {
        let topology = queue_topology(&state.config);
        let consumer_tag = format!("memory-engine-rollup-{consumer_index}");
        match rabbitmq_queue::open_consumer(
            state.config.rabbitmq_url.as_str(),
            &topology,
            consumer_tag.as_str(),
        )
        .await
        {
            Ok((connection, channel, mut consumer)) => {
                let _connection = connection;
                info!(
                    queue = state.config.rollup_queue.as_str(),
                    consumer_index, "Memory Engine rollup consumer connected to RabbitMQ"
                );
                while let Some(delivery) = consumer.next().await {
                    let delivery = match delivery {
                        Ok(delivery) => delivery,
                        Err(err) => {
                            warn!(
                                consumer_index,
                                error = err.to_string().as_str(),
                                "Memory Engine rollup delivery failed"
                            );
                            break;
                        }
                    };
                    if let Err(err) = handle_delivery(&state, &channel, delivery).await {
                        warn!(
                            consumer_index,
                            error = err.as_str(),
                            "Memory Engine rollup consumer channel will reconnect"
                        );
                        break;
                    }
                }
            }
            Err(err) => warn!(
                consumer_index,
                error = err.as_str(),
                "Memory Engine rollup consumer failed to connect to RabbitMQ"
            ),
        }
        tokio::time::sleep(state.config.rabbitmq_reconnect_delay).await;
    }
}

async fn handle_delivery(
    state: &Arc<AppState>,
    channel: &Channel,
    delivery: Delivery,
) -> Result<(), String> {
    let envelope = match serde_json::from_slice::<RollupRequestedEnvelope>(&delivery.data) {
        Ok(envelope) => envelope,
        Err(err) => {
            warn!(
                error = err.to_string().as_str(),
                "discarded invalid Memory Engine rollup event"
            );
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|ack_err| ack_err.to_string())?;
            return Ok(());
        }
    };

    match process_rollup_event(state, &envelope).await {
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
                summaries::mark_rollup_dispatch_failed(&state.pool, &event, error.as_str()).await;
            let next_attempt = envelope.attempt.saturating_add(1);
            if next_attempt >= state.config.rollup_max_delivery_attempts {
                let mut dead = envelope.clone();
                dead.attempt = next_attempt;
                publish_envelope(
                    channel,
                    &state.config,
                    state.config.rollup_dead_letter_queue.as_str(),
                    &dead,
                )
                .await?;
                summaries::mark_rollup_dispatch_dead_lettered(&state.pool, &event, error.as_str())
                    .await?;
                warn!(
                    summary_id = envelope.summary_id.as_str(),
                    version = envelope.version,
                    attempt = next_attempt,
                    error = error.as_str(),
                    dead_letter_queue = state.config.rollup_dead_letter_queue.as_str(),
                    "Memory Engine rollup event exhausted retries and entered the DLQ"
                );
            } else {
                let mut retry = envelope.clone();
                retry.attempt = next_attempt;
                publish_envelope(
                    channel,
                    &state.config,
                    state.config.rollup_retry_queue.as_str(),
                    &retry,
                )
                .await?;
                warn!(
                    summary_id = envelope.summary_id.as_str(),
                    version = envelope.version,
                    attempt = next_attempt,
                    retry_delay_ms = state.config.rollup_retry_delay.as_millis(),
                    error = error.as_str(),
                    "Memory Engine rollup event failed and was deferred"
                );
            }
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|err| err.to_string())
        }
    }
}

async fn process_rollup_event(
    state: &Arc<AppState>,
    envelope: &RollupRequestedEnvelope,
) -> Result<(), String> {
    let Some(dispatch_state) = summaries::get_rollup_dispatch_state(
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        envelope.summary_id.as_str(),
    )
    .await?
    else {
        return Ok(());
    };
    if dispatch_state.rollup_dispatch_consumed_version >= envelope.version {
        return Ok(());
    }

    let policy = control_plane::get_effective_job_policy(&state.pool, "rollup").await?;
    let event = envelope.as_outbox();
    if !policy.enabled {
        summaries::mark_rollup_dispatch_consumed(&state.pool, &event).await?;
        return Ok(());
    }

    let settings = cp_service::build_rollup_settings_from_policy(&policy);
    let Some(prepared) = summary::prepare_thread_rollup(
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        envelope.thread_id.as_str(),
        &settings,
    )
    .await?
    else {
        summaries::mark_rollup_dispatch_consumed(&state.pool, &event).await?;
        return Ok(());
    };

    summary::run_prepared_thread_rollup(
        &state.config,
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        envelope.thread_id.as_str(),
        prepared,
        &settings,
        ROLLUP_QUEUE_TRIGGER,
    )
    .await?;
    summaries::mark_rollup_dispatch_consumed(&state.pool, &event).await?;

    if summary::prepare_thread_rollup(
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        envelope.thread_id.as_str(),
        &settings,
    )
    .await?
    .is_some()
    {
        if let Some(next) = summaries::rearm_rollup_dispatch_if_eligible(
            &state.pool,
            envelope.tenant_id.as_str(),
            envelope.source_id.as_str(),
            envelope.thread_id.as_str(),
            settings.max_level,
        )
        .await?
        {
            if next.rollup_dispatch_pending {
                publish_outbox_event(&state.pool, &state.config, &next).await?;
            }
        }
    }
    Ok(())
}

async fn publish_outbox_event(
    db: &crate::db::Db,
    config: &AppConfig,
    event: &summaries::RollupDispatchOutbox,
) -> Result<(), String> {
    let topology = queue_topology(config);
    let (_connection, channel) =
        rabbitmq_queue::open_publisher(config.rabbitmq_url.as_str(), &topology).await?;
    publish_envelope(
        &channel,
        config,
        config.rollup_queue.as_str(),
        &RollupRequestedEnvelope::from_outbox(event),
    )
    .await?;
    summaries::mark_rollup_dispatch_published(db, event).await?;
    Ok(())
}

async fn publish_envelope(
    channel: &Channel,
    config: &AppConfig,
    routing_key: &str,
    envelope: &RollupRequestedEnvelope,
) -> Result<(), String> {
    let payload = serde_json::to_vec(envelope).map_err(|err| err.to_string())?;
    rabbitmq_queue::publish_persistent_json(
        channel,
        &queue_topology(config),
        routing_key,
        payload.as_slice(),
        format!(
            "memory-rollup:{}:{}:{}",
            envelope.summary_id, envelope.version, envelope.attempt
        ),
    )
    .await
}

async fn run_outbox_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(state.config.rollup_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match reconcile_rollup_outbox(&state).await {
            Ok(count) if count > 0 => info!(
                published_count = count,
                "Memory Engine reconciled pending rollup Outbox events"
            ),
            Ok(_) => {}
            Err(err) => warn!(
                error = err.as_str(),
                "Memory Engine failed to reconcile rollup Outbox events"
            ),
        }
    }
}

async fn reconcile_rollup_outbox(state: &AppState) -> Result<usize, String> {
    let mut published = publish_pending_outbox_batch(state).await?;
    let policy = control_plane::get_effective_job_policy(&state.pool, "rollup").await?;
    if !policy.enabled {
        return Ok(published);
    }

    let settings = cp_service::build_rollup_settings_from_policy(&policy);
    let candidates = summaries::list_threads_with_pending_rollups(
        &state.pool,
        None,
        None,
        settings.max_level,
        state.config.rollup_outbox_batch_size,
    )
    .await?;
    for (tenant_id, source_id, thread_id) in candidates {
        if summary::prepare_thread_rollup(
            &state.pool,
            tenant_id.as_str(),
            source_id.as_str(),
            thread_id.as_str(),
            &settings,
        )
        .await?
        .is_none()
        {
            continue;
        }
        let Some(event) = summaries::rearm_rollup_dispatch_if_eligible(
            &state.pool,
            tenant_id.as_str(),
            source_id.as_str(),
            thread_id.as_str(),
            settings.max_level,
        )
        .await?
        else {
            continue;
        };
        if event.rollup_dispatch_pending {
            publish_outbox_event(&state.pool, &state.config, &event).await?;
            published += 1;
        }
    }
    Ok(published)
}

async fn publish_pending_outbox_batch(state: &AppState) -> Result<usize, String> {
    let events = summaries::list_pending_rollup_dispatches(
        &state.pool,
        state.config.rollup_outbox_batch_size,
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
    use super::RollupRequestedEnvelope;
    use crate::repositories::summaries::RollupDispatchOutbox;

    #[test]
    fn outbox_event_contains_only_scope_ids_and_version() {
        let event = RollupDispatchOutbox {
            id: "summary-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "source-1".to_string(),
            thread_id: "thread-1".to_string(),
            rollup_dispatch_version: 7,
            rollup_dispatch_published_version: 6,
            rollup_dispatch_consumed_version: 5,
            rollup_dispatch_pending: true,
        };

        let envelope = RollupRequestedEnvelope::from_outbox(&event);

        assert_eq!(envelope.thread_id, "thread-1");
        assert_eq!(envelope.summary_id, "summary-1");
        assert_eq!(envelope.version, 7);
        assert_eq!(envelope.attempt, 0);
    }
}
