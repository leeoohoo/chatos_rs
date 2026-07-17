// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::local_runtime::storage::LocalMemorySummaryRecord;
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;

#[derive(Debug, Default, Deserialize)]
pub(super) struct SummaryQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(super) struct LocalSummaryListResponse {
    items: Vec<LocalMemorySummaryRecord>,
    total: i64,
    has_summary: bool,
}

pub(super) async fn list_summaries(
    Path(session_id): Path<String>,
    Query(query): Query<SummaryQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalSummaryListResponse>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let session_id = required(session_id, "session_id")?;
    let items = runtime
        .local_database()?
        .list_memory_summaries(
            owner.owner_user_id.as_str(),
            session_id.as_str(),
            query.limit.unwrap_or(100),
            query.offset.unwrap_or_default(),
        )
        .await?;
    let total = items.len() as i64;
    Ok(Json(LocalSummaryListResponse {
        has_summary: total > 0,
        total,
        items,
    }))
}

pub(super) async fn delete_summary(
    Path((session_id, summary_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let session_id = required(session_id, "session_id")?;
    let summary_id = required(summary_id, "summary_id")?;
    let deleted = runtime
        .local_database()?
        .delete_memory_summary(
            owner.owner_user_id.as_str(),
            session_id.as_str(),
            summary_id.as_str(),
        )
        .await?;
    if !deleted {
        return Err(LocalRuntimeApiError::not_found(
            "local_memory_summary_not_found",
            "Local memory summary was not found",
        ));
    }
    Ok(Json(serde_json::json!({ "success": true })))
}

pub(super) async fn clear_summaries(
    Path(session_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<serde_json::Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let session_id = required(session_id, "session_id")?;
    let deleted = runtime
        .local_database()?
        .clear_memory_summaries(owner.owner_user_id.as_str(), session_id.as_str())
        .await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "deleted_summaries": deleted,
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
