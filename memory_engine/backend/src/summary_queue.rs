// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use tracing::{info, warn};

use crate::repositories::{control_plane, thread_partitions, threads};
use crate::services::summary;
use crate::state::AppState;

const SUMMARY_QUEUE_TRIGGER: &str = "database_worker";

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
                error, "Memory Engine summary database worker failed"
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
    let events = thread_partitions::list_ready_summary_dispatches(
        &state.pool,
        state.config.summary_outbox_batch_size,
    )
    .await?;
    let mut processed = 0usize;
    for event in events {
        match process_partition_summary_event(state, &event).await {
            Ok(()) => processed = processed.saturating_add(1),
            Err(error)
                if error == crate::services::memory_cloud_agent::MEMORY_CLOUD_AGENT_DEFERRED =>
            {
                processed = processed.saturating_add(1);
            }
            Err(error) => {
                thread_partitions::mark_summary_dispatch_failed(
                    &state.pool,
                    &event,
                    "summary_worker_error",
                )
                .await?;
                warn!(
                    thread_id = event.thread_id,
                    project_partition_key = event.project_partition_key,
                    error,
                    retry_delay_ms = state.config.summary_retry_delay.as_millis(),
                    "Memory Engine summary database dispatch was deferred"
                );
            }
        }
    }
    Ok(processed)
}

async fn process_partition_summary_event(
    state: &Arc<AppState>,
    event: &thread_partitions::PartitionSummaryDispatch,
) -> Result<(), String> {
    let policy = control_plane::get_effective_job_policy(&state.pool, "summary").await?;
    let token_threshold = policy
        .dispatch_threshold_tokens
        .ok_or_else(|| "summary dispatch_threshold_tokens is missing".to_string())?;
    if !event.forced() && event.pending_summary_tokens < token_threshold {
        thread_partitions::consume_summary_dispatch(&state.pool, event).await?;
        return Ok(());
    }
    let thread = threads::get_thread_by_id(
        &state.pool,
        event.tenant_id.as_str(),
        event.source_id.as_str(),
        event.thread_id.as_str(),
    )
    .await?
    .ok_or_else(|| "partition summary thread is missing".to_string())?;
    let result = summary::run_thread_summary_partition_with_thread(
        &state.config,
        &state.pool,
        thread,
        event.project_partition_key.as_str(),
        SUMMARY_QUEUE_TRIGGER,
    )
    .await;
    match result {
        Ok(_) => {
            thread_partitions::consume_summary_dispatch(&state.pool, event).await?;
            Ok(())
        }
        Err(error) => Err(error),
    }
}

async fn run_reconciler(state: Arc<AppState>) {
    loop {
        match release_expired_summary_slots(&state).await {
            Ok(count) if count > 0 => info!(
                recovered_count = count,
                "Memory Engine released expired summary thread slots"
            ),
            Ok(_) => {}
            Err(error) => warn!(
                error,
                "Memory Engine failed to release expired summary thread slots"
            ),
        }
        tokio::time::sleep(state.config.summary_outbox_reconcile_interval).await;
    }
}

async fn release_expired_summary_slots(state: &AppState) -> Result<usize, String> {
    thread_partitions::release_expired_summary_claims(
        &state.pool,
        state.config.summary_outbox_batch_size,
    )
    .await
    .map(|count| usize::try_from(count).unwrap_or(usize::MAX))
}

#[cfg(test)]
include!("summary_queue_inline_tests.rs");
