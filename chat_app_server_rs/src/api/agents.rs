// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::models::chatos_agent_types::{
    ChatosAgentSkillDto, CreateChatosAgentRequest, UpdateChatosAgentRequest,
};
use crate::services::{agent_builder, chatos_agents};

mod skills;

#[derive(Debug, Deserialize)]
struct ListAgentsQuery {
    user_id: Option<String>,
    enabled: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ListAgentSessionsQuery {
    user_id: Option<String>,
    project_id: Option<String>,
    status: Option<String>,
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
    skills: Option<Vec<ChatosAgentSkillDto>>,
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
    skills: Option<Vec<ChatosAgentSkillDto>>,
    skill_ids: Option<Vec<String>>,
    default_skill_ids: Option<Vec<String>>,
    mcp_policy: Option<Value>,
    project_policy: Option<Value>,
    enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AiCreateAgentRequest {
    user_id: Option<String>,
    model_config_id: Option<String>,
    requirement: Option<String>,
    name: Option<String>,
    category: Option<String>,
    description: Option<String>,
    role_definition: Option<String>,
    skill_ids: Option<Vec<String>>,
    skill_prompts: Option<Vec<String>>,
    enabled: Option<bool>,
    mcp_enabled: Option<bool>,
    enabled_mcp_ids: Option<Vec<String>>,
    project_id: Option<String>,
    project_root: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/agents", get(list_agents).post(create_agent))
        .route(
            "/api/agents/{agent_id}",
            get(get_agent).patch(update_agent).delete(delete_agent),
        )
        .route(
            "/api/agents/{agent_id}/runtime-context",
            get(get_agent_runtime_context),
        )
        .route("/api/agents/{agent_id}/sessions", get(list_agent_sessions))
        .route("/api/agents/ai-create", axum::routing::post(ai_create))
        .route("/api/skills", get(skills::list_skills))
        .route("/api/skills/plugins", get(skills::list_skill_plugins))
        .route("/api/skills/{skill_id}", get(skills::get_skill))
        .route("/api/skills/plugins/detail", get(skills::get_skill_plugin))
        .route(
            "/api/skills/import-git",
            axum::routing::post(skills::import_skills_from_git),
        )
        .route(
            "/api/skills/plugins/install",
            axum::routing::post(skills::install_skill_plugins),
        )
        // Compatibility aliases for existing chat_app callers while ownership
        // moves to the chatos API surface.
        .route("/api/memory-agents", get(list_agents).post(create_agent))
        .route(
            "/api/memory-agents/{agent_id}",
            get(get_agent).patch(update_agent).delete(delete_agent),
        )
        .route(
            "/api/memory-agents/{agent_id}/runtime-context",
            get(get_agent_runtime_context),
        )
        .route(
            "/api/memory-agents/{agent_id}/sessions",
            get(list_agent_sessions),
        )
        .route(
            "/api/agent-builder/ai-create",
            axum::routing::post(ai_create),
        )
}

async fn list_agents(
    auth: AuthUser,
    Query(query): Query<ListAgentsQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_scope_user_id(query.user_id, &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };

    match chatos_agents::list_agents(
        user_id.as_str(),
        query.enabled,
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list agents failed", "detail": err})),
        ),
    }
}

async fn get_agent(auth: AuthUser, Path(agent_id): Path<String>) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_agent_read_access(&auth, agent_id.as_str()).await {
        return err;
    }
    match chatos_agents::get_agent(agent_id.as_str()).await {
        Ok(Some(agent)) => (StatusCode::OK, Json(json!(agent))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get agent failed", "detail": err})),
        ),
    }
}

async fn create_agent(
    auth: AuthUser,
    Json(req): Json<CreateAgentRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_scope_user_id(req.user_id.clone(), &auth) {
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

    let payload = CreateChatosAgentRequest {
        user_id: Some(user_id),
        name,
        description: req.description,
        category: req.category,
        role_definition,
        auto_provision_task_runner_account: Some(true),
        plugin_sources: req.plugin_sources,
        skills: req.skills,
        skill_ids: req.skill_ids,
        default_skill_ids: req.default_skill_ids,
        mcp_policy: req.mcp_policy,
        project_policy: req.project_policy,
        enabled: req.enabled,
    };

    match chatos_agents::create_agent(&payload).await {
        Ok(agent) => (StatusCode::CREATED, Json(json!(agent))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create agent failed", "detail": err})),
        ),
    }
}

async fn update_agent(
    auth: AuthUser,
    Path(agent_id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_agent_manage_access(&auth, agent_id.as_str()).await {
        return err;
    }
    let payload = UpdateChatosAgentRequest {
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
    match chatos_agents::update_agent(agent_id.as_str(), &payload).await {
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

async fn delete_agent(auth: AuthUser, Path(agent_id): Path<String>) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_agent_manage_access(&auth, agent_id.as_str()).await {
        return err;
    }
    match chatos_agents::delete_agent(agent_id.as_str()).await {
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

async fn get_agent_runtime_context(
    auth: AuthUser,
    Path(agent_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_agent_read_access(&auth, agent_id.as_str()).await {
        return err;
    }
    match chatos_agents::get_agent_runtime_context(agent_id.as_str()).await {
        Ok(Some(context)) => (StatusCode::OK, Json(json!(context))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get agent runtime context failed", "detail": err})),
        ),
    }
}

async fn list_agent_sessions(
    auth: AuthUser,
    Path(agent_id): Path<String>,
    Query(query): Query<ListAgentSessionsQuery>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_agent_read_access(&auth, agent_id.as_str()).await {
        return err;
    }
    let user_id = match resolve_scope_user_id(query.user_id, &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };

    match chatos_agents::list_agent_sessions(
        agent_id.as_str(),
        user_id.as_str(),
        query.project_id.as_deref(),
        query.status.as_deref(),
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list agent sessions failed", "detail": err})),
        ),
    }
}

async fn ai_create(
    auth: AuthUser,
    Json(req): Json<AiCreateAgentRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_scope_user_id(req.user_id.clone(), &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };

    let requirement = req
        .requirement
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    if requirement.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "requirement is required"})),
        );
    }

    match agent_builder::ai_create_agent(
        user_id,
        agent_builder::AiCreateAgentRequest {
            model_config_id: req.model_config_id,
            requirement: req.requirement,
            name: req.name,
            category: req.category,
            description: req.description,
            role_definition: req.role_definition,
            skill_ids: req.skill_ids,
            skill_prompts: req.skill_prompts,
            enabled: req.enabled,
            mcp_enabled: req.mcp_enabled,
            enabled_mcp_ids: req.enabled_mcp_ids,
            project_id: req.project_id,
            project_root: req.project_root,
        },
    )
    .await
    {
        Ok(result) => (StatusCode::OK, Json(json!(result))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "ai-create agent failed", "detail": err})),
        ),
    }
}

fn resolve_scope_user_id(
    requested_user_id: Option<String>,
    auth: &AuthUser,
) -> Result<String, (StatusCode, Json<Value>)> {
    resolve_user_id(requested_user_id, auth)
}

fn can_read_owned_resource(owner_user_id: &str, auth: &AuthUser) -> bool {
    owner_user_id == auth.user_id
}

fn can_manage_owned_resource(owner_user_id: &str, auth: &AuthUser) -> bool {
    owner_user_id == auth.user_id
}

async fn ensure_agent_read_access(
    auth: &AuthUser,
    agent_id: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    match chatos_agents::get_agent(agent_id).await {
        Ok(Some(agent)) => {
            if can_read_owned_resource(agent.user_id.as_str(), auth) {
                Ok(())
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get agent failed", "detail": err})),
        )),
    }
}

async fn ensure_agent_manage_access(
    auth: &AuthUser,
    agent_id: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    match chatos_agents::get_agent(agent_id).await {
        Ok(Some(agent)) => {
            if can_manage_owned_resource(agent.user_id.as_str(), auth) {
                Ok(())
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get agent failed", "detail": err})),
        )),
    }
}
