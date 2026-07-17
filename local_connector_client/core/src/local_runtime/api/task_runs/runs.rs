// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::LocalRuntime;

use super::super::context::owner_context;
use super::super::error::LocalRuntimeApiError;

pub(super) async fn get_run(
    Path(run_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let run = runtime
        .local_database()?
        .get_local_task_run(owner.owner_user_id.as_str(), run_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_task_run_not_found",
                "Local task run was not found",
            )
        })?;
    Ok(Json(json!(run)))
}

pub(super) async fn cancel_run(
    Path(run_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let run = runtime
        .local_database()?
        .request_local_task_run_cancel(owner.owner_user_id.as_str(), run_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_task_run_not_found",
                "Local task run was not found",
            )
        })?;
    Ok(Json(json!({ "success": true, "run": run })))
}

pub(super) async fn retry_run(
    Path(run_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let run = runtime
        .local_database()?
        .retry_local_task_run(owner.owner_user_id.as_str(), run_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::conflict(
                "local_task_run_not_retryable",
                "Local task run cannot be retried",
            )
        })?;
    Ok(Json(json!({ "success": true, "run": run })))
}
