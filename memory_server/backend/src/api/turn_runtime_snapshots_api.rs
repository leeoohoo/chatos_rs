use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::models::{SyncTurnRuntimeSnapshotRequest, TurnRuntimeSnapshotLookupResponse};
use crate::repositories::{messages, sessions, turn_runtime_snapshots};

use super::{ensure_session_access, require_auth, SharedState};

pub(super) async fn sync_turn_runtime_snapshot(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
    Json(req): Json<SyncTurnRuntimeSnapshotRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }
    let normalized_turn_id = turn_id.trim().to_string();
    if normalized_turn_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "turn_id is required"})),
        );
    }

    let session = match sessions::get_session_by_id(&state.pool, session_id.as_str()).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "session not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load session failed", "detail": err})),
            )
        }
    };

    match turn_runtime_snapshots::upsert_turn_runtime_snapshot(
        &state.pool,
        session_id.as_str(),
        normalized_turn_id.as_str(),
        session.user_id.as_str(),
        req,
    )
    .await
    {
        Ok(snapshot) => (StatusCode::OK, Json(json!(snapshot))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync turn runtime snapshot failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_sync_turn_runtime_snapshot(
    State(state): State<SharedState>,
    Path((session_id, turn_id)): Path<(String, String)>,
    Json(req): Json<SyncTurnRuntimeSnapshotRequest>,
) -> (StatusCode, Json<Value>) {
    let normalized_turn_id = turn_id.trim().to_string();
    if normalized_turn_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "turn_id is required"})),
        );
    }

    let session = match sessions::get_session_by_id(&state.pool, session_id.as_str()).await {
        Ok(Some(session)) => session,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "session not found"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load session failed", "detail": err})),
            )
        }
    };

    match turn_runtime_snapshots::upsert_turn_runtime_snapshot(
        &state.pool,
        session_id.as_str(),
        normalized_turn_id.as_str(),
        session.user_id.as_str(),
        req,
    )
    .await
    {
        Ok(snapshot) => (StatusCode::OK, Json(json!(snapshot))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync turn runtime snapshot failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_latest_turn_runtime_snapshot(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    let latest_user_message = match messages::get_latest_user_message_by_session(
        &state.pool,
        session_id.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load latest user message failed", "detail": err})),
            )
        }
    };

    let Some(user_message) = latest_user_message else {
        return (
            StatusCode::OK,
            Json(json!(build_missing_response(session_id, None))),
        );
    };

    let turn_id = derive_turn_id_from_message(&user_message);
    let found = match turn_runtime_snapshots::get_turn_runtime_snapshot(
        &state.pool,
        session_id.as_str(),
        turn_id.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load turn runtime snapshot failed", "detail": err})),
            )
        }
    };

    let response = match found {
        Some(snapshot) => TurnRuntimeSnapshotLookupResponse {
            session_id,
            turn_id: Some(turn_id),
            status: snapshot.status.clone(),
            snapshot_source: "captured".to_string(),
            snapshot: Some(snapshot),
        },
        None => build_missing_response(session_id, Some(turn_id)),
    };
    (StatusCode::OK, Json(json!(response)))
}

pub(super) async fn internal_get_latest_turn_runtime_snapshot(
    State(state): State<SharedState>,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let latest_user_message = match messages::get_latest_user_message_by_session(
        &state.pool,
        session_id.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load latest user message failed", "detail": err})),
            )
        }
    };

    let Some(user_message) = latest_user_message else {
        return (
            StatusCode::OK,
            Json(json!(build_missing_response(session_id, None))),
        );
    };

    let turn_id = derive_turn_id_from_message(&user_message);
    let found = match turn_runtime_snapshots::get_turn_runtime_snapshot(
        &state.pool,
        session_id.as_str(),
        turn_id.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load turn runtime snapshot failed", "detail": err})),
            )
        }
    };

    let response = match found {
        Some(snapshot) => TurnRuntimeSnapshotLookupResponse {
            session_id,
            turn_id: Some(turn_id),
            status: snapshot.status.clone(),
            snapshot_source: "captured".to_string(),
            snapshot: Some(snapshot),
        },
        None => build_missing_response(session_id, Some(turn_id)),
    };
    (StatusCode::OK, Json(json!(response)))
}

pub(super) async fn get_turn_runtime_snapshot_by_turn(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }
    let normalized_turn_id = turn_id.trim().to_string();
    if normalized_turn_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "turn_id is required"})),
        );
    }

    match turn_runtime_snapshots::get_turn_runtime_snapshot(
        &state.pool,
        session_id.as_str(),
        normalized_turn_id.as_str(),
    )
    .await
    {
        Ok(Some(snapshot)) => (
            StatusCode::OK,
            Json(json!(TurnRuntimeSnapshotLookupResponse {
                session_id,
                turn_id: Some(normalized_turn_id),
                status: snapshot.status.clone(),
                snapshot_source: "captured".to_string(),
                snapshot: Some(snapshot),
            })),
        ),
        Ok(None) => (
            StatusCode::OK,
            Json(json!(build_missing_response(
                session_id,
                Some(normalized_turn_id)
            ))),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load turn runtime snapshot failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_get_turn_runtime_snapshot_by_turn(
    State(state): State<SharedState>,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let normalized_turn_id = turn_id.trim().to_string();
    if normalized_turn_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "turn_id is required"})),
        );
    }

    match turn_runtime_snapshots::get_turn_runtime_snapshot(
        &state.pool,
        session_id.as_str(),
        normalized_turn_id.as_str(),
    )
    .await
    {
        Ok(Some(snapshot)) => (
            StatusCode::OK,
            Json(json!(TurnRuntimeSnapshotLookupResponse {
                session_id,
                turn_id: Some(normalized_turn_id),
                status: snapshot.status.clone(),
                snapshot_source: "captured".to_string(),
                snapshot: Some(snapshot),
            })),
        ),
        Ok(None) => (
            StatusCode::OK,
            Json(json!(build_missing_response(
                session_id,
                Some(normalized_turn_id)
            ))),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load turn runtime snapshot failed", "detail": err})),
        ),
    }
}

fn derive_turn_id_from_message(message: &crate::models::Message) -> String {
    let metadata_turn_id = message
        .metadata
        .as_ref()
        .and_then(|metadata| {
            metadata
                .get("conversation_turn_id")
                .or_else(|| metadata.get("conversationTurnId"))
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    metadata_turn_id.unwrap_or_else(|| message.id.clone())
}

fn build_missing_response(
    session_id: String,
    turn_id: Option<String>,
) -> TurnRuntimeSnapshotLookupResponse {
    TurnRuntimeSnapshotLookupResponse {
        session_id,
        turn_id,
        status: "unknown".to_string(),
        snapshot_source: "missing".to_string(),
        snapshot: None,
    }
}
