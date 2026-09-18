// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt::Write;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Json;

use crate::models::{HealthResponse, SystemConfigResponse};
use crate::state::AppState;
use crate::store::now_rfc3339;

use super::ApiResult;

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        service: "user_service_backend".to_string(),
        now: now_rfc3339(),
    })
}

pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.retention.stats();
    let mut body = String::new();
    for (name, help, metric_type, value) in [
        (
            "chatos_user_service_retention_successful_runs_total",
            "Successful User Service retention runs.",
            "counter",
            stats.successful_runs_total,
        ),
        (
            "chatos_user_service_retention_failed_runs_total",
            "Failed User Service retention runs.",
            "counter",
            stats.failed_runs_total,
        ),
        (
            "chatos_user_service_retention_deleted_rows_total",
            "Expired User Service rows deleted by retention.",
            "counter",
            stats.deleted_rows_total,
        ),
        (
            "chatos_user_service_retention_last_success_unix",
            "Unix timestamp of the last successful User Service retention run; zero means none.",
            "gauge",
            stats.last_success_unix.unwrap_or_default() as u64,
        ),
    ] {
        let _ = writeln!(body, "# HELP {name} {help}");
        let _ = writeln!(body, "# TYPE {name} {metric_type}");
        let _ = writeln!(body, "{name}{{service=\"user-service\"}} {value}");
    }
    body.push_str(&chatos_postgres::render_pool_metrics(
        state.store.pool(),
        "user-service",
    ));
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

pub async fn get_system_config(State(state): State<AppState>) -> ApiResult<SystemConfigResponse> {
    Ok(Json(SystemConfigResponse {
        service: "user_service_backend".to_string(),
        issuer: state.config.jwt_issuer.clone(),
        user_service_audience: state.config.user_service_audience.clone(),
        task_runner_audience: state.config.task_runner_audience.clone(),
        database_url: state.config.database_url.clone(),
        user_access_ttl_seconds: state.config.user_access_ttl_seconds,
        task_runner_access_ttl_seconds: state.config.task_runner_access_ttl_seconds,
    }))
}
