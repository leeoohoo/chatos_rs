// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use tracing::{info, warn};

use crate::models::now_rfc3339;
use crate::repositories::{control_plane, threads};
use crate::services::summary;
use crate::state::AppState;

const SUMMARY_QUEUE_TRIGGER: &str = "database_worker";
const SUMMARY_DISPATCH_CLAIM_LEASE_SECS: i64 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
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

pub async fn publish_rearmed_summary_dispatch(
    _state: &AppState,
    _event: &threads::SummaryDispatchOutbox,
) -> Result<(), String> {
    Ok(())
}

pub async fn archive_summary_dead_letter(
    _config: &crate::config::AppConfig,
    _tenant_id: &str,
    _source_id: &str,
    _thread_id: &str,
    _version: i64,
    _scan_limit: usize,
) -> Result<bool, String> {
    // Dead letters are database state now. Replay clears the old marker atomically.
    Ok(true)
}

pub async fn dead_letter_current_summary_dispatch(
    state: &AppState,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    error: &str,
) -> Result<bool, String> {
    let Some(event) =
        threads::get_summary_dispatch_state(&state.pool, tenant_id, source_id, thread_id).await?
    else {
        return Ok(false);
    };
    if event.summary_dispatch_version <= 0
        || event.summary_dispatch_consumed_version >= event.summary_dispatch_version
    {
        return Ok(false);
    }
    threads::mark_summary_dispatch_dead_lettered(&state.pool, &event, error).await?;
    warn!(
        thread_id,
        version = event.summary_dispatch_version,
        error,
        "Memory Engine summary database dispatch was dead-lettered"
    );
    Ok(true)
}

pub fn start(state: Arc<AppState>) {
    for worker_index in 0..state.config.worker_summary_concurrency.max(1) {
        tokio::spawn(run_worker(state.clone(), worker_index));
    }
    tokio::spawn(run_reconciler(state));
}

async fn run_worker(state: Arc<AppState>, worker_index: usize) {
    let mut pressure = state.pressure.subscribe();
    let mut interval = tokio::time::interval(state.config.summary_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        if wait_until_consumer_enabled(&mut pressure, worker_index)
            .await
            .is_err()
        {
            return;
        }
        interval.tick().await;
        if !summary_consumer_enabled(&pressure.borrow(), worker_index) {
            continue;
        }
        match process_claimed_batch(&state).await {
            Ok(count) if count > 0 => info!(
                worker_index,
                processed_count = count,
                "Memory Engine processed summary database dispatches"
            ),
            Ok(_) => {}
            Err(error) => warn!(
                worker_index,
                error,
                "Memory Engine summary database worker failed"
            ),
        }
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

async fn process_claimed_batch(state: &Arc<AppState>) -> Result<usize, String> {
    let events = threads::claim_pending_summary_dispatches(
        &state.pool,
        state.config.summary_outbox_batch_size,
    )
    .await?;
    let mut processed = 0usize;
    for event in events {
        let envelope = SummaryRequestedEnvelope::from_outbox(&event);
        match process_summary_event(state, &envelope).await {
            Ok(()) => processed = processed.saturating_add(1),
            Err(error)
                if error == crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED =>
            {
                processed = processed.saturating_add(1);
            }
            Err(error) => {
                threads::mark_summary_dispatch_failed(&state.pool, &event, error.as_str()).await?;
                threads::defer_summary_dispatch_until_unlock(&state.pool, &event).await?;
                warn!(
                    thread_id = event.thread_id,
                    version = event.summary_dispatch_version,
                    error,
                    retry_delay_ms = state.config.summary_retry_delay.as_millis(),
                    "Memory Engine summary database dispatch was deferred"
                );
            }
        }
    }
    Ok(processed)
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

    let token_threshold = summary::required_thread_summary_token_limit(policy.token_limit)?;
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
    if summary_slot_is_active(&thread, now_rfc3339().as_str()) {
        threads::defer_summary_dispatch_until_unlock(&state.pool, &event).await?;
        return Ok(());
    }
    if thread.pending_summary_tokens < token_threshold {
        threads::mark_summary_dispatch_consumed(&state.pool, &event).await?;
        return Ok(());
    }

    let run_response = match summary::run_thread_summary_with_thread(
        &state.config,
        &state.pool,
        thread,
        SUMMARY_QUEUE_TRIGGER,
    )
    .await
    {
        Ok(response) => response,
        Err(error) if error.contains("summary slot already occupied") => {
            return Err(
                crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED.to_string(),
            );
        }
        Err(error) => return Err(error),
    };

    if !run_response.generated {
        threads::refresh_summary_queue_state(
            &state.pool,
            envelope.tenant_id.as_str(),
            envelope.source_id.as_str(),
            envelope.thread_id.as_str(),
        )
        .await?;
    }
    threads::mark_summary_dispatch_consumed(&state.pool, &event).await?;
    let _ = threads::rearm_summary_dispatch_if_eligible(
        &state.pool,
        envelope.tenant_id.as_str(),
        envelope.source_id.as_str(),
        envelope.thread_id.as_str(),
        token_threshold,
    )
    .await?;
    Ok(())
}

fn summary_slot_is_active(thread: &crate::models::EngineThread, now: &str) -> bool {
    thread.summary_status == "running"
        && thread
            .summary_lock_expires_at
            .as_deref()
            .is_some_and(|expires_at| expires_at > now)
}

async fn run_reconciler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(state.config.summary_outbox_reconcile_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match recover_stale_claims(&state).await {
            Ok(count) if count > 0 => info!(
                recovered_count = count,
                lease_seconds = SUMMARY_DISPATCH_CLAIM_LEASE_SECS,
                "Memory Engine recovered stale summary database claims"
            ),
            Ok(_) => {}
            Err(error) => warn!(error, "Memory Engine failed to recover summary database claims"),
        }
        match arm_automatic_summary_dispatches(&state).await {
            Ok(count) if count > 0 => info!(
                armed_count = count,
                "Memory Engine armed automatic summary database dispatches"
            ),
            Ok(_) => {}
            Err(error) => warn!(error, "Memory Engine failed to arm summary database dispatches"),
        }
    }
}

async fn recover_stale_claims(state: &AppState) -> Result<usize, String> {
    let policy = control_plane::get_effective_job_policy(&state.pool, "summary").await?;
    if !policy.enabled {
        return Ok(0);
    }
    let token_threshold = summary::required_thread_summary_token_limit(policy.token_limit)?;
    let stale_before = (chrono::Utc::now()
        - chrono::Duration::seconds(SUMMARY_DISPATCH_CLAIM_LEASE_SECS))
    .to_rfc3339();
    let candidates = threads::list_stale_published_summary_dispatches(
        &state.pool,
        token_threshold,
        stale_before.as_str(),
        state.config.summary_outbox_batch_size,
    )
    .await?;
    let mut recovered = 0usize;
    for candidate in candidates {
        if threads::rearm_stale_published_summary_dispatch(
            &state.pool,
            &candidate,
            token_threshold,
            stale_before.as_str(),
        )
        .await?
        .is_some()
        {
            recovered = recovered.saturating_add(1);
        }
    }
    Ok(recovered)
}

async fn arm_automatic_summary_dispatches(state: &AppState) -> Result<usize, String> {
    let policy = control_plane::get_effective_job_policy(&state.pool, "summary").await?;
    if !policy.enabled {
        return Ok(0);
    }
    let token_threshold = summary::required_thread_summary_token_limit(policy.token_limit)?;
    let candidates = threads::list_eligible_summary_dispatches(
        &state.pool,
        token_threshold,
        state.config.summary_outbox_batch_size,
    )
    .await?;
    let mut armed = 0usize;
    for candidate in candidates {
        if threads::rearm_summary_dispatch_if_eligible(
            &state.pool,
            candidate.tenant_id.as_str(),
            candidate.source_id.as_str(),
            candidate.thread_id.as_str(),
            token_threshold,
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
include!("summary_queue_inline_tests.rs");
