// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::models::{
    EngineThreadSnapshot, ThreadSnapshotLookupResponse, UpsertThreadSnapshotRequest,
};
use crate::repositories::thread_snapshots;
use crate::state::AppState;

use super::auth::SdkAuthContext;
use super::internal_error;
use super::requests::{
    SdkGetLatestThreadSnapshotRequest, SdkGetThreadSnapshotByTurnRequest,
    SdkUpsertThreadSnapshotRequest,
};

pub async fn upsert_thread_snapshot(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path((thread_id, snapshot_type, turn_id)): Path<(String, String, String)>,
    Json(req): Json<SdkUpsertThreadSnapshotRequest>,
) -> Result<Json<EngineThreadSnapshot>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    let direct = UpsertThreadSnapshotRequest {
        tenant_id: req.tenant_id,
        source_id: auth.source_id().to_string(),
        user_message_id: req.user_message_id,
        status: req.status,
        snapshot_source: req.snapshot_source,
        snapshot_version: req.snapshot_version,
        payload: req.payload,
        metadata: req.metadata,
        captured_at: req.captured_at,
    };
    thread_snapshots::upsert_thread_snapshot(
        &state.pool,
        thread_id.as_str(),
        snapshot_type.as_str(),
        turn_id.as_str(),
        direct,
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn get_latest_thread_snapshot(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path((thread_id, snapshot_type)): Path<(String, String)>,
    Json(req): Json<SdkGetLatestThreadSnapshotRequest>,
) -> Result<Json<ThreadSnapshotLookupResponse>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    thread_snapshots::get_latest_thread_snapshot(
        &state.pool,
        thread_id.as_str(),
        snapshot_type.as_str(),
        req.tenant_id.as_str(),
        auth.source_id(),
    )
    .await
    .map(Json)
    .map_err(internal_error)
}

pub async fn get_thread_snapshot_by_turn(
    State(state): State<Arc<AppState>>,
    auth: SdkAuthContext,
    Path((thread_id, snapshot_type, turn_id)): Path<(String, String, String)>,
    Json(req): Json<SdkGetThreadSnapshotByTurnRequest>,
) -> Result<Json<ThreadSnapshotLookupResponse>, (StatusCode, String)> {
    auth.require_tenant(req.tenant_id.as_str())?;
    thread_snapshots::get_thread_snapshot_by_turn(
        &state.pool,
        thread_id.as_str(),
        snapshot_type.as_str(),
        turn_id.as_str(),
        req.tenant_id.as_str(),
        auth.source_id(),
    )
    .await
    .map(Json)
    .map_err(internal_error)
}
