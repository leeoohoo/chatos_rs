use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};

use super::{memory_auth::MemoryAuthContext, source_guard};
use crate::models::{EngineSubject, UpsertSubjectRequest};
use crate::repositories::subjects;
use crate::state::AppState;

pub async fn upsert_subject(
    State(state): State<Arc<AppState>>,
    auth: MemoryAuthContext,
    Path(subject_id): Path<String>,
    Json(req): Json<UpsertSubjectRequest>,
) -> Result<Json<EngineSubject>, (axum::http::StatusCode, String)> {
    auth.ensure_tenant_scope(req.tenant_id.as_str())?;
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    subjects::upsert_subject(&state.pool, subject_id.as_str(), req)
        .await
        .map(Json)
        .map_err(internal_error)
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
