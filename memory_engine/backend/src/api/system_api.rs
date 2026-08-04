// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use tokio::try_join;

use crate::models::{
    MemoryEngineRoleStats, MemoryEngineSystemStatsResponse, MemoryEngineWorkerConfigStats,
};
use crate::repositories::{control_plane, observability};
use crate::state::AppState;

pub async fn system_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<MemoryEngineSystemStatsResponse>, (axum::http::StatusCode, String)> {
    let (backlog, job_runs_last_24h) = try_join!(
        observability::system_backlog_stats(&state.pool),
        control_plane::job_run_stats(&state.pool, None, None, None, 24),
    )
    .map_err(|err| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("load Memory Engine system stats failed: {err}"),
        )
    })?;

    Ok(Json(MemoryEngineSystemStatsResponse {
        ok: true,
        service: "memory-engine",
        now: chrono::Utc::now().to_rfc3339(),
        roles: MemoryEngineRoleStats {
            api_enabled: state.config.api_enabled,
            worker_enabled: state.config.worker_enabled,
        },
        worker_config: MemoryEngineWorkerConfigStats {
            interval_secs: state.config.worker_interval_secs,
            max_threads_per_tick: state.config.worker_max_threads_per_tick,
            summary_concurrency: state.config.worker_summary_concurrency,
            rollup_concurrency: state.config.worker_rollup_concurrency,
            subject_memory_concurrency: state.config.worker_subject_memory_concurrency,
            reconcile_concurrency: state.config.worker_reconcile_concurrency,
        },
        worker_runtime: state.runtime_stats.snapshot(),
        backlog,
        job_runs_last_24h,
    }))
}
