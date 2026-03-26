use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::services::memory_server_client;

#[derive(Debug, Deserialize)]
struct ListAgentsQuery {
    user_id: Option<String>,
    enabled: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreateAgentRequest {
    user_id: Option<String>,
    name: Option<String>,
    description: Option<String>,
    category: Option<String>,
    role_definition: Option<String>,
    plugin_sources: Option<Vec<String>>,
    skills: Option<Vec<memory_server_client::MemoryAgentSkillDto>>,
    skill_ids: Option<Vec<String>>,
    default_skill_ids: Option<Vec<String>>,
    mcp_policy: Option<Value>,
    project_policy: Option<Value>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateAgentRequest {
    name: Option<String>,
    description: Option<String>,
    category: Option<String>,
    role_definition: Option<String>,
    plugin_sources: Option<Vec<String>>,
    skills: Option<Vec<memory_server_client::MemoryAgentSkillDto>>,
    skill_ids: Option<Vec<String>>,
    default_skill_ids: Option<Vec<String>>,
    mcp_policy: Option<Value>,
    project_policy: Option<Value>,
    enabled: Option<bool>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/memory-agents", get(list_agents).post(create_agent))
        .route(
            "/api/memory-agents/:agent_id",
            get(get_agent).patch(update_agent).delete(delete_agent),
        )
        .route(
            "/api/memory-agents/:agent_id/runtime-context",
            get(get_agent_runtime_context),
        )
}

async fn list_agents(
    auth: AuthUser,
    Query(query): Query<ListAgentsQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };

    match memory_server_client::list_memory_agents(
        Some(user_id.as_str()),
        query.enabled,
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list memory agents failed", "detail": err})),
        ),
    }
}

async fn get_agent(_auth: AuthUser, Path(agent_id): Path<String>) -> (StatusCode, Json<Value>) {
    match memory_server_client::get_memory_agent(agent_id.as_str()).await {
        Ok(Some(agent)) => (StatusCode::OK, Json(json!(agent))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get memory agent failed", "detail": err})),
        ),
    }
}

async fn create_agent(
    auth: AuthUser,
    Json(req): Json<CreateAgentRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id.clone(), &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };
    let name = req.name.unwrap_or_default().trim().to_string();
    let role_definition = req.role_definition.unwrap_or_default().trim().to_string();
    if name.is_empty() || role_definition.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "name 和 role_definition 为必填项"})),
        );
    }

    let payload = memory_server_client::CreateMemoryAgentRequestDto {
        user_id: Some(user_id),
        name,
        description: req.description,
        category: req.category,
        role_definition,
        plugin_sources: req.plugin_sources,
        skills: req.skills,
        skill_ids: req.skill_ids,
        default_skill_ids: req.default_skill_ids,
        mcp_policy: req.mcp_policy,
        project_policy: req.project_policy,
        enabled: req.enabled,
    };

    match memory_server_client::create_memory_agent(&payload).await {
        Ok(agent) => (StatusCode::CREATED, Json(json!(agent))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create memory agent failed", "detail": err})),
        ),
    }
}

async fn update_agent(
    _auth: AuthUser,
    Path(agent_id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> (StatusCode, Json<Value>) {
    let payload = memory_server_client::UpdateMemoryAgentRequestDto {
        name: req.name,
        description: req.description,
        category: req.category,
        role_definition: req.role_definition,
        plugin_sources: req.plugin_sources,
        skills: req.skills,
        skill_ids: req.skill_ids,
        default_skill_ids: req.default_skill_ids,
        mcp_policy: req.mcp_policy,
        project_policy: req.project_policy,
        enabled: req.enabled,
    };
    match memory_server_client::update_memory_agent(agent_id.as_str(), &payload).await {
        Ok(Some(agent)) => (StatusCode::OK, Json(json!(agent))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "update memory agent failed", "detail": err})),
        ),
    }
}

async fn delete_agent(_auth: AuthUser, Path(agent_id): Path<String>) -> (StatusCode, Json<Value>) {
    match memory_server_client::delete_memory_agent(agent_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete memory agent failed", "detail": err})),
        ),
    }
}

async fn get_agent_runtime_context(
    _auth: AuthUser,
    Path(agent_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match memory_server_client::get_memory_agent_runtime_context(agent_id.as_str()).await {
        Ok(Some(context)) => (StatusCode::OK, Json(json!(context))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get memory agent runtime context failed", "detail": err})),
        ),
    }
}
