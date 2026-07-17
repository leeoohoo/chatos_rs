// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::local_runtime::storage::LocalSubjectMemoryRecord;
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;

#[derive(Debug, Default, Deserialize)]
pub(super) struct RecallQuery {
    limit: Option<i64>,
}

pub(super) async fn list_recalls(
    Path(session_id): Path<String>,
    Query(query): Query<RecallQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Vec<LocalSubjectMemoryRecord>>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_invalid_request",
            "session_id is required",
        ));
    }
    let database = runtime.local_database()?;
    let recall_limit = match query.limit {
        Some(limit) => limit,
        None => database
            .get_runtime_settings(owner.owner_user_id.as_str(), session_id)
            .await?
            .map(|settings| settings.memory_recall_limit)
            .unwrap_or(8),
    };
    let records = database
        .list_subject_memories_for_session(owner.owner_user_id.as_str(), session_id, recall_limit)
        .await?;
    Ok(Json(records))
}

pub(super) async fn forget_recall(
    Path((session_id, recall_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let session_id = required(session_id, "session_id")?;
    let recall_id = required(recall_id, "recall_id")?;
    let deleted = runtime
        .local_database()?
        .forget_subject_memory_for_session(
            owner.owner_user_id.as_str(),
            session_id.as_str(),
            recall_id.as_str(),
        )
        .await?;
    if deleted == 0 {
        return Err(LocalRuntimeApiError::not_found(
            "local_memory_recall_not_found",
            "Local memory recall was not found",
        ));
    }
    Ok(Json(serde_json::json!({
        "success": true,
        "deleted_recalls": deleted,
    })))
}

fn required(value: String, name: &str) -> Result<String, LocalRuntimeApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_invalid_request",
            format!("{name} is required"),
        ));
    }
    Ok(value)
}
