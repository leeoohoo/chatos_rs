use std::sync::Arc;
use std::time::Duration;

use tokio::time::{self, MissedTickBehavior};
use tracing::{info, warn};

use crate::repositories::control_plane;
use crate::services::control_plane as cp_service;
use crate::services::subject_memory;
use crate::state::AppState;

use super::summary_jobs;

pub fn start(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut ticker = time::interval(Duration::from_secs(state.config.worker_interval_secs));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        info!("[MEMORY-ENGINE-WORKER] started tick={}s", state.config.worker_interval_secs);

        loop {
            ticker.tick().await;

            let summary_policy =
                match control_plane::get_effective_job_policy(&state.pool, "summary").await {
                    Ok(policy) => policy,
                    Err(err) => {
                        warn!("[MEMORY-ENGINE-WORKER] load summary policy failed: {}", err);
                        continue;
                    }
                };
            if summary_policy.enabled {
                let limit = summary_policy
                    .max_threads_per_tick
                    .unwrap_or(state.config.worker_max_threads_per_tick)
                    .max(1);
                match summary_jobs::run_pending_thread_summaries_with_limit(
                    &state.pool,
                    &state.config,
                    None,
                    None,
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

            let rollup_policy =
                match control_plane::get_effective_job_policy(&state.pool, "rollup").await {
                    Ok(policy) => policy,
                    Err(err) => {
                        warn!("[MEMORY-ENGINE-WORKER] load rollup policy failed: {}", err);
                        continue;
                    }
                };
            if rollup_policy.enabled {
                let limit = rollup_policy
                    .max_threads_per_tick
                    .unwrap_or(state.config.worker_max_threads_per_tick)
                    .max(1);
                let rollup_settings = cp_service::build_rollup_settings_from_policy(&rollup_policy);
                match summary_jobs::run_pending_thread_rollups(
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

            let subject_memory_policy =
                match control_plane::get_effective_job_policy(&state.pool, "subject_memory").await {
                    Ok(policy) => policy,
                    Err(err) => {
                        warn!("[MEMORY-ENGINE-WORKER] load subject_memory policy failed: {}", err);
                        continue;
                    }
                };
            if subject_memory_policy.enabled {
                let limit = subject_memory_policy
                    .max_threads_per_tick
                    .unwrap_or(state.config.worker_max_threads_per_tick)
                    .max(1);
                match subject_memory::run_registered_subject_memory_scopes(
                    &state.config,
                    &state.pool,
                    None,
                    Some("memory_server"),
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
        }
    });
}
