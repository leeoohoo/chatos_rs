// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use tracing::{info, warn};

use crate::config::AppConfig;
use crate::models::now_rfc3339;
use crate::repositories::{control_plane, subject_memory_scopes, summaries, threads};
use crate::services::subject_memory;
use crate::state::AppState;

const SOURCE_AVAILABLE_EVENT: &str = "source_available";
const SCOPE_REQUESTED_EVENT: &str = "scope_requested";
const SUBJECT_MEMORY_CLAIM_LEASE_SECS: i64 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
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
    _config: &AppConfig,
    db: &crate::db::Db,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
) -> Result<bool, String> {
    Ok(summaries::get_pending_subject_memory_source_dispatch(
        db, tenant_id, source_id, summary_id,
    )
    .await?
    .is_some())
}

pub async fn publish_pending_scope(
    _config: &AppConfig,
    db: &crate::db::Db,
    tenant_id: &str,
    source_id: &str,
    scope_key: &str,
) -> Result<bool, String> {
    Ok(subject_memory_scopes::get_pending_subject_memory_dispatch(
        db, tenant_id, source_id, scope_key,
    )
    .await?
    .is_some())
}

pub async fn publish_rearmed_source_dispatch(
    _config: &AppConfig,
    _db: &crate::db::Db,
    _event: &summaries::SubjectMemorySourceDispatchOutbox,
) -> Result<(), String> {
    Ok(())
}

pub async fn publish_rearmed_scope_dispatch(
    _config: &AppConfig,
    _db: &crate::db::Db,
    _event: &subject_memory_scopes::SubjectMemoryScopeDispatchOutbox,
) -> Result<(), String> {
    Ok(())
}

pub async fn archive_subject_memory_source_dead_letter(
    _config: &AppConfig,
    _tenant_id: &str,
    _source_id: &str,
    _summary_id: &str,
    _version: i64,
    _scan_limit: usize,
) -> Result<bool, String> {
    Ok(true)
}

pub async fn archive_subject_memory_scope_dead_letter(
    _config: &AppConfig,
    _tenant_id: &str,
    _source_id: &str,
    _scope_key: &str,
    _version: i64,
    _scan_limit: usize,
) -> Result<bool, String> {
    Ok(true)
}

pub fn start(state: Arc<AppState>) {
    for worker_index in 0..state.config.worker_subject_memory_concurrency.max(1) {
        tokio::spawn(run_worker(state.clone(), worker_index));
    }
    tokio::spawn(run_reconciler(state));
}

async fn run_worker(state: Arc<AppState>, worker_index: usize) {
    let mut interval = tokio::time::interval(state.config.subject_memory_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match process_claimed_batch(&state).await {
            Ok(count) if count > 0 => info!(
                worker_index,
                processed_count = count,
                "Memory Engine processed subject-memory database dispatches"
            ),
            Ok(_) => {}
            Err(error) => warn!(
                worker_index,
                error,
                "Memory Engine subject-memory database worker failed"
            ),
        }
    }
}

async fn process_claimed_batch(state: &Arc<AppState>) -> Result<usize, String> {
    let mut envelopes = summaries::claim_pending_subject_memory_source_dispatches(
        &state.pool,
        state.config.subject_memory_outbox_batch_size,
    )
    .await?
    .iter()
    .map(SubjectMemoryEnvelope::from_source)
    .collect::<Vec<_>>();
    envelopes.extend(
        subject_memory_scopes::claim_pending_subject_memory_dispatches(
            &state.pool,
            state.config.subject_memory_outbox_batch_size,
        )
        .await?
        .iter()
        .map(SubjectMemoryEnvelope::from_scope),
    );

    let mut processed = 0usize;
    for envelope in envelopes {
        match process_event(state, &envelope).await {
            Ok(()) => processed = processed.saturating_add(1),
            Err(error)
                if error == crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED =>
            {
                processed = processed.saturating_add(1);
            }
            Err(error) => {
                mark_failed_and_defer(&state.pool, &envelope, error.as_str()).await?;
                warn!(
                    event_type = envelope.event_type,
                    event_id = envelope.message_identity(),
                    version = envelope.version,
                    retry_delay_ms = state.config.subject_memory_retry_delay.as_millis(),
                    error,
                    "Memory Engine subject-memory database dispatch was deferred"
                );
            }
        }
    }
    Ok(processed)
}

async fn process_event(
    state: &Arc<AppState>,
    envelope: &SubjectMemoryEnvelope,
) -> Result<(), String> {
    match envelope.event_type.as_str() {
        SOURCE_AVAILABLE_EVENT => process_source_event(state, envelope).await,
        SCOPE_REQUESTED_EVENT => process_scope_event(state, envelope).await,
        _ => Err(format!(
            "unsupported subject memory event type {}",
            envelope.event_type
        )),
    }
}

async fn process_source_event(
    state: &Arc<AppState>,
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
    let scopes = subject_memory_scopes::list_matching_active_subject_memory_scopes(
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        thread.labels.unwrap_or_default().as_slice(),
        event.summary_type.as_str(),
        10_000,
    )
    .await?;
    for scope in scopes {
        subject_memory_scopes::rearm_subject_memory_dispatch(
            &state.pool,
            scope.tenant_id.as_str(),
            scope.source_id.as_str(),
            scope.scope_key.as_str(),
        )
        .await?;
    }
    summaries::mark_subject_memory_source_dispatch_consumed(&state.pool, &event).await?;
    Ok(())
}

async fn process_scope_event(
    state: &Arc<AppState>,
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
        subject_memory_scopes::rearm_subject_memory_dispatch(
            &state.pool,
            scope.tenant_id.as_str(),
            scope.source_id.as_str(),
            scope.scope_key.as_str(),
        )
        .await?;
    }
    Ok(())
}

async fn mark_failed_and_defer(
    db: &crate::db::Db,
    envelope: &SubjectMemoryEnvelope,
    error: &str,
) -> Result<(), String> {
    match envelope.event_type.as_str() {
        SOURCE_AVAILABLE_EVENT => {
            let event = envelope.source_outbox()?;
            summaries::mark_subject_memory_source_dispatch_failed(db, &event, error).await?;
            summaries::defer_subject_memory_source_dispatch(db, &event).await?;
        }
        SCOPE_REQUESTED_EVENT => {
            let event = envelope.scope_outbox()?;
            subject_memory_scopes::mark_subject_memory_dispatch_failed(db, &event, error).await?;
            subject_memory_scopes::defer_subject_memory_dispatch(db, &event).await?;
        }
        _ => {}
    }
    Ok(())
}

async fn run_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(state.config.subject_memory_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut recovery_offset = 0_u64;
    loop {
        interval.tick().await;
        let stale_before = (chrono::Utc::now()
            - chrono::Duration::seconds(SUBJECT_MEMORY_CLAIM_LEASE_SECS))
        .to_rfc3339();
        let source_recovery = summaries::recover_stale_subject_memory_source_dispatches(
            &state.pool,
            stale_before.as_str(),
            state.config.subject_memory_outbox_batch_size,
        )
        .await;
        let scope_recovery = subject_memory_scopes::recover_stale_subject_memory_dispatches(
            &state.pool,
            stale_before.as_str(),
            state.config.subject_memory_outbox_batch_size,
        )
        .await;
        if let Err(error) = source_recovery {
            warn!(error, "Memory Engine failed to recover subject source database claims");
        }
        if let Err(error) = scope_recovery {
            warn!(error, "Memory Engine failed to recover subject scope database claims");
        }
        match arm_pending_scopes(&state, recovery_offset).await {
            Ok((count, next_offset)) => {
                recovery_offset = next_offset;
                if count > 0 {
                    info!(armed_count = count, "Memory Engine armed subject-memory scopes");
                }
            }
            Err(error) => warn!(error, "Memory Engine failed to arm subject-memory scopes"),
        }
    }
}

async fn arm_pending_scopes(state: &AppState, recovery_offset: u64) -> Result<(usize, u64), String> {
    let policy = control_plane::get_effective_job_policy(&state.pool, "subject_memory").await?;
    if !policy.enabled {
        return Ok((0, recovery_offset));
    }
    let scopes = subject_memory_scopes::list_active_subject_memory_scopes_page(
        &state.pool,
        None,
        None,
        state.config.subject_memory_outbox_batch_size,
        recovery_offset,
    )
    .await?;
    let scanned_count = scopes.len() as u64;
    let mut armed = 0usize;
    for scope in scopes {
        if !subject_memory::scope_has_pending_work(&state.pool, &scope).await? {
            continue;
        }
        if subject_memory_scopes::rearm_subject_memory_dispatch(
            &state.pool,
            scope.tenant_id.as_str(),
            scope.source_id.as_str(),
            scope.scope_key.as_str(),
        )
        .await?
        .is_some()
        {
            armed = armed.saturating_add(1);
        }
    }
    let batch_size = state.config.subject_memory_outbox_batch_size.max(1) as u64;
    let next_offset = if scanned_count < batch_size {
        0
    } else {
        recovery_offset.saturating_add(scanned_count)
    };
    Ok((armed, next_offset))
}

#[cfg(test)]
include!("subject_memory_queue_inline_tests.rs");
