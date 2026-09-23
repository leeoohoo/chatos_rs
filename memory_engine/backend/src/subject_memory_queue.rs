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
use crate::repositories::{control_plane, subject_memory_scopes, summaries, threads};
use crate::services::subject_memory;
use crate::state::AppState;

const SOURCE_AVAILABLE_EVENT: &str = "source_available";
const SCOPE_REQUESTED_EVENT: &str = "scope_requested";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SubjectMemoryEnvelope {
    event_type: String,
    tenant_id: String,
    source_id: String,
    summary_id: Option<String>,
    thread_id: Option<String>,
    summary_type: Option<String>,
    scope_key: Option<String>,
    version: i64,
    attempt: u32,
    requested_at: String,
}

impl SubjectMemoryEnvelope {
    fn from_source(event: &summaries::SubjectMemorySourceDispatchOutbox) -> Self {
        Self {
            event_type: SOURCE_AVAILABLE_EVENT.to_string(),
            tenant_id: event.tenant_id.clone(),
            source_id: event.source_id.clone(),
            summary_id: Some(event.id.clone()),
            thread_id: Some(event.thread_id.clone()),
            summary_type: Some(event.summary_type.clone()),
            scope_key: None,
            version: event.subject_memory_source_dispatch_version,
            attempt: 0,
            requested_at: now_rfc3339(),
        }
    }

    fn from_scope(event: &subject_memory_scopes::SubjectMemoryScopeDispatchOutbox) -> Self {
        Self {
            event_type: SCOPE_REQUESTED_EVENT.to_string(),
            tenant_id: event.tenant_id.clone(),
            source_id: event.source_id.clone(),
            summary_id: None,
            thread_id: None,
            summary_type: None,
            scope_key: Some(event.scope_key.clone()),
            version: event.subject_memory_dispatch_version,
            attempt: 0,
            requested_at: now_rfc3339(),
        }
    }

    fn source_outbox(&self) -> Result<summaries::SubjectMemorySourceDispatchOutbox, String> {
        Ok(summaries::SubjectMemorySourceDispatchOutbox {
            id: required_id(self.summary_id.as_deref(), "summary_id")?.to_string(),
            tenant_id: self.tenant_id.clone(),
            source_id: self.source_id.clone(),
            thread_id: required_id(self.thread_id.as_deref(), "thread_id")?.to_string(),
            summary_type: required_id(self.summary_type.as_deref(), "summary_type")?.to_string(),
            subject_memory_source_dispatch_version: self.version,
            subject_memory_source_dispatch_published_version: self.version,
            subject_memory_source_dispatch_consumed_version: 0,
            subject_memory_source_dispatch_pending: false,
        })
    }

    fn scope_outbox(
        &self,
    ) -> Result<subject_memory_scopes::SubjectMemoryScopeDispatchOutbox, String> {
        Ok(subject_memory_scopes::SubjectMemoryScopeDispatchOutbox {
            id: String::new(),
            tenant_id: self.tenant_id.clone(),
            source_id: self.source_id.clone(),
            scope_key: required_id(self.scope_key.as_deref(), "scope_key")?.to_string(),
            subject_memory_dispatch_version: self.version,
            subject_memory_dispatch_published_version: self.version,
            subject_memory_dispatch_consumed_version: 0,
            subject_memory_dispatch_pending: false,
        })
    }

    fn message_identity(&self) -> &str {
        self.summary_id
            .as_deref()
            .or(self.scope_key.as_deref())
            .unwrap_or("invalid")
    }
}

fn required_id<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("subject memory event missing {field}"))
}

pub async fn publish_pending_source_for_summary(
    config: &AppConfig,
    db: &crate::db::Db,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
) -> Result<bool, String> {
    let Some(event) =
        summaries::get_pending_subject_memory_source_dispatch(db, tenant_id, source_id, summary_id)
            .await?
    else {
        return Ok(false);
    };
    let (_connection, channel) = open_publisher(config).await?;
    publish_source_outbox(db, config, &channel, &event).await?;
    Ok(true)
}

pub async fn publish_pending_scope(
    config: &AppConfig,
    db: &crate::db::Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<bool, String> {
    let Some(event) = subject_memory_scopes::get_pending_subject_memory_dispatch(
        db, tenant_id, source_id, scope_key,
    )
    .await?
    else {
        return Ok(false);
    };
    let (_connection, channel) = open_publisher(config).await?;
    publish_scope_outbox(db, config, &channel, &event).await?;
    Ok(true)
}

pub async fn publish_rearmed_source_dispatch(
    config: &AppConfig,
    db: &crate::db::Db,
    event: &summaries::SubjectMemorySourceDispatchOutbox,
) -> Result<(), String> {
    let (_connection, channel) = open_publisher(config).await?;
    publish_source_outbox(db, config, &channel, event).await
}

pub async fn publish_rearmed_scope_dispatch(
    config: &AppConfig,
    db: &crate::db::Db,
    event: &subject_memory_scopes::SubjectMemoryScopeDispatchOutbox,
) -> Result<(), String> {
    let (_connection, channel) = open_publisher(config).await?;
    publish_scope_outbox(db, config, &channel, event).await
}

pub async fn archive_subject_memory_source_dead_letter(
    config: &AppConfig,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
    version: i64,
    scan_limit: usize,
) -> Result<bool, String> {
    archive_subject_memory_dead_letter(
        config,
        SOURCE_AVAILABLE_EVENT,
        tenant_id,
        source_id,
        summary_id,
        version,
        scan_limit,
    )
    .await
}

pub async fn archive_subject_memory_scope_dead_letter(
    config: &AppConfig,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
    version: i64,
    scan_limit: usize,
) -> Result<bool, String> {
    archive_subject_memory_dead_letter(
        config,
        SCOPE_REQUESTED_EVENT,
        tenant_id,
        source_id,
        scope_key,
        version,
        scan_limit,
    )
    .await
}

async fn archive_subject_memory_dead_letter(
    config: &AppConfig,
    event_type: &str,
    tenant_id: &str,
    source_id: &str,
    item_id: &str,
    version: i64,
    scan_limit: usize,
) -> Result<bool, String> {
    let (_connection, channel) = open_publisher(config).await?;
    let mut unmatched = Vec::new();
    let mut matched = None;
    for _ in 0..scan_limit.clamp(1, 1_000) {
        let Some(delivery) = channel
            .basic_get(
                config.subject_memory_dead_letter_queue.as_str(),
                BasicGetOptions::default(),
            )
            .await
            .map_err(|err| err.to_string())?
        else {
            break;
        };
        let is_match =
            serde_json::from_slice::<SubjectMemoryEnvelope>(&delivery.data).is_ok_and(|envelope| {
                envelope.event_type == event_type
                    && envelope.tenant_id == tenant_id
                    && envelope.source_id == source_id
                    && envelope.message_identity() == item_id
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
    for consumer_index in 0..state.config.worker_subject_memory_concurrency.max(1) {
        tokio::spawn(run_consumer(state.clone(), consumer_index));
    }
    tokio::spawn(run_outbox_reconciler(state));
}

async fn run_consumer(state: Arc<AppState>, consumer_index: usize) {
    loop {
        match open_consumer(&state.config, consumer_index).await {
            Ok((connection, channel, mut consumer)) => {
                let _connection = connection;
                info!(
                    queue = state.config.subject_memory_queue.as_str(),
                    consumer_index, "Memory Engine subject memory consumer connected to RabbitMQ"
                );
                while let Some(delivery) = consumer.next().await {
                    let delivery = match delivery {
                        Ok(delivery) => delivery,
                        Err(err) => {
                            warn!(
                                consumer_index,
                                error = err.to_string().as_str(),
                                "Memory Engine subject memory delivery failed"
                            );
                            break;
                        }
                    };
                    if let Err(err) = handle_delivery(&state, &channel, delivery).await {
                        warn!(
                            consumer_index,
                            error = err.as_str(),
                            "Memory Engine subject memory consumer channel will reconnect"
                        );
                        break;
                    }
                }
            }
            Err(err) => warn!(
                consumer_index,
                error = err.as_str(),
                "Memory Engine subject memory consumer failed to connect to RabbitMQ"
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
    let envelope = match serde_json::from_slice::<SubjectMemoryEnvelope>(&delivery.data) {
        Ok(envelope) => envelope,
        Err(err) => {
            warn!(
                error = err.to_string().as_str(),
                "discarded invalid Memory Engine subject memory event"
            );
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|ack_err| ack_err.to_string())?;
            return Ok(());
        }
    };

    match process_event(state, channel, &envelope).await {
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
            mark_failed(&state.pool, &envelope, error.as_str()).await;
            let next_attempt = envelope.attempt.saturating_add(1);
            let mut next = envelope.clone();
            next.attempt = next_attempt;
            if next_attempt >= state.config.subject_memory_max_delivery_attempts {
                publish_envelope(
                    channel,
                    &state.config,
                    state.config.subject_memory_dead_letter_queue.as_str(),
                    &next,
                )
                .await?;
                mark_dead_lettered(&state.pool, &envelope, error.as_str()).await?;
                warn!(
                    event_type = envelope.event_type.as_str(),
                    event_id = envelope.message_identity(),
                    version = envelope.version,
                    attempt = next_attempt,
                    error = error.as_str(),
                    "Memory Engine subject memory event exhausted retries and entered the DLQ"
                );
            } else {
                publish_envelope(
                    channel,
                    &state.config,
                    state.config.subject_memory_retry_queue.as_str(),
                    &next,
                )
                .await?;
                warn!(
                    event_type = envelope.event_type.as_str(),
                    event_id = envelope.message_identity(),
                    version = envelope.version,
                    attempt = next_attempt,
                    retry_delay_ms = state.config.subject_memory_retry_delay.as_millis(),
                    error = error.as_str(),
                    "Memory Engine subject memory event failed and was deferred"
                );
            }
            delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|err| err.to_string())
        }
    }
}

async fn process_event(
    state: &Arc<AppState>,
    channel: &Channel,
    envelope: &SubjectMemoryEnvelope,
) -> Result<(), String> {
    match envelope.event_type.as_str() {
        SOURCE_AVAILABLE_EVENT => process_source_event(state, channel, envelope).await,
        SCOPE_REQUESTED_EVENT => process_scope_event(state, channel, envelope).await,
        _ => Err(format!(
            "unsupported subject memory event type {}",
            envelope.event_type
        )),
    }
}

async fn process_source_event(
    state: &Arc<AppState>,
    channel: &Channel,
    envelope: &SubjectMemoryEnvelope,
) -> Result<(), String> {
    let event = envelope.source_outbox()?;
    let Some(dispatch_state) = summaries::get_subject_memory_source_dispatch_state(
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        event.id.as_str(),
    )
    .await?
    else {
        return Ok(());
    };
    if dispatch_state.subject_memory_source_dispatch_consumed_version >= envelope.version {
        return Ok(());
    }

    let policy = control_plane::get_effective_job_policy(&state.pool, "subject_memory").await?;
    if !policy.enabled {
        summaries::mark_subject_memory_source_dispatch_consumed(&state.pool, &event).await?;
        return Ok(());
    }

    let Some(thread) = threads::get_thread_by_id(
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        event.thread_id.as_str(),
    )
    .await?
    else {
        summaries::mark_subject_memory_source_dispatch_consumed(&state.pool, &event).await?;
        return Ok(());
    };
    let labels = thread.labels.unwrap_or_default();
    let scopes = subject_memory_scopes::list_matching_active_subject_memory_scopes(
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        labels.as_slice(),
        event.summary_type.as_str(),
        10_000,
    )
    .await?;
    for scope in scopes {
        let Some(scope_event) = subject_memory_scopes::rearm_subject_memory_dispatch(
            &state.pool,
            scope.tenant_id.as_str(),
            scope.source_id.as_str(),
            scope.scope_key.as_str(),
        )
        .await?
        else {
            continue;
        };
        if scope_event.subject_memory_dispatch_pending {
            publish_scope_outbox(&state.pool, &state.config, channel, &scope_event).await?;
        }
    }
    summaries::mark_subject_memory_source_dispatch_consumed(&state.pool, &event).await?;
    Ok(())
}

async fn process_scope_event(
    state: &Arc<AppState>,
    channel: &Channel,
    envelope: &SubjectMemoryEnvelope,
) -> Result<(), String> {
    let event = envelope.scope_outbox()?;
    let Some(dispatch_state) = subject_memory_scopes::get_subject_memory_dispatch_state(
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        event.scope_key.as_str(),
    )
    .await?
    else {
        return Ok(());
    };
    if dispatch_state.subject_memory_dispatch_consumed_version >= envelope.version {
        return Ok(());
    }

    let policy = control_plane::get_effective_job_policy(&state.pool, "subject_memory").await?;
    if !policy.enabled {
        subject_memory_scopes::mark_subject_memory_dispatch_consumed(&state.pool, &event).await?;
        return Ok(());
    }

    let Some(scope) = subject_memory_scopes::get_subject_memory_scope(
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        event.scope_key.as_str(),
    )
    .await?
    else {
        return Ok(());
    };
    if scope.status != "active"
        || !subject_memory::scope_has_pending_work(&state.pool, &scope).await?
    {
        subject_memory_scopes::mark_subject_memory_dispatch_consumed(&state.pool, &event).await?;
        return Ok(());
    }

    subject_memory::run_scope_once(&state.config, &state.pool, &scope).await?;
    subject_memory_scopes::mark_subject_memory_dispatch_consumed(&state.pool, &event).await?;
    if subject_memory::scope_has_pending_work(&state.pool, &scope).await? {
        if let Some(next) = subject_memory_scopes::rearm_subject_memory_dispatch(
            &state.pool,
            scope.tenant_id.as_str(),
            scope.source_id.as_str(),
            scope.scope_key.as_str(),
        )
        .await?
        {
            if next.subject_memory_dispatch_pending {
                publish_scope_outbox(&state.pool, &state.config, channel, &next).await?;
            }
        }
    }
    Ok(())
}

async fn mark_failed(db: &crate::db::Db, envelope: &SubjectMemoryEnvelope, error: &str) {
    match envelope.event_type.as_str() {
        SOURCE_AVAILABLE_EVENT => {
            if let Ok(event) = envelope.source_outbox() {
                let _ =
                    summaries::mark_subject_memory_source_dispatch_failed(db, &event, error).await;
            }
        }
        SCOPE_REQUESTED_EVENT => {
            if let Ok(event) = envelope.scope_outbox() {
                let _ =
                    subject_memory_scopes::mark_subject_memory_dispatch_failed(db, &event, error)
                        .await;
            }
        }
        _ => {}
    }
}

async fn mark_dead_lettered(
    db: &crate::db::Db,
    envelope: &SubjectMemoryEnvelope,
    error: &str,
) -> Result<(), String> {
    match envelope.event_type.as_str() {
        SOURCE_AVAILABLE_EVENT => {
            summaries::mark_subject_memory_source_dispatch_dead_lettered(
                db,
                &envelope.source_outbox()?,
                error,
            )
            .await?;
        }
        SCOPE_REQUESTED_EVENT => {
            subject_memory_scopes::mark_subject_memory_dispatch_dead_lettered(
                db,
                &envelope.scope_outbox()?,
                error,
            )
            .await?;
        }
        _ => {}
    }
    Ok(())
}

async fn publish_source_outbox(
    db: &crate::db::Db,
    config: &AppConfig,
    channel: &Channel,
    event: &summaries::SubjectMemorySourceDispatchOutbox,
) -> Result<(), String> {
    publish_envelope(
        channel,
        config,
        config.subject_memory_queue.as_str(),
        &SubjectMemoryEnvelope::from_source(event),
    )
    .await?;
    summaries::mark_subject_memory_source_dispatch_published(db, event).await?;
    Ok(())
}

async fn publish_scope_outbox(
    db: &crate::db::Db,
    config: &AppConfig,
    channel: &Channel,
    event: &subject_memory_scopes::SubjectMemoryScopeDispatchOutbox,
) -> Result<(), String> {
    publish_envelope(
        channel,
        config,
        config.subject_memory_queue.as_str(),
        &SubjectMemoryEnvelope::from_scope(event),
    )
    .await?;
    subject_memory_scopes::mark_subject_memory_dispatch_published(db, event).await?;
    Ok(())
}

async fn publish_envelope(
    channel: &Channel,
    config: &AppConfig,
    routing_key: &str,
    envelope: &SubjectMemoryEnvelope,
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
                        "memory-subject:{}:{}:{}:{}",
                        envelope.event_type,
                        envelope.message_identity(),
                        envelope.version,
                        envelope.attempt
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
            "RabbitMQ returned unroutable Memory Engine subject memory event for {routing_key}"
        )),
        Confirmation::Nack(_) => Err(format!(
            "RabbitMQ rejected Memory Engine subject memory event for {routing_key}"
        )),
        Confirmation::NotRequested => Err(
            "RabbitMQ publisher confirm was not enabled for Memory Engine subject memory event"
                .to_string(),
        ),
    }
}

include!("subject_memory_queue_part01.rs");
