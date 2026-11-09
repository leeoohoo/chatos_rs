// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
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
        compose_baseline: state.runtime_stats.compose_baseline(),
        pressure: MemoryEnginePressureStats {
            level: pressure.level.as_str(),
            active_summary_concurrency: pressure.active_summary_concurrency,
            reconcile_paused: pressure.reconcile_paused,
            refresh_interval_ms: pressure.refresh_interval.as_millis().min(u64::MAX as u128) as u64,
        },
        dispatch_backend: "postgres",
        backlog,
        job_runs_last_24h,
    }))
}

pub async fn prometheus_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut body = String::new();
    body.push_str(&chatos_postgres::render_pool_metrics(
        &state.pool,
        "memory-engine-api",
    ));
    let pressure = state.pressure.snapshot();
    body.push_str(
        "# HELP chatos_memory_engine_pressure_level Authoritative platform pressure level applied by Memory Engine.\n\
# TYPE chatos_memory_engine_pressure_level gauge\n",
    );
    let compose = state.runtime_stats.compose_baseline();
    body.push_str(
        "# HELP chatos_memory_compose_requests_total Compose context responses produced by this process.\n\
# TYPE chatos_memory_compose_requests_total counter\n",
    );
    body.push_str(
        format!(
            "chatos_memory_compose_requests_total {}\n",
            compose.requests_total
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_memory_compose_estimated_tokens_total Conservative estimated tokens returned by compose context.\n\
# TYPE chatos_memory_compose_estimated_tokens_total counter\n",
    );
    body.push_str(
        format!(
            "chatos_memory_compose_estimated_tokens_total {}\n",
            compose.estimated_tokens_total
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_memory_compose_estimated_tokens_max Largest conservative compose token estimate observed by this process.\n\
# TYPE chatos_memory_compose_estimated_tokens_max gauge\n",
    );
    body.push_str(
        format!(
            "chatos_memory_compose_estimated_tokens_max {}\n",
            compose.estimated_tokens_max
        )
        .as_str(),
    );
    body.push_str(
        "# HELP chatos_memory_reliability_audit_success Whether the latest reliability audit completed.\n\
# TYPE chatos_memory_reliability_audit_success gauge\n\
# HELP chatos_memory_reliability_findings Current reliability findings grouped by invariant and severity.\n\
# TYPE chatos_memory_reliability_findings gauge\n\
# HELP chatos_memory_reliability_backlog Current durable Memory Engine reliability backlog.\n\
# TYPE chatos_memory_reliability_backlog gauge\n",
    );
    match crate::reliability_audit::run_reliability_audit(
        &state.pool,
        crate::reliability_audit::ReliabilityAuditScope {
            tenant_id: None,
            source_id: None,
        },
    )
    .await
    {
        Ok(report) => {
            body.push_str("chatos_memory_reliability_audit_success 1\n");
            for alert in report.alerts {
                body.push_str(
                    format!(
                        "chatos_memory_reliability_findings{{kind=\"{}\",severity=\"{}\"}} {}\n",
                        alert.code, alert.severity, alert.count
                    )
                    .as_str(),
                );
            }
            for (kind, value) in [
                ("pending_records", report.baseline.pending_records),
                ("pending_tokens", report.baseline.pending_tokens),
                ("summary_dispatch_pending", report.baseline.summary_dispatch_pending),
                ("summary_dispatch_dead_lettered", report.baseline.summary_dispatch_dead_lettered),
                ("rollup_inputs_pending", report.baseline.rollup_inputs_pending),
                ("rollup_dispatch_pending", report.baseline.rollup_dispatch_pending),
                ("subject_memory_source_dispatch_pending", report.baseline.subject_memory_source_dispatch_pending),
                ("subject_memory_rollup_pending", report.baseline.subject_memory_rollup_pending),
                ("running_jobs", report.baseline.running_jobs),
                ("failed_jobs_last_24h", report.baseline.failed_jobs_last_24h),
                ("provider_overflow_outputs", report.baseline.outputs_with_provider_overflow_retry),
            ] {
                body.push_str(
                    format!("chatos_memory_reliability_backlog{{kind=\"{kind}\"}} {value}\n")
                        .as_str(),
                );
            }
        }
        Err(error) => {
            tracing::error!(error = %error, "Memory Engine reliability metrics audit failed");
            body.push_str("chatos_memory_reliability_audit_success 0\n");
        }
    }
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
