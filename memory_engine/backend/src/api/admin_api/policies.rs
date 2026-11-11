// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::json;

use super::error::internal_error;
use crate::api::memory_auth::MemoryAuthContext;
use crate::repositories::control_plane;
use crate::state::AppState;

pub async fn list_job_policies(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    auth.require_super_admin_or_operator()?;
    control_plane::list_job_policies(&state.pool)
        .await
        .map(|items| Json(json!({ "items": items })))
        .map_err(internal_error)
}
pub async fn get_job_policy(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(job_type): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    auth.require_super_admin_or_operator()?;
    control_plane::get_effective_job_policy(&state.pool, job_type.as_str())
        .await
        .map(|item| Json(json!(item)))
        .map_err(internal_error)
}
