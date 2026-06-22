use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use super::source_guard;
use crate::models::{
    EngineThreadSnapshot, ThreadSnapshotLookupResponse, UpsertThreadSnapshotRequest,
};
use crate::repositories::thread_snapshots;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SnapshotLookupQuery {
    tenant_id: String,
    source_id: String,
}

pub async fn upsert_thread_snapshot(
    State(state): State<Arc<AppState>>,
    Path((thread_id, snapshot_type, turn_id)): Path<(String, String, String)>,
    Json(req): Json<UpsertThreadSnapshotRequest>,
) -> Result<Json<EngineThreadSnapshot>, (axum::http::StatusCode, String)> {
    source_guard::ensure_write_source_allowed(&state.pool, req.source_id.as_str()).await?;
    thread_snapshots::upsert_thread_snapshot(
        &state.pool,
        thread_id.as_str(),
        snapshot_type.as_str(),
        turn_id.as_str(),
        req,
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn get_latest_thread_snapshot(
    State(state): State<Arc<AppState>>,
    Path((thread_id, snapshot_type)): Path<(String, String)>,
    Query(query): Query<SnapshotLookupQuery>,
) -> Result<Json<ThreadSnapshotLookupResponse>, (axum::http::StatusCode, String)> {
    thread_snapshots::get_latest_thread_snapshot(
        &state.pool,
        thread_id.as_str(),
        snapshot_type.as_str(),
        query.tenant_id.as_str(),
        query.source_id.as_str(),
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn get_thread_snapshot_by_turn(
    State(state): State<Arc<AppState>>,
    Path((thread_id, snapshot_type, turn_id)): Path<(String, String, String)>,
    Query(query): Query<SnapshotLookupQuery>,
) -> Result<Json<ThreadSnapshotLookupResponse>, (axum::http::StatusCode, String)> {
    thread_snapshots::get_thread_snapshot_by_turn(
        &state.pool,
        thread_id.as_str(),
        snapshot_type.as_str(),
        turn_id.as_str(),
        query.tenant_id.as_str(),
        query.source_id.as_str(),
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

fn internal_error(message: String) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::INTERNAL_SERVER_ERROR, message)
}
