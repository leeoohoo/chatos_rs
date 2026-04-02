use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{CreateContactRequest, CreateMemoryAgentRequest, MemoryAgent};
use crate::repositories::{
    agents as agents_repo, auth as auth_repo, contacts as contacts_repo, sessions,
};

use super::{
    ensure_agent_read_access, ensure_contact_manage_access, require_auth, resolve_scope_user_id,
    SharedState,
};

#[derive(Debug, Deserialize)]
pub(super) struct ListContactsQuery {
    user_id: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateContactPayload {
    user_id: Option<String>,
    agent_id: String,
    agent_name_snapshot: Option<String>,
}

const CLONE_META_KEY: &str = "__chatos_clone_meta";

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

async fn ensure_user_managed_agent_for_contact(
    state: &SharedState,
    scope_user_id: &str,
    source_agent: &MemoryAgent,
) -> Result<MemoryAgent, (StatusCode, Json<Value>)> {
    if source_agent.user_id == scope_user_id {
        return Ok(source_agent.clone());
    }
    if source_agent.user_id != auth_repo::ADMIN_USER_ID {
        return Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))));
    }

    match agents_repo::get_user_clone_by_source_agent_id(
        &state.pool,
        scope_user_id,
        source_agent.id.as_str(),
    )
    .await
    {
        Ok(Some(existing)) => return Ok(existing),
        Ok(None) => {}
        Err(err) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load cloned agent failed", "detail": err})),
            ))
        }
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
            agents_repo::create_agent(&state.pool, fallback_req)
                .await
                .map_err(|fallback_err| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({
                            "error": "clone admin agent for user failed",
                            "detail": fallback_err,
                            "source_detail": err,
                        })),
                    )
                })
        }
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "clone admin agent for user failed", "detail": err})),
        )),
    }
}

pub(super) async fn list_contacts(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListContactsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, q.user_id);
    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);
    let status = q
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("active");

    match contacts_repo::list_contacts(
        &state.pool,
        scope_user_id.as_str(),
        Some(status),
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list contacts failed", "detail": err})),
        ),
    }
}

pub(super) async fn create_contact(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<CreateContactPayload>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let agent_id = req.agent_id.trim().to_string();
    if agent_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "agent_id is required"})),
        );
    }

    let agent = match ensure_agent_read_access(state.as_ref(), &auth, agent_id.as_str()).await {
        Ok(agent) => agent,
        Err(err) => return err,
    };
    if !agent.enabled {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "agent is disabled"})),
        );
    }

    let managed_agent =
        match ensure_user_managed_agent_for_contact(&state, scope_user_id.as_str(), &agent).await {
            Ok(value) => value,
            Err(err) => return err,
        };
    if !managed_agent.enabled {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "agent is disabled"})),
        );
    }

    let snapshot_name = req
        .agent_name_snapshot
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Some(managed_agent.name.clone()));

    if managed_agent.id != agent.id {
        match contacts_repo::get_contact_by_user_and_agent(
            &state.pool,
            scope_user_id.as_str(),
            managed_agent.id.as_str(),
        )
        .await
        {
            Ok(Some(contact)) => {
                return (
                    StatusCode::OK,
                    Json(json!({"created": false, "contact": contact})),
                )
            }
            Ok(None) => {}
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "load cloned contact failed", "detail": err})),
                )
            }
        }

        match contacts_repo::get_contact_by_user_and_agent(
            &state.pool,
            scope_user_id.as_str(),
            agent.id.as_str(),
        )
        .await
        {
            Ok(Some(legacy_contact)) => {
                match contacts_repo::update_contact_agent(
                    &state.pool,
                    legacy_contact.id.as_str(),
                    managed_agent.id.as_str(),
                    snapshot_name.clone(),
                )
                .await
                {
                    Ok(Some(contact)) => {
                        return (
                            StatusCode::OK,
                            Json(json!({"created": false, "contact": contact})),
                        )
                    }
                    Ok(None) => {}
                    Err(err) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                json!({"error": "rebind legacy contact agent failed", "detail": err}),
                            ),
                        )
                    }
                }
            }
            Ok(None) => {}
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "load legacy contact failed", "detail": err})),
                )
            }
        }
    }

    let create_req = CreateContactRequest {
        user_id: scope_user_id,
        agent_id: managed_agent.id,
        agent_name_snapshot: snapshot_name,
    };

    match contacts_repo::create_contact_idempotent(&state.pool, create_req).await {
        Ok((contact, created)) => {
            let status = if created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            (
                status,
                Json(json!({"created": created, "contact": contact})),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create contact failed", "detail": err})),
        ),
    }
}

pub(super) async fn delete_contact(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(contact_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let contact =
        match ensure_contact_manage_access(state.as_ref(), &auth, contact_id.as_str()).await {
            Ok(contact) => contact,
            Err(err) => return err,
        };

    if let Err(err) = sessions::archive_sessions_by_contact(
        &state.pool,
        contact.user_id.as_str(),
        contact.id.as_str(),
        contact.agent_id.as_str(),
    )
    .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "archive contact sessions failed", "detail": err})),
        );
    }

    match contacts_repo::delete_contact_by_id(&state.pool, contact_id.as_str()).await {
        Ok(true) => (StatusCode::OK, Json(json!({"success": true}))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "contact not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "delete contact failed", "detail": err})),
        ),
    }
}
