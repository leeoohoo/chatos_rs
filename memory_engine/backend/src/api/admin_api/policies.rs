use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::json;

use super::error::internal_error;
use crate::models::{GenerateJobPolicyPromptRequest, UpsertEngineJobPolicyRequest};
use crate::repositories::control_plane;
use crate::services::policy_prompt;
use crate::state::AppState;

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

pub async fn generate_job_policy_prompt(
    State(state): State<Arc<AppState>>,
    Path(job_type): Path<String>,
    Json(req): Json<GenerateJobPolicyPromptRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    policy_prompt::generate_job_policy_prompt(state.as_ref(), job_type.as_str(), &req)
        .await
        .map(|item| Json(json!(item)))
        .map_err(internal_error)
}
