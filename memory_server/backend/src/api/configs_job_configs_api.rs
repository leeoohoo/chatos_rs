use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{
    UpsertAgentMemoryJobConfigRequest, UpsertSummaryJobConfigRequest,
    UpsertSummaryRollupJobConfigRequest, UpsertTaskExecutionRollupJobConfigRequest,
    UpsertTaskExecutionSummaryJobConfigRequest,
};
use crate::repositories::configs;

use super::{require_auth, resolve_scope_user_id, SharedState};

#[derive(Debug, Deserialize)]
pub(super) struct UserIdQuery {
    user_id: Option<String>,
}

pub(super) async fn get_summary_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<UserIdQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let user_id = resolve_scope_user_id(&auth, q.user_id);
    match configs::get_summary_job_config(&state.pool, user_id.as_str()).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get summary job config failed", "detail": err})),
        ),
    }
}

pub(super) async fn put_summary_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<UpsertSummaryJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    req.user_id = resolve_scope_user_id(&auth, Some(req.user_id.clone()));

    match configs::upsert_summary_job_config(&state.pool, req).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "save summary job config failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_summary_rollup_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<UserIdQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let user_id = resolve_scope_user_id(&auth, q.user_id);
    match configs::get_summary_rollup_job_config(&state.pool, user_id.as_str()).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get summary rollup job config failed", "detail": err})),
        ),
    }
}

pub(super) async fn put_summary_rollup_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<UpsertSummaryRollupJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    req.user_id = resolve_scope_user_id(&auth, Some(req.user_id.clone()));

    match configs::upsert_summary_rollup_job_config(&state.pool, req).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "save summary rollup job config failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_agent_memory_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<UserIdQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let user_id = resolve_scope_user_id(&auth, q.user_id);
    match configs::get_agent_memory_job_config(&state.pool, user_id.as_str()).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get agent memory job config failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_task_execution_summary_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<UserIdQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let user_id = resolve_scope_user_id(&auth, q.user_id);
    match configs::get_task_execution_summary_job_config(&state.pool, user_id.as_str()).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get task execution summary job config failed", "detail": err})),
        ),
    }
}

pub(super) async fn put_task_execution_summary_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<UpsertTaskExecutionSummaryJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    req.user_id = resolve_scope_user_id(&auth, Some(req.user_id.clone()));

    match configs::upsert_task_execution_summary_job_config(&state.pool, req).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "save task execution summary job config failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_task_execution_rollup_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<UserIdQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let user_id = resolve_scope_user_id(&auth, q.user_id);
    match configs::get_task_execution_rollup_job_config(&state.pool, user_id.as_str()).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get task execution rollup job config failed", "detail": err})),
        ),
    }
}

pub(super) async fn put_task_execution_rollup_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<UpsertTaskExecutionRollupJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    req.user_id = resolve_scope_user_id(&auth, Some(req.user_id.clone()));

    match configs::upsert_task_execution_rollup_job_config(&state.pool, req).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "save task execution rollup job config failed", "detail": err})),
        ),
    }
}

pub(super) async fn put_agent_memory_job_config(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(mut req): Json<UpsertAgentMemoryJobConfigRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    req.user_id = resolve_scope_user_id(&auth, Some(req.user_id.clone()));

    match configs::upsert_agent_memory_job_config(&state.pool, req).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "save agent memory job config failed", "detail": err})),
        ),
    }
}
