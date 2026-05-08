use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{
    UpsertAgentMemoryJobConfigRequest, UpsertSummaryJobConfigRequest,
    UpsertSummaryRollupJobConfigRequest,
};
use crate::services::memory_engine_client;

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
    match memory_engine_client::get_global_summary_job_config(&state.config, user_id.as_str()).await {
        Ok(cfg) => (
            StatusCode::OK,
            Json(json!({
                "config": Some(cfg),
                "config_role": "memory_engine_summary",
                "backend": "memory_engine",
                "memory_engine_enabled": true,
            })),
        ),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
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

    match memory_engine_client::put_global_summary_job_config(&state.config, &req).await {
        Ok(cfg) => (
            StatusCode::OK,
            Json(json!({
                "config": cfg,
                "config_role": "memory_engine_summary",
                "backend": "memory_engine",
                "memory_engine_enabled": true,
            })),
        ),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
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
    match memory_engine_client::get_global_rollup_job_config(&state.config, user_id.as_str()).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
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

    match memory_engine_client::put_global_rollup_job_config(&state.config, &req).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
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
    match memory_engine_client::get_global_agent_memory_job_config(&state.config, user_id.as_str()).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "get agent memory job config failed", "detail": err})),
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

    match memory_engine_client::put_global_agent_memory_job_config(&state.config, &req).await {
        Ok(cfg) => (StatusCode::OK, Json(json!(cfg))),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "save agent memory job config failed", "detail": err})),
        ),
    }
}
