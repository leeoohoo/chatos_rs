use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::models::{MemoryProject, MemoryProjectAgentLink, ProjectMemory};
use crate::repositories::{
    project_agent_links as project_agent_links_repo, projects as projects_repo,
};
use crate::services::memory_engine_client;

use super::super::{
    default_project_name, normalize_project_scope_id, pick_latest_timestamp, SharedState,
};
use super::contracts::ListContactProjectsQuery;
use super::support::{internal_error, resolve_contact};

pub(in crate::api) async fn list_contact_projects(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(contact_id): Path<String>,
    Query(q): Query<ListContactProjectsQuery>,
) -> (StatusCode, Json<Value>) {
    let contact = match resolve_contact(&state, &headers, contact_id.as_str()).await {
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
        Err(err) => return internal_error("list contact projects failed", err),
    };

    let memories = match memory_engine_client::list_project_memories_by_contact(
        &state.config,
        &state.pool,
        contact.user_id.as_str(),
        contact.id.as_str(),
        2_000,
        0,
    )
    .await {
        Ok(items) => items,
        Err(err) => return internal_error("list contact project memories failed", err),
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
    if let Ok(thread_rows) = memory_engine_client::list_threads_by_label(
        &state.config,
        contact.user_id.as_str(),
        format!("agent:{}", contact.agent_id).as_str(),
        Some("active"),
        500,
        0,
    )
    .await
    {
        for thread in thread_rows {
            let pid = normalize_project_scope_id(
                thread
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("legacy_session_mapping"))
                    .and_then(|mapping| mapping.get("project_id"))
                    .and_then(Value::as_str),
            );
            if !ordered_project_ids
                .iter()
                .any(|existing| existing == pid.as_str())
            {
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
        Err(err) => return internal_error("load projects failed", err),
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

pub(in crate::api) async fn list_contact_project_summaries(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((contact_id, project_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    let contact = match resolve_contact(&state, &headers, contact_id.as_str()).await {
        Ok(contact) => contact,
        Err(err) => return err,
    };

    let normalized_project_id = normalize_project_scope_id(Some(project_id.as_str()));
    let thread_label = format!("contact_project:{}:{}", contact.id, normalized_project_id);

    match memory_engine_client::list_summaries_by_thread_label(
        &state.config,
        contact.user_id.as_str(),
        thread_label.as_str(),
        Some("thread_incremental"),
        Some("done"),
        None,
        None,
        500,
        0,
    )
    .await {
        Ok(items) => (
            StatusCode::OK,
            Json(json!({
                "session_id": Value::Null,
                "items": items,
            })),
        ),
        Err(err) => internal_error("list contact project summaries failed", err),
    }
}
