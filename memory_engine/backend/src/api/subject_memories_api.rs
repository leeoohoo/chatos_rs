use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use super::source_guard;
use crate::models::{
    EngineSubjectMemory, MarkSubjectMemoriesRolledUpRequest, MarkSubjectMemoriesRolledUpResponse,
    QuerySubjectMemoriesRequest, UpsertSubjectMemoryRequest,
};
use crate::repositories::subject_memories;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListSubjectMemoriesQuery {
    tenant_id: String,
    source_id: String,
    memory_type: Option<String>,
    level: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn upsert_subject_memory(
    State(state): State<Arc<AppState>>,
    Path((subject_id, memory_key)): Path<(String, String)>,
    Json(req): Json<UpsertSubjectMemoryRequest>,
) -> Result<Json<EngineSubjectMemory>, (axum::http::StatusCode, String)> {
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    subject_memories::upsert_subject_memory(
        &state.pool,
        subject_id.as_str(),
        memory_key.as_str(),
        req,
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn list_subject_memories(
    State(state): State<Arc<AppState>>,
    Path(subject_id): Path<String>,
    Query(query): Query<ListSubjectMemoriesQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let items = subject_memories::list_subject_memories(
        &state.pool,
        query.tenant_id.as_str(),
        query.source_id.as_str(),
        subject_id.as_str(),
        query.memory_type.as_deref(),
        query.level,
        query.limit.unwrap_or(100),
        query.offset.unwrap_or(0),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}

pub async fn mark_subject_memories_rolled_up(
    State(state): State<Arc<AppState>>,
    Path(subject_id): Path<String>,
    Json(req): Json<MarkSubjectMemoriesRolledUpRequest>,
) -> Result<Json<MarkSubjectMemoriesRolledUpResponse>, (axum::http::StatusCode, String)> {
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    let marked = subject_memories::mark_subject_memories_rolled_up(
        &state.pool,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
        subject_id.as_str(),
        req.memory_ids.as_slice(),
        req.rollup_memory_key.as_str(),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(MarkSubjectMemoriesRolledUpResponse { marked }))
}

pub async fn query_subject_memories(
    State(state): State<Arc<AppState>>,
    Json(req): Json<QuerySubjectMemoriesRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let items = subject_memories::query_subject_memories(
        &state.pool,
        req.tenant_id.as_str(),
        req.source_id.as_str(),
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

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
