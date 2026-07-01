// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::json;

use crate::models::{
    EngineSubjectMemory, EngineSubjectMemoryScope, UpsertSubjectMemoryScopeRequest,
};
use crate::repositories::{subject_memories, subject_memory_scopes, summaries};
use crate::state::AppState;

use super::auth::SdkAuthContext;
use super::internal_error;
use super::requests::{
    SdkListSummariesByThreadLabelRequest, SdkQuerySubjectMemoriesRequest,
    SdkUpsertSubjectMemoryScopeRequest,
};

pub async fn upsert_subject_memory_scope(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path(scope_key): Path<String>,
    Json(req): Json<SdkUpsertSubjectMemoryScopeRequest>,
) -> Result<Json<EngineSubjectMemoryScope>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let direct = UpsertSubjectMemoryScopeRequest {
        tenant_id: req.tenant_id,
        source_id: auth.source_id().to_string(),
        subject_id: req.subject_id,
        memory_type: req.memory_type,
        source_thread_label: req.source_thread_label,
        relation_subject_id: req.relation_subject_id,
        source_summary_type: req.source_summary_type,
        prompt_title: req.prompt_title,
        memory_metadata: req.memory_metadata,
        status: req.status,
    };
    subject_memory_scopes::upsert_subject_memory_scope(&state.pool, scope_key.as_str(), direct)
        .await
        .map(Json)
        .map_err(internal_error)
}

pub async fn query_subject_memories(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Json(req): Json<SdkQuerySubjectMemoriesRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let items: Vec<EngineSubjectMemory> = subject_memories::query_subject_memories(
        &state.pool,
        req.tenant_id.as_str(),
        auth.source_id(),
        req.subject_id.as_str(),
        req.memory_type.as_deref(),
        req.level,
        req.max_level_exclusive,
        req.rollup_status.as_deref(),
        req.relation_subject_id.as_deref(),
        req.source_digest.as_deref(),
        req.limit.unwrap_or(100),
        req.offset.unwrap_or(0),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}

pub async fn list_summaries_by_thread_label(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Json(req): Json<SdkListSummariesByThreadLabelRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let items = summaries::list_summaries_by_thread_label(
        &state.pool,
        req.tenant_id.as_str(),
        auth.source_id(),
        req.thread_label.as_str(),
        req.summary_type.as_deref(),
        req.status.as_deref(),
        req.level,
        req.subject_memory_summarized,
        req.limit.unwrap_or(200),
        req.offset.unwrap_or(0),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}
