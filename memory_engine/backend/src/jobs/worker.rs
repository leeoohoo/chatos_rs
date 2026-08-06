// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use futures_util::{stream, StreamExt};
use tokio::time::{self, MissedTickBehavior};
use tracing::{info, warn};

use crate::repositories::control_plane;
use crate::repositories::{records, threads};
use crate::state::AppState;

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
            let tick_started_at = time::Instant::now();

            if let Err(err) = control_plane::fail_stale_running_job_runs(&state.pool, 300).await {
                warn!("[MEMORY-ENGINE-WORKER] stale job cleanup failed: {}", err);
            }

            run_pending_queue_reconcile_tick(&state).await;
            state
                .runtime_stats
                .record_worker_tick(tick_started_at.elapsed());
        }
    });
}

async fn run_pending_queue_reconcile_tick(state: &Arc<AppState>) {
    if state.pressure.snapshot().reconcile_paused {
        return;
    }
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
        .clamp(1, 5);
    let candidates = match records::list_pending_record_thread_scopes(&state.pool, limit).await {
        Ok(items) => items,
        Err(err) => {
            warn!(
                "[MEMORY-ENGINE-WORKER] load pending queue reconcile candidates failed: {}",
                err
            );
            return;
        }
    };
    let pool = state.pool.clone();
    let concurrency = pending_queue_reconcile_concurrency(&state.config, limit);
    let token_threshold = summary_policy.token_limit.unwrap_or(6000).max(128);
    let results = stream::iter(candidates.into_iter().map(|scope| {
        let pool = pool.clone();
        async move {
            let thread_id = scope.thread_id.clone();
            let result = async {
                threads::refresh_summary_queue_state(
                    &pool,
                    scope.tenant_id.as_str(),
                    scope.source_id.as_str(),
                    scope.thread_id.as_str(),
                )
                .await?;
                threads::rearm_summary_dispatch_if_eligible(
                    &pool,
                    scope.tenant_id.as_str(),
                    scope.source_id.as_str(),
                    scope.thread_id.as_str(),
                    token_threshold,
                )
                .await?;
                Ok::<(), String>(())
            }
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
                "[MEMORY-ENGINE-WORKER] summary recovery reconcile failed thread_id={} error={}",
                thread_id, err
            );
        }
    }

    let eligible_threads = match threads::list_threads_with_pending_records_by_token_threshold(
        &state.pool,
        None,
        None,
        token_threshold,
        limit,
    )
    .await
    {
        Ok(items) => items,
        Err(err) => {
            warn!(
                "[MEMORY-ENGINE-WORKER] load summary dispatch recovery candidates failed: {}",
                err
            );
            return;
        }
    };
    let pool = state.pool.clone();
    let results = stream::iter(eligible_threads.into_iter().map(|thread| {
        let pool = pool.clone();
        async move {
            let thread_id = thread.id.clone();
            let result = threads::rearm_summary_dispatch_if_eligible(
                &pool,
                thread.tenant_id.as_str(),
                thread.source_id.as_str(),
                thread.id.as_str(),
                token_threshold,
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
                "[MEMORY-ENGINE-WORKER] summary dispatch recovery failed thread_id={} error={}",
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
