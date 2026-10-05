// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use tracing::{info, warn};

use crate::config::AppConfig;
use crate::models::now_rfc3339;
use crate::repositories::{control_plane, summaries};
use crate::services::{control_plane as cp_service, summary};
use crate::state::AppState;

const ROLLUP_QUEUE_TRIGGER: &str = "database_worker";
const ROLLUP_DISPATCH_CLAIM_LEASE_SECS: i64 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
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
    _config: &AppConfig,
    db: &crate::db::Db,
    tenant_id: &str,
    source_id: &str,
    summary_id: &str,
) -> Result<bool, String> {
    Ok(
        summaries::get_pending_rollup_dispatch(db, tenant_id, source_id, summary_id)
            .await?
            .is_some(),
    )
}

pub async fn publish_rearmed_rollup_dispatch(
    _state: &AppState,
    _event: &summaries::RollupDispatchOutbox,
) -> Result<(), String> {
    Ok(())
}

pub async fn archive_rollup_dead_letter(
    _config: &AppConfig,
    _tenant_id: &str,
    _source_id: &str,
    _summary_id: &str,
    _version: i64,
    _scan_limit: usize,
) -> Result<bool, String> {
    Ok(true)
}

pub fn start(state: Arc<AppState>) {
    for worker_index in 0..state.config.worker_rollup_concurrency.max(1) {
        tokio::spawn(run_worker(state.clone(), worker_index));
    }
    tokio::spawn(run_reconciler(state));
}

async fn run_worker(state: Arc<AppState>, worker_index: usize) {
    let mut interval = tokio::time::interval(state.config.rollup_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match process_claimed_batch(&state).await {
            Ok(count) if count > 0 => info!(
                worker_index,
                processed_count = count,
                "Memory Engine processed rollup database dispatches"
            ),
            Ok(_) => {}
            Err(error) => warn!(
                worker_index,
                error, "Memory Engine rollup database worker failed"
            ),
        }
    }
}

async fn process_claimed_batch(state: &Arc<AppState>) -> Result<usize, String> {
    let events = summaries::claim_pending_rollup_dispatches(
        &state.pool,
        state.config.rollup_outbox_batch_size,
    )
    .await?;
    let mut processed = 0usize;
    for event in events {
        let envelope = RollupRequestedEnvelope::from_outbox(&event);
        match process_rollup_event(state, &envelope).await {
            Ok(()) => processed = processed.saturating_add(1),
            Err(error)
                if error == crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED =>
            {
                processed = processed.saturating_add(1);
            }
            Err(error) => {
                summaries::mark_rollup_dispatch_failed(&state.pool, &event, error.as_str()).await?;
                summaries::defer_rollup_dispatch(&state.pool, &event).await?;
                warn!(
                    summary_id = event.id,
                    version = event.rollup_dispatch_version,
                    retry_delay_ms = state.config.rollup_retry_delay.as_millis(),
                    error,
                    "Memory Engine rollup database dispatch was deferred"
                );
            }
        }
    }
    Ok(processed)
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
        summaries::rearm_rollup_dispatch_if_eligible(
            &state.pool,
            envelope.tenant_id.as_str(),
            envelope.source_id.as_str(),
            envelope.thread_id.as_str(),
            settings.max_level,
        )
        .await?;
    }
    Ok(())
}

async fn run_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(state.config.rollup_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let stale_before = (chrono::Utc::now()
            - chrono::Duration::seconds(ROLLUP_DISPATCH_CLAIM_LEASE_SECS))
        .to_rfc3339();
        match summaries::recover_stale_rollup_dispatches(
            &state.pool,
            stale_before.as_str(),
            state.config.rollup_outbox_batch_size,
        )
        .await
        {
            Ok(count) if count > 0 => info!(
                recovered_count = count,
                "Memory Engine recovered stale rollup database claims"
            ),
            Ok(_) => {}
            Err(error) => warn!(
                error,
                "Memory Engine failed to recover rollup database claims"
            ),
        }
        if let Err(error) = arm_rollup_dispatches(&state).await {
            warn!(
                error,
                "Memory Engine failed to arm rollup database dispatches"
            );
        }
    }
}

async fn arm_rollup_dispatches(state: &AppState) -> Result<usize, String> {
    let policy = control_plane::get_effective_job_policy(&state.pool, "rollup").await?;
    if !policy.enabled {
        return Ok(0);
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
    let mut armed = 0usize;
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
        if summaries::rearm_rollup_dispatch_if_eligible(
            &state.pool,
            tenant_id.as_str(),
            source_id.as_str(),
            thread_id.as_str(),
            settings.max_level,
        )
        .await?
        .is_some()
        {
            armed = armed.saturating_add(1);
        }
    }
    Ok(armed)
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
