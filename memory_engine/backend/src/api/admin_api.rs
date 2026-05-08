use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::models::{
    UpsertEngineJobPolicyRequest, UpsertEngineModelProfileRequest,
};
use crate::repositories::control_plane;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct JobRunsQuery {
    job_type: Option<String>,
    thread_id: Option<String>,
    status: Option<String>,
    tenant_id: Option<String>,
    source_id: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct JobRunStatsQuery {
    job_type: Option<String>,
    tenant_id: Option<String>,
    source_id: Option<String>,
    since_hours: Option<i64>,
}

pub async fn list_model_profiles(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    control_plane::list_model_profiles(&state.pool)
        .await
        .map(|items| Json(json!({ "items": items })))
        .map_err(internal_error)
}

pub async fn create_model_profile(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpsertEngineModelProfileRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    control_plane::create_model_profile(&state.pool, req)
        .await
        .map(|item| Json(json!(item)))
        .map_err(internal_error)
}

pub async fn update_model_profile(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
    Json(req): Json<UpsertEngineModelProfileRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    match control_plane::update_model_profile(&state.pool, model_id.as_str(), req).await {
        Ok(Some(item)) => Ok(Json(json!(item))),
        Ok(None) => Err((axum::http::StatusCode::NOT_FOUND, "model profile not found".to_string())),
        Err(err) => Err(internal_error(err)),
    }
}

pub async fn delete_model_profile(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    match control_plane::delete_model_profile(&state.pool, model_id.as_str()).await {
        Ok(true) => Ok(Json(json!({"success": true}))),
        Ok(false) => Err((axum::http::StatusCode::NOT_FOUND, "model profile not found".to_string())),
        Err(err) => Err(internal_error(err)),
    }
}

pub async fn list_job_policies(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    control_plane::list_job_policies(&state.pool)
        .await
        .map(|items| Json(json!({ "items": items })))
        .map_err(internal_error)
}

pub async fn get_job_policy(
    State(state): State<Arc<AppState>>,
    Path(job_type): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    control_plane::get_effective_job_policy(&state.pool, job_type.as_str())
        .await
        .map(|item| Json(json!(item)))
        .map_err(internal_error)
}

pub async fn upsert_job_policy(
    State(state): State<Arc<AppState>>,
    Path(job_type): Path<String>,
    Json(req): Json<UpsertEngineJobPolicyRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    control_plane::upsert_job_policy(&state.pool, job_type.as_str(), req)
        .await
        .map(|item| Json(json!(item)))
        .map_err(internal_error)
}

pub async fn list_job_runs(
    State(state): State<Arc<AppState>>,
    Query(q): Query<JobRunsQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    control_plane::list_job_runs(
        &state.pool,
        q.job_type.as_deref(),
        q.thread_id.as_deref(),
        q.status.as_deref(),
        q.tenant_id.as_deref(),
        q.source_id.as_deref(),
        q.limit.unwrap_or(100),
    )
    .await
    .map(|items| Json(json!({ "items": items })))
    .map_err(internal_error)
}

pub async fn job_run_stats(
    State(state): State<Arc<AppState>>,
    Query(q): Query<JobRunStatsQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    control_plane::job_run_stats(
        &state.pool,
        q.job_type.as_deref(),
        q.tenant_id.as_deref(),
        q.source_id.as_deref(),
        q.since_hours.unwrap_or(24),
    )
    .await
    .map(|stats| Json(json!({ "stats": stats })))
    .map_err(internal_error)
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
