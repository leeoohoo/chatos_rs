// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use chatos_builtin_tools::TaskUpdatePatch;
use serde_json::{json, Value};

use crate::LocalRuntime;

use super::super::context::owner_context;
use super::super::error::LocalRuntimeApiError;

pub(super) async fn update_task(
    Path((session_id, task_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(patch): Json<TaskUpdatePatch>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let record = runtime
        .local_database()?
        .update_local_task_board_task(
            owner.owner_user_id.as_str(),
            session_id.as_str(),
            task_id.as_str(),
            patch,
        )
        .await?;
    Ok(Json(serde_json::to_value(record).map_err(|error| {
        LocalRuntimeApiError::internal(error.to_string())
    })?))
}

pub(super) async fn complete_task(
    Path((session_id, task_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
    Json(patch): Json<TaskUpdatePatch>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let record = runtime
        .local_database()?
        .complete_local_task_board_task(
            owner.owner_user_id.as_str(),
            session_id.as_str(),
            task_id.as_str(),
            patch,
        )
        .await?;
    Ok(Json(serde_json::to_value(record).map_err(|error| {
        LocalRuntimeApiError::internal(error.to_string())
    })?))
}

pub(super) async fn delete_task(
    Path((session_id, task_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let deleted = runtime
        .local_database()?
        .delete_local_task_board_task(
            owner.owner_user_id.as_str(),
            session_id.as_str(),
            task_id.as_str(),
        )
        .await?;
    Ok(Json(json!({ "success": deleted })))
}
