use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::jobs;
use crate::repositories::jobs as job_repo;

use super::{
    build_ai_client, ensure_admin, ensure_session_access, require_auth, resolve_scope_user_id,
    SharedState,
};

#[derive(Debug, Deserialize)]
pub(super) struct RunJobRequest {
    user_id: Option<String>,
    session_id: Option<String>,
}

pub(super) async fn run_summary_once(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<RunJobRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let ai = match build_ai_client(&state) {
        Ok(client) => client,
        Err(err) => return err,
    };

    let result = if let Some(session_id) = req.session_id.as_deref() {
        if let Err(err) = ensure_session_access(state.as_ref(), &auth, session_id).await {
            return err;
        }
        jobs::summary::run_once_for_session(&state.pool, &ai, scope_user_id.as_str(), session_id)
            .await
            .map(|_| json!({"session_id": session_id, "done": true}))
    } else {
        jobs::summary::run_once(&state.pool, &ai, scope_user_id.as_str())
            .await
            .map(|r| json!(r))
    };

    match result {
        Ok(data) => (StatusCode::OK, Json(json!({"ok": true, "data": data}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": err})),
        ),
    }
}

pub(super) async fn run_rollup_once(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<RunJobRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let ai = match build_ai_client(&state) {
        Ok(client) => client,
        Err(err) => return err,
    };

    match jobs::rollup::run_once(&state.pool, &ai, scope_user_id.as_str()).await {
        Ok(data) => (StatusCode::OK, Json(json!({"ok": true, "data": data}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": err})),
        ),
    }
}

pub(super) async fn run_agent_memory_once(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<RunJobRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let ai = match build_ai_client(&state) {
        Ok(client) => client,
        Err(err) => return err,
    };

    match jobs::agent_memory::run_once(&state.pool, &ai, scope_user_id.as_str()).await {
        Ok(data) => (StatusCode::OK, Json(json!({"ok": true, "data": data}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": err})),
        ),
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct JobRunsQuery {
    job_type: Option<String>,
    session_id: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
}

pub(super) async fn list_job_runs(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<JobRunsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    match job_repo::list_job_runs(
        &state.pool,
        q.job_type.as_deref(),
        q.session_id.as_deref(),
        q.status.as_deref(),
        q.limit.unwrap_or(100),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list job runs failed", "detail": err})),
        ),
    }
}

pub(super) async fn job_stats(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_admin(&auth) {
        return err;
    }

    match job_repo::job_stats(&state.pool).await {
        Ok(stats) => (StatusCode::OK, Json(json!({"stats": stats}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "job stats failed", "detail": err})),
        ),
    }
}
