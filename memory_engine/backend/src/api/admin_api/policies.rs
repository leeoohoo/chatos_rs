// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde_json::json;

use super::error::internal_error;
use crate::api::memory_auth::MemoryAuthContext;
use crate::services::control_plane;
use crate::state::AppState;

pub async fn list_job_policies(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    auth.require_super_admin_or_operator()?;
    let owner = auth.runtime_owner_user_id()?;
    let mut items = Vec::new();
    for job_type in ["summary", "rollup", "subject_memory"] {
        items.push(
            control_plane::get_effective_job_policy(&state.config, &owner, job_type)
                .await
                .map_err(internal_error)?,
        );
    }
    Ok(Json(json!({"items": items})))
}

pub async fn get_job_policy(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(job_type): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    auth.require_super_admin_or_operator()?;
    let owner = auth.runtime_owner_user_id()?;
    control_plane::get_effective_job_policy(&state.config, &owner, job_type.as_str())
        .await
        .map(|item| Json(json!(item)))
        .map_err(internal_error)
}
