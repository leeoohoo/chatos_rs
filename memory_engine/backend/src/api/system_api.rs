// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use chatos_queue_observability::{RabbitMqQueueRuntimeStats, RabbitMqQueueSpec};
use tokio::try_join;

use crate::models::{
    MemoryEnginePressureStats, MemoryEngineRoleStats, MemoryEngineSystemStatsResponse,
    MemoryEngineWorkerConfigStats,
};
use crate::repositories::{control_plane, observability};
use crate::state::AppState;

const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

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
    let rabbitmq_queues = rabbitmq_queue_stats(&state).await;
    let pressure = state.pressure.snapshot();

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
        pressure: MemoryEnginePressureStats {
            level: pressure.level.as_str(),
            active_summary_concurrency: pressure.active_summary_concurrency,
            reconcile_paused: pressure.reconcile_paused,
            refresh_interval_ms: pressure.refresh_interval.as_millis().min(u64::MAX as u128) as u64,
        },
        rabbitmq_queues,
        backlog,
        job_runs_last_24h,
    }))
}

pub async fn prometheus_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let stats = rabbitmq_queue_stats(&state).await;
    let mut body = chatos_queue_observability::render_prometheus_metrics("memory-engine", &stats);
    let pressure = state.pressure.snapshot();
    body.push_str(
        "# HELP chatos_memory_engine_pressure_level Authoritative platform pressure level applied by Memory Engine.\n\
# TYPE chatos_memory_engine_pressure_level gauge\n",
    );
    for level in ["normal", "elevated", "critical"] {
        body.push_str(
            format!(
                "chatos_memory_engine_pressure_level{{level=\"{level}\"}} {}\n",
                u8::from(pressure.level.as_str() == level)
            )
            .as_str(),
        );
    }
    body.push_str(
        "# HELP chatos_memory_engine_active_summary_concurrency Summary consumers enabled by the current pressure policy.\n\
# TYPE chatos_memory_engine_active_summary_concurrency gauge\n",
    );
    body.push_str(
        format!(
            "chatos_memory_engine_active_summary_concurrency {}\n",
            pressure.active_summary_concurrency
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_memory_engine_reconcile_paused Whether non-critical reconcile scanning is paused.\n\
# TYPE chatos_memory_engine_reconcile_paused gauge\n",
    );
    body.push_str(
        format!(
            "chatos_memory_engine_reconcile_paused {}\n",
            u8::from(pressure.reconcile_paused)
        )
        .as_str(),
    );
    ([(header::CONTENT_TYPE, PROMETHEUS_CONTENT_TYPE)], body)
}

async fn rabbitmq_queue_stats(state: &AppState) -> RabbitMqQueueRuntimeStats {
    state
        .rabbitmq_queue_inspector
        .inspect(&[
            RabbitMqQueueSpec::new("summary", state.config.summary_queue.as_str()),
            RabbitMqQueueSpec::new("summary_retry", state.config.summary_retry_queue.as_str()),
            RabbitMqQueueSpec::new(
                "summary_dead_letter",
                state.config.summary_dead_letter_queue.as_str(),
            ),
            RabbitMqQueueSpec::new("rollup", state.config.rollup_queue.as_str()),
            RabbitMqQueueSpec::new("rollup_retry", state.config.rollup_retry_queue.as_str()),
            RabbitMqQueueSpec::new(
                "rollup_dead_letter",
                state.config.rollup_dead_letter_queue.as_str(),
            ),
            RabbitMqQueueSpec::new("subject_memory", state.config.subject_memory_queue.as_str()),
            RabbitMqQueueSpec::new(
                "subject_memory_retry",
                state.config.subject_memory_retry_queue.as_str(),
            ),
            RabbitMqQueueSpec::new(
                "subject_memory_dead_letter",
                state.config.subject_memory_dead_letter_queue.as_str(),
            ),
        ])
        .await
}
