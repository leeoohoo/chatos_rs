// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use futures_util::{stream, StreamExt};
use tokio::join;
use tokio::time::{self, MissedTickBehavior};
use tracing::{info, warn};

use crate::repositories::control_plane;
use crate::repositories::threads;
use crate::services::control_plane as cp_service;
use crate::services::subject_memory;
use crate::state::AppState;

use super::summary_jobs;

pub fn start(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut ticker = time::interval(Duration::from_secs(state.config.worker_interval_secs));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        info!(
            "[MEMORY-ENGINE-WORKER] started tick={}s summary_concurrency={} rollup_concurrency={} subject_memory_concurrency={} reconcile_concurrency={}",
            state.config.worker_interval_secs,
            state.config.worker_summary_concurrency,
            state.config.worker_rollup_concurrency,
            state.config.worker_subject_memory_concurrency,
            state.config.worker_reconcile_concurrency,
        );

        loop {
            ticker.tick().await;

            if let Err(err) = control_plane::fail_stale_running_job_runs(&state.pool, 300).await {
                warn!("[MEMORY-ENGINE-WORKER] stale job cleanup failed: {}", err);
            }

            let summary_task = run_summary_tick(&state);
            let rollup_task = run_rollup_tick(&state);
            let subject_memory_task = run_subject_memory_tick(&state);
            let reconcile_task = run_pending_queue_reconcile_tick(&state);
            let _ = join!(
                summary_task,
                rollup_task,
                subject_memory_task,
                reconcile_task
            );
        }
    });
}

async fn run_summary_tick(state: &Arc<AppState>) {
    let summary_policy = match control_plane::get_effective_job_policy(&state.pool, "summary").await
    {
        Ok(policy) => policy,
        Err(err) => {
            warn!("[MEMORY-ENGINE-WORKER] load summary policy failed: {}", err);
            return;
        }
    };
    if !summary_policy.enabled {
        return;
    }

    let limit = summary_policy
        .max_threads_per_tick
        .unwrap_or(state.config.worker_max_threads_per_tick)
        .max(1);
    match summary_jobs::run_pending_thread_summaries_due(
        &state.pool,
        &state.config,
        None,
        None,
        summary_policy.token_limit.unwrap_or(6000).max(128),
        limit,
    )
    .await
    {
        Ok(result) if result.summarized_threads > 0 || result.processed_threads > 0 => {
            info!(
                "[MEMORY-ENGINE-WORKER] summary processed_threads={} summarized_threads={} limit={}",
                result.processed_threads, result.summarized_threads, limit
            );
        }
        Ok(_) => {}
        Err(err) => warn!("[MEMORY-ENGINE-WORKER] summary tick failed: {}", err),
    }
}

async fn run_rollup_tick(state: &Arc<AppState>) {
    let rollup_policy = match control_plane::get_effective_job_policy(&state.pool, "rollup").await {
        Ok(policy) => policy,
        Err(err) => {
            warn!("[MEMORY-ENGINE-WORKER] load rollup policy failed: {}", err);
            return;
        }
    };
    if !rollup_policy.enabled {
        return;
    }

    let limit = rollup_policy
        .max_threads_per_tick
        .unwrap_or(state.config.worker_max_threads_per_tick)
        .max(1);
    let rollup_settings = cp_service::build_rollup_settings_from_policy(&rollup_policy);
    match summary_jobs::run_pending_thread_rollups_due(
        &state.pool,
        &state.config,
        None,
        None,
        limit,
        &rollup_settings,
    )
    .await
    {
        Ok(result)
            if result.generated_summaries > 0
                || result.marked_summaries > 0
                || result.processed_threads > 0 =>
        {
            info!(
                "[MEMORY-ENGINE-WORKER] rollup processed_threads={} rolled_up_threads={} generated_summaries={} marked_summaries={} failed_threads={} limit={}",
                result.processed_threads,
                result.rolled_up_threads,
                result.generated_summaries,
                result.marked_summaries,
                result.failed_threads,
                limit
            );
        }
        Ok(_) => {}
        Err(err) => warn!("[MEMORY-ENGINE-WORKER] rollup tick failed: {}", err),
    }
}

async fn run_subject_memory_tick(state: &Arc<AppState>) {
    let subject_memory_policy =
        match control_plane::get_effective_job_policy(&state.pool, "subject_memory").await {
            Ok(policy) => policy,
            Err(err) => {
                warn!(
                    "[MEMORY-ENGINE-WORKER] load subject_memory policy failed: {}",
                    err
                );
                return;
            }
        };
    if !subject_memory_policy.enabled {
        return;
    }

    let limit = subject_memory_policy
        .max_threads_per_tick
        .unwrap_or(state.config.worker_max_threads_per_tick)
        .max(1);
    match subject_memory::run_registered_subject_memory_scopes_due(
        &state.config,
        &state.pool,
        None,
        None,
        limit,
    )
    .await
    {
        Ok(result)
            if result.generated_memories > 0
                || result.marked_source_memories > 0
                || result.marked_source_summaries > 0
                || result.failed_scopes > 0 =>
        {
            info!(
                "[MEMORY-ENGINE-WORKER] subject_memory processed_scopes={} generated_scopes={} generated_memories={} marked_summaries={} marked_memories={} failed_scopes={} limit={}",
                result.processed_scopes,
                result.generated_scopes,
                result.generated_memories,
                result.marked_source_summaries,
                result.marked_source_memories,
                result.failed_scopes,
                limit
            );
        }
        Ok(_) => {}
        Err(err) => warn!("[MEMORY-ENGINE-WORKER] subject_memory tick failed: {}", err),
    }
}

async fn run_pending_queue_reconcile_tick(state: &Arc<AppState>) {
    let summary_policy = match control_plane::get_effective_job_policy(&state.pool, "summary").await
    {
        Ok(policy) => policy,
        Err(err) => {
            warn!(
                "[MEMORY-ENGINE-WORKER] load summary policy for reconcile failed: {}",
                err
            );
            return;
        }
    };
    if !summary_policy.enabled {
        return;
    }

    let limit = summary_policy
        .max_threads_per_tick
        .unwrap_or(state.config.worker_max_threads_per_tick)
        .max(1)
        .min(5);
    let candidates = match threads::list_threads_with_pending_records_by_token_threshold(
        &state.pool,
        None,
        None,
        1,
        limit,
    )
    .await
    {
        Ok(items) => items,
        Err(err) => {
            warn!(
                "[MEMORY-ENGINE-WORKER] load pending queue reconcile candidates failed: {}",
                err
            );
            return;
        }
    };
    if candidates.is_empty() {
        return;
    }

    let pool = state.pool.clone();
    let concurrency = pending_queue_reconcile_concurrency(&state.config, limit);
    let results = stream::iter(candidates.into_iter().map(|thread| {
        let pool = pool.clone();
        async move {
            let thread_id = thread.id.clone();
            let result = threads::refresh_summary_queue_state(
                &pool,
                thread.tenant_id.as_str(),
                thread.source_id.as_str(),
                thread.id.as_str(),
            )
            .await;
            (thread_id, result)
        }
    }))
    .buffer_unordered(concurrency)
    .collect::<Vec<_>>()
    .await;

    for (thread_id, result) in results {
        if let Err(err) = result {
            warn!(
                "[MEMORY-ENGINE-WORKER] pending queue reconcile failed thread_id={} error={}",
                thread_id, err
            );
        }
    }
}

fn pending_queue_reconcile_concurrency(config: &crate::config::AppConfig, limit: i64) -> usize {
    limit
        .max(1)
        .min(config.worker_reconcile_concurrency.max(1) as i64) as usize
}
