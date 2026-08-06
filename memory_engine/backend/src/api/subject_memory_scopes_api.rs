// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use super::{memory_auth::MemoryAuthContext, source_guard};
use crate::models::{EngineSubjectMemoryScope, UpsertSubjectMemoryScopeRequest};
use crate::repositories::subject_memory_scopes;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListSubjectMemoryScopesQuery {
    tenant_id: Option<String>,
    source_id: Option<String>,
    limit: Option<i64>,
}

pub async fn upsert_subject_memory_scope(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(scope_key): Path<String>,
    Json(req): Json<UpsertSubjectMemoryScopeRequest>,
) -> Result<Json<EngineSubjectMemoryScope>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(req.tenant_id.as_str())?;
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    let tenant_id = req.tenant_id.clone();
    let source_id = req.source_id.clone();
    let scope =
        subject_memory_scopes::upsert_subject_memory_scope(&state.pool, scope_key.as_str(), req)
            .await
            .map_err(internal_error)?;
    if let Err(err) = crate::subject_memory_queue::publish_pending_scope(
        &state.config,
        &state.pool,
        tenant_id.as_str(),
        source_id.as_str(),
        scope_key.as_str(),
    )
    .await
    {
        tracing::warn!(
            scope_key = scope_key.as_str(),
            error = err.as_str(),
            "Memory Engine left subject memory scope event in Outbox for recovery"
        );
    }
    Ok(Json(scope))
}

pub async fn list_subject_memory_scopes(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Query(query): Query<ListSubjectMemoryScopesQuery>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let tenant_id = auth.resolve_tenant_scope(query.tenant_id.as_deref())?;
    let items = subject_memory_scopes::list_active_subject_memory_scopes(
        &state.pool,
        tenant_id.as_deref(),
        query.source_id.as_deref(),
        query.limit.unwrap_or(200),
    )
    .await
    .map_err(internal_error)?;
    Ok(Json(json!({ "items": items })))
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
