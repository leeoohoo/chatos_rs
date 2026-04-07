use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{CreateSessionRequest, UpdateSessionRequest};
use crate::repositories::sessions;

use super::{ensure_admin, ensure_session_access, require_auth, SharedState};

#[derive(Debug, Deserialize)]
pub(super) struct ListSessionsQuery {
    user_id: Option<String>,
    project_id: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SyncSessionRequest {
    user_id: String,
    project_id: Option<String>,
    title: Option<String>,
    metadata: Option<Value>,
    status: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

pub(super) async fn create_session(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<CreateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if !auth.is_admin() {
        req.user_id = auth.user_id;
    } else if req.user_id.trim().is_empty() {
        req.user_id = auth.user_id;
    }

    match sessions::create_session(&state.pool, req).await {
        Ok(session) => (StatusCode::OK, Json(json!(session))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create session failed", "detail": err})),
        ),
    }
}

pub(super) async fn sync_session(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(req): Json<SyncSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    match sessions::upsert_session_sync(
        &state.pool,
        session_id.as_str(),
        req.user_id.as_str(),
        req.project_id,
        req.title,
        req.metadata,
        req.status,
        req.created_at,
        req.updated_at,
    )
    .await
    {
        Ok(session) => (StatusCode::OK, Json(json!(session))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync session failed", "detail": err})),
        ),
    }
}

pub(super) async fn list_sessions(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListSessionsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let limit = q.limit.unwrap_or(50);
    let offset = q.offset.unwrap_or(0);
    let scope_user_id = if auth.is_admin() {
        q.user_id.as_deref()
    } else {
        Some(auth.user_id.as_str())
    };
    match sessions::list_sessions(
        &state.pool,
        scope_user_id,
        q.project_id.as_deref(),
        q.status.as_deref(),
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list sessions failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_create_session(
    State(state): State<SharedState>,
    Json(req): Json<CreateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    match sessions::create_session(&state.pool, req).await {
        Ok(session) => (StatusCode::OK, Json(json!(session))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create session failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_list_sessions(
    State(state): State<SharedState>,
    Query(q): Query<ListSessionsQuery>,
) -> (StatusCode, Json<Value>) {
    let limit = q.limit.unwrap_or(50);
    let offset = q.offset.unwrap_or(0);
    match sessions::list_sessions(
        &state.pool,
        q.user_id.as_deref(),
        q.project_id.as_deref(),
        q.status.as_deref(),
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list sessions failed", "detail": err})),
        ),
    }
}

pub(super) async fn delete_session(
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

    match sessions::delete_session(&state.pool, session_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({ "success": true }))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete session failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_session(
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

    match sessions::get_session_by_id(&state.pool, session_id.as_str()).await {
        Ok(Some(session)) => (StatusCode::OK, Json(json!(session))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get session failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_get_session(
    State(state): State<SharedState>,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match sessions::get_session_by_id(&state.pool, session_id.as_str()).await {
        Ok(Some(session)) => (StatusCode::OK, Json(json!(session))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get session failed", "detail": err})),
        ),
    }
}

pub(super) async fn update_session(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(req): Json<UpdateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id.as_str()).await {
        return err;
    }

    match sessions::update_session(&state.pool, session_id.as_str(), req).await {
        Ok(Some(session)) => (StatusCode::OK, Json(json!(session))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "update session failed", "detail": err})),
        ),
    }
}

pub(super) async fn internal_update_session(
    State(state): State<SharedState>,
    Path(session_id): Path<String>,
    Json(req): Json<UpdateSessionRequest>,
) -> (StatusCode, Json<Value>) {
    match sessions::update_session(&state.pool, session_id.as_str(), req).await {
        Ok(Some(session)) => (StatusCode::OK, Json(json!(session))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "update session failed", "detail": err})),
        ),
    }
}
