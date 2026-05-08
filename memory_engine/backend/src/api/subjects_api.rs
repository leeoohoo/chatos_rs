use std::sync::Arc;

use axum::{extract::{Path, State}, Json};

use crate::models::{EngineSubject, UpsertSubjectRequest};
use crate::repositories::subjects;
use crate::state::AppState;

pub async fn upsert_subject(
    State(state): State<Arc<AppState>>,
    Path(subject_id): Path<String>,
    Json(req): Json<UpsertSubjectRequest>,
) -> Result<Json<EngineSubject>, (axum::http::StatusCode, String)> {
    subjects::upsert_subject(&state.pool, subject_id.as_str(), req)
        .await
        .map(Json)
        .map_err(internal_error)
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
