use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{CreateMemoryAgentRequest, MemoryAgentSkill, UpdateMemoryAgentRequest};
use crate::repositories::{agents as agents_repo, sessions};

use super::{
    ensure_agent_manage_access, ensure_agent_read_access, require_auth, resolve_scope_user_id,
    resolve_visible_user_ids, SharedState,
};

#[derive(Debug, Deserialize)]
pub(super) struct ListAgentsQuery {
    user_id: Option<String>,
    enabled: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ListAgentSessionsQuery {
    user_id: Option<String>,
    project_id: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateAgentRequest {
    user_id: Option<String>,
    name: String,
    description: Option<String>,
    category: Option<String>,
    role_definition: String,
    skills: Option<Vec<MemoryAgentSkill>>,
    skill_ids: Option<Vec<String>>,
    default_skill_ids: Option<Vec<String>>,
    mcp_policy: Option<Value>,
    project_policy: Option<Value>,
    enabled: Option<bool>,
}

pub(super) async fn list_agents(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListAgentsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, q.user_id);
    let visible_user_ids = resolve_visible_user_ids(scope_user_id.as_str());
    let limit = q.limit.unwrap_or(100);
    let offset = q.offset.unwrap_or(0);

    match agents_repo::list_agents(
        &state.pool,
        visible_user_ids.as_slice(),
        q.enabled,
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list agents failed", "detail": err})),
        ),
    }
}

pub(super) async fn list_agent_sessions(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Query(q): Query<ListAgentSessionsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_agent_read_access(state.as_ref(), &auth, agent_id.as_str()).await {
        return err;
    }

    let scope_user_id = resolve_scope_user_id(&auth, q.user_id);
    let limit = q.limit.unwrap_or(100);
    let offset = q.offset.unwrap_or(0);
    let status = q.status.as_deref().or(Some("active"));

    match sessions::list_sessions_by_agent(
        &state.pool,
        scope_user_id.as_str(),
        agent_id.as_str(),
        q.project_id.as_deref(),
        status,
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list agent sessions failed", "detail": err})),
        ),
    }
}

pub(super) async fn create_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<CreateAgentRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let name = req.name.trim().to_string();
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "name is required"})),
        );
    }

    let role_definition = req.role_definition.trim().to_string();
    if role_definition.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "role_definition is required"})),
        );
    }

    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let create_req = CreateMemoryAgentRequest {
        user_id: scope_user_id,
        name,
        description: req.description,
        category: req.category,
        role_definition,
        skills: req.skills,
        skill_ids: req.skill_ids,
        default_skill_ids: req.default_skill_ids,
        mcp_policy: req.mcp_policy,
        project_policy: req.project_policy,
        enabled: req.enabled,
    };

    match agents_repo::create_agent(&state.pool, create_req).await {
        Ok(agent) => (StatusCode::OK, Json(json!(agent))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create agent failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    match ensure_agent_read_access(state.as_ref(), &auth, agent_id.as_str()).await {
        Ok(agent) => (StatusCode::OK, Json(json!(agent))),
        Err(err) => err,
    }
}

pub(super) async fn update_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(req): Json<UpdateMemoryAgentRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_agent_manage_access(state.as_ref(), &auth, agent_id.as_str()).await {
        return err;
    }

    match agents_repo::update_agent(&state.pool, agent_id.as_str(), req).await {
        Ok(Some(agent)) => (StatusCode::OK, Json(json!(agent))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "update agent failed", "detail": err})),
        ),
    }
}

pub(super) async fn delete_agent(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_agent_manage_access(state.as_ref(), &auth, agent_id.as_str()).await {
        return err;
    }

    match agents_repo::delete_agent(&state.pool, agent_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete agent failed", "detail": err})),
        ),
    }
}

pub(super) async fn get_agent_runtime_context(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    if let Err(err) = ensure_agent_read_access(state.as_ref(), &auth, agent_id.as_str()).await {
        return err;
    }

    match agents_repo::get_runtime_context(&state.pool, agent_id.as_str()).await {
        Ok(Some(context)) => (StatusCode::OK, Json(json!(context))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load runtime context failed", "detail": err})),
        ),
    }
}
