use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{MemoryProject, MemoryProjectAgentLink, ProjectMemory};
use crate::repositories::{
    memories as memories_repo, project_agent_links as project_agent_links_repo,
    projects as projects_repo, sessions, summaries as summaries_repo,
};

use super::{
    default_project_name, ensure_contact_access, normalize_project_scope_id, pick_latest_timestamp,
    require_auth, SharedState,
};

#[derive(Debug, Deserialize)]
pub(super) struct ListContactMemoriesQuery {
    project_id: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ListContactProjectsQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

pub(super) async fn list_contact_projects(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(contact_id): Path<String>,
    Query(q): Query<ListContactProjectsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let contact = match ensure_contact_access(state.as_ref(), &auth, contact_id.as_str()).await {
        Ok(contact) => contact,
        Err(err) => return err,
    };

    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);
    let links = match project_agent_links_repo::list_project_agent_links_by_contact(
        &state.pool,
        contact.user_id.as_str(),
        contact.id.as_str(),
        Some("active"),
        limit,
        offset,
    )
    .await
    {
        Ok(items) => items,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "list contact projects failed", "detail": err})),
            )
        }
    };

    let memories =
        match memories_repo::list_project_memories_by_contact(
            &state.pool,
            contact.user_id.as_str(),
            contact.id.as_str(),
            2_000,
            0,
        )
        .await
        {
            Ok(items) => items,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "list contact project memories failed", "detail": err})),
                )
            }
        };

    let mut latest_memory_by_project: HashMap<String, ProjectMemory> = HashMap::new();
    for memory in memories {
        let pid = normalize_project_scope_id(Some(memory.project_id.as_str()));
        let should_replace = latest_memory_by_project
            .get(pid.as_str())
            .map(|existing| existing.updated_at.as_str() <= memory.updated_at.as_str())
            .unwrap_or(true);
        if should_replace {
            latest_memory_by_project.insert(pid, memory);
        }
    }

    let mut ordered_project_ids: Vec<String> = Vec::new();
    let mut link_by_project: HashMap<String, MemoryProjectAgentLink> = HashMap::new();
    for link in links {
        let pid = normalize_project_scope_id(Some(link.project_id.as_str()));
        if !link_by_project.contains_key(pid.as_str()) {
            ordered_project_ids.push(pid.clone());
            link_by_project.insert(pid, link);
        }
    }
    for pid in latest_memory_by_project.keys() {
        if !link_by_project.contains_key(pid.as_str()) {
            ordered_project_ids.push(pid.clone());
        }
    }
    if let Ok(session_rows) = sessions::list_sessions_by_agent(
        &state.pool,
        contact.user_id.as_str(),
        contact.agent_id.as_str(),
        None,
        Some("active"),
        500,
        0,
    )
    .await
    {
        for session in session_rows {
            let pid = normalize_project_scope_id(session.project_id.as_deref());
            if !ordered_project_ids.iter().any(|existing| existing == pid.as_str()) {
                ordered_project_ids.push(pid);
            }
        }
    }

    if ordered_project_ids.is_empty() {
        return (StatusCode::OK, Json(json!({"items": []})));
    }

    let projects = match projects_repo::list_projects_by_ids(
        &state.pool,
        contact.user_id.as_str(),
        ordered_project_ids.as_slice(),
    )
    .await
    {
        Ok(items) => items,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load projects failed", "detail": err})),
            )
        }
    };
    let project_map: HashMap<String, MemoryProject> = projects
        .into_iter()
        .map(|project| (project.project_id.clone(), project))
        .collect();

    let mut items: Vec<Value> = Vec::new();
    for project_id in ordered_project_ids {
        let project = project_map.get(project_id.as_str());
        let link = link_by_project.get(project_id.as_str());
        let latest_memory = latest_memory_by_project.get(project_id.as_str());
        let updated_at = pick_latest_timestamp(&[
            link.map(|v| v.updated_at.as_str()),
            latest_memory.map(|v| v.updated_at.as_str()),
            project.map(|v| v.updated_at.as_str()),
        ])
        .unwrap_or_else(crate::repositories::now_rfc3339);

        items.push(json!({
            "project_id": project_id,
            "project_name": project
                .map(|v| v.name.clone())
                .unwrap_or_else(|| default_project_name(project_id.as_str())),
            "project_root": project.and_then(|v| v.root_path.clone()),
            "status": project
                .map(|v| v.status.clone())
                .unwrap_or_else(|| "active".to_string()),
            "is_virtual": project
                .map(|v| v.is_virtual)
                .unwrap_or_else(|| if project_id == "0" { 1 } else { 0 }),
            "has_memory": latest_memory.is_some(),
            "memory_version": latest_memory.map(|v| v.memory_version).unwrap_or(0),
            "recall_summarized": latest_memory.map(|v| v.recall_summarized).unwrap_or(0),
            "last_source_at": latest_memory.and_then(|v| v.last_source_at.clone()),
            "updated_at": updated_at,
        }));
    }

    items.sort_by(|left, right| {
        let left_updated = left
            .get("updated_at")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let right_updated = right
            .get("updated_at")
            .and_then(Value::as_str)
            .unwrap_or_default();
        right_updated.cmp(left_updated)
    });

    (StatusCode::OK, Json(json!({"items": items})))
}

pub(super) async fn list_contact_project_memories(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(contact_id): Path<String>,
    Query(q): Query<ListContactMemoriesQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let contact = match ensure_contact_access(state.as_ref(), &auth, contact_id.as_str()).await {
        Ok(contact) => contact,
        Err(err) => return err,
    };

    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);
    let target_project_id = q
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let list_result = match target_project_id {
        Some(project_id) => {
            memories_repo::list_project_memories(
                &state.pool,
                contact.user_id.as_str(),
                contact.id.as_str(),
                project_id.as_str(),
                limit,
                offset,
            )
            .await
        }
        None => {
            memories_repo::list_project_memories_by_contact(
                &state.pool,
                contact.user_id.as_str(),
                contact.id.as_str(),
                limit,
                offset,
            )
            .await
        }
    };

    match list_result {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list project memories failed", "detail": err})),
        ),
    }
}

pub(super) async fn list_contact_project_memories_by_project(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((contact_id, project_id)): Path<(String, String)>,
    Query(q): Query<ListContactMemoriesQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let contact = match ensure_contact_access(state.as_ref(), &auth, contact_id.as_str()).await {
        Ok(contact) => contact,
        Err(err) => return err,
    };

    let project_id = project_id.trim().to_string();
    if project_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "project_id is required"})),
        );
    }

    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);
    match memories_repo::list_project_memories(
        &state.pool,
        contact.user_id.as_str(),
        contact.id.as_str(),
        project_id.as_str(),
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list project memories failed", "detail": err})),
        ),
    }
}

pub(super) async fn list_contact_project_summaries(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((contact_id, project_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let contact = match ensure_contact_access(state.as_ref(), &auth, contact_id.as_str()).await {
        Ok(contact) => contact,
        Err(err) => return err,
    };

    let normalized_project_id = normalize_project_scope_id(Some(project_id.as_str()));
    let session = match sessions::get_active_session_by_contact_project(
        &state.pool,
        contact.user_id.as_str(),
        normalized_project_id.as_str(),
        Some(contact.id.as_str()),
        Some(contact.agent_id.as_str()),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "resolve contact project session failed", "detail": err})),
            )
        }
    };

    let Some(session) = session else {
        return (
            StatusCode::OK,
            Json(json!({
                "session_id": Value::Null,
                "items": [],
            })),
        );
    };

    match summaries_repo::list_all_summaries_by_session(&state.pool, session.id.as_str()).await {
        Ok(items) => (
            StatusCode::OK,
            Json(json!({
                "session_id": session.id,
                "items": items,
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list contact project summaries failed", "detail": err})),
        ),
    }
}

pub(super) async fn list_contact_agent_recalls(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(contact_id): Path<String>,
    Query(q): Query<ListContactMemoriesQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let contact = match ensure_contact_access(state.as_ref(), &auth, contact_id.as_str()).await {
        Ok(contact) => contact,
        Err(err) => return err,
    };

    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);
    match memories_repo::list_agent_recalls(
        &state.pool,
        contact.user_id.as_str(),
        contact.agent_id.as_str(),
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list agent recalls failed", "detail": err})),
        ),
    }
}
