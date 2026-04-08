use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{CreateMemoryAgentRequest, MemoryAgentSkill, UpdateMemoryAgentRequest};
use crate::repositories::{
    agents as agents_repo, auth as auth_repo, contacts as contacts_repo, sessions,
};

use super::{
    ensure_agent_manage_access, ensure_agent_read_access, require_auth, resolve_scope_user_id,
    resolve_visible_user_ids, SharedState,
};

#[derive(Debug, Deserialize)]
pub(super) struct ListAgentsQuery {
    user_id: Option<String>,
    include_shared: Option<bool>,
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
    model_config_id: Option<String>,
    role_definition: String,
    plugin_sources: Option<Vec<String>>,
    skills: Option<Vec<MemoryAgentSkill>>,
    skill_ids: Option<Vec<String>>,
    default_skill_ids: Option<Vec<String>>,
    mcp_policy: Option<Value>,
    project_policy: Option<Value>,
    enabled: Option<bool>,
}

const CLONE_META_KEY: &str = "__agent_workspace_clone_meta";

fn with_clone_meta_project_policy(
    project_policy: Option<Value>,
    source_agent_id: &str,
) -> Option<Value> {
    let mut root = match project_policy {
        Some(Value::Object(map)) => map,
        Some(other) => {
            let mut map = serde_json::Map::new();
            map.insert("__original_project_policy".to_string(), other);
            map
        }
        None => serde_json::Map::new(),
    };
    root.insert(
        CLONE_META_KEY.to_string(),
        json!({
            "source_agent_id": source_agent_id,
            "source_user_id": auth_repo::ADMIN_USER_ID,
        }),
    );
    Some(Value::Object(root))
}

async fn ensure_user_clone_for_source_agent(
    state: &SharedState,
    scope_user_id: &str,
    source_agent: &crate::models::MemoryAgent,
) -> Result<crate::models::MemoryAgent, String> {
    if source_agent.user_id == scope_user_id {
        return Ok(source_agent.clone());
    }
    if source_agent.user_id != auth_repo::ADMIN_USER_ID {
        return Err("forbidden source agent owner".to_string());
    }

    if let Some(existing) = agents_repo::get_user_clone_by_source_agent_id(
        &state.pool,
        scope_user_id,
        source_agent.id.as_str(),
    )
    .await?
    {
        return Ok(existing);
    }

    let req = CreateMemoryAgentRequest {
        user_id: scope_user_id.to_string(),
        name: source_agent.name.clone(),
        description: source_agent.description.clone(),
        category: source_agent.category.clone(),
        model_config_id: source_agent.model_config_id.clone(),
        role_definition: source_agent.role_definition.clone(),
        plugin_sources: Some(source_agent.plugin_sources.clone()),
        skills: Some(source_agent.skills.clone()),
        skill_ids: Some(source_agent.skill_ids.clone()),
        default_skill_ids: Some(source_agent.default_skill_ids.clone()),
        mcp_policy: source_agent.mcp_policy.clone(),
        project_policy: with_clone_meta_project_policy(
            source_agent.project_policy.clone(),
            source_agent.id.as_str(),
        ),
        enabled: Some(source_agent.enabled),
    };
    match agents_repo::create_agent(&state.pool, req.clone()).await {
        Ok(agent) => Ok(agent),
        Err(err) if err.contains("unknown skill_ids") || err.contains("unknown plugin_sources") => {
            let fallback_req = CreateMemoryAgentRequest {
                user_id: req.user_id,
                name: req.name,
                description: req.description,
                category: req.category,
                model_config_id: req.model_config_id,
                role_definition: req.role_definition,
                plugin_sources: Some(Vec::new()),
                skills: req.skills,
                skill_ids: Some(Vec::new()),
                default_skill_ids: Some(Vec::new()),
                mcp_policy: req.mcp_policy,
                project_policy: req.project_policy,
                enabled: req.enabled,
            };
            agents_repo::create_agent(&state.pool, fallback_req).await
        }
        Err(err) => Err(err),
    }
}

async fn backfill_contact_agent_clones_for_user(
    state: &SharedState,
    scope_user_id: &str,
) -> Result<(), String> {
    if scope_user_id.trim().is_empty() || scope_user_id == auth_repo::ADMIN_USER_ID {
        return Ok(());
    }

    let contacts =
        contacts_repo::list_contacts(&state.pool, scope_user_id, Some("active"), 2000, 0).await?;
    for contact in contacts {
        let Some(source_agent) =
            agents_repo::get_agent_by_id(&state.pool, contact.agent_id.as_str()).await?
        else {
            continue;
        };
        if source_agent.user_id == scope_user_id {
            continue;
        }
        if source_agent.user_id != auth_repo::ADMIN_USER_ID {
            continue;
        }

        let cloned_agent =
            ensure_user_clone_for_source_agent(state, scope_user_id, &source_agent).await?;
        if cloned_agent.id == contact.agent_id {
            continue;
        }
        let _ = contacts_repo::update_contact_agent(
            &state.pool,
            contact.id.as_str(),
            cloned_agent.id.as_str(),
            contact
                .agent_name_snapshot
                .clone()
                .or_else(|| Some(cloned_agent.name.clone())),
        )
        .await?;
    }
    Ok(())
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
    let include_shared = q.include_shared.unwrap_or(true);
    if !include_shared {
        if let Err(err) =
            backfill_contact_agent_clones_for_user(&state, scope_user_id.as_str()).await
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "backfill contact agent clones failed", "detail": err})),
            );
        }
    }
    let visible_user_ids = if include_shared {
        resolve_visible_user_ids(scope_user_id.as_str())
    } else {
        vec![scope_user_id.clone()]
    };
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
    let status = q
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let agent_sessions = sessions::list_sessions_by_agent(
        &state.pool,
        scope_user_id.as_str(),
        agent_id.as_str(),
        q.project_id.as_deref(),
        status,
        limit,
        offset,
    )
    .await;

    match agent_sessions {
        Ok(items) if !items.is_empty() => (StatusCode::OK, Json(json!({"items": items}))),
        Ok(_) => {
            let contact = contacts_repo::get_contact_by_user_and_agent(
                &state.pool,
                scope_user_id.as_str(),
                agent_id.as_str(),
            )
            .await;
            match contact {
                Ok(Some(contact)) => {
                    match sessions::list_sessions_by_contact(
                        &state.pool,
                        scope_user_id.as_str(),
                        contact.id.as_str(),
                        q.project_id.as_deref(),
                        status,
                        limit,
                        offset,
                    )
                    .await
                    {
                        Ok(rows) => (StatusCode::OK, Json(json!({"items": rows}))),
                        Err(err) => (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                json!({"error": "list agent sessions by contact failed", "detail": err}),
                            ),
                        ),
                    }
                }
                Ok(None) => (StatusCode::OK, Json(json!({"items": []}))),
                Err(err) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        json!({"error": "resolve contact for agent sessions failed", "detail": err}),
                    ),
                ),
            }
        }
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
        model_config_id: req.model_config_id,
        role_definition,
        plugin_sources: req.plugin_sources,
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

pub(super) async fn internal_get_agent(
    State(state): State<SharedState>,
    Path(agent_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match agents_repo::get_agent_by_id(&state.pool, agent_id.as_str()).await {
        Ok(Some(agent)) => (StatusCode::OK, Json(json!(agent))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load agent failed", "detail": err})),
        ),
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

pub(super) async fn internal_get_agent_runtime_context(
    State(state): State<SharedState>,
    Path(agent_id): Path<String>,
) -> (StatusCode, Json<Value>) {
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
