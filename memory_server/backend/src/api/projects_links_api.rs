use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{Contact, MemoryProjectAgentLink};
use crate::repositories::{
    contacts as contacts_repo, project_agent_links as project_agent_links_repo,
    projects as projects_repo,
};

use super::{
    default_project_name, normalize_optional_text, normalize_project_scope_id,
    pick_latest_timestamp, require_auth, resolve_scope_user_id, SharedState,
};

#[derive(Debug, Deserialize)]
pub(super) struct ListProjectContactsQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SyncProjectAgentLinkRequest {
    user_id: Option<String>,
    project_id: Option<String>,
    agent_id: Option<String>,
    contact_id: Option<String>,
    session_id: Option<String>,
    last_message_at: Option<String>,
    status: Option<String>,
}

pub(super) async fn list_project_contacts(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(project_id): Path<String>,
    Query(q): Query<ListProjectContactsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, None);
    let normalized_project_id = normalize_project_scope_id(Some(project_id.as_str()));
    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);

    let links = match project_agent_links_repo::list_project_agent_links_by_project(
        &state.pool,
        scope_user_id.as_str(),
        normalized_project_id.as_str(),
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
                Json(json!({"error": "list project contacts failed", "detail": err})),
            )
        }
    };

    if links.is_empty() {
        return (StatusCode::OK, Json(json!({"items": []})));
    }

    let mut ordered_contact_ids: Vec<String> = Vec::new();
    let mut link_by_contact_id: HashMap<String, MemoryProjectAgentLink> = HashMap::new();
    for link in links {
        let contact_id = normalize_optional_text(link.contact_id.as_deref());
        let Some(contact_id) = contact_id else {
            continue;
        };
        if !link_by_contact_id.contains_key(contact_id.as_str()) {
            ordered_contact_ids.push(contact_id.clone());
            link_by_contact_id.insert(contact_id, link);
        }
    }

    if ordered_contact_ids.is_empty() {
        return (StatusCode::OK, Json(json!({"items": []})));
    }

    let contacts = match contacts_repo::list_contacts_by_ids(
        &state.pool,
        scope_user_id.as_str(),
        ordered_contact_ids.as_slice(),
        Some("active"),
    )
    .await
    {
        Ok(items) => items,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load project contacts failed", "detail": err})),
            )
        }
    };
    let contact_map: HashMap<String, Contact> = contacts
        .into_iter()
        .map(|contact| (contact.id.clone(), contact))
        .collect();

    let mut items: Vec<Value> = Vec::new();
    for contact_id in ordered_contact_ids {
        let Some(contact) = contact_map.get(contact_id.as_str()) else {
            continue;
        };
        let link = link_by_contact_id.get(contact_id.as_str());
        let updated_at = pick_latest_timestamp(&[
            link.map(|v| v.updated_at.as_str()),
            Some(contact.updated_at.as_str()),
        ])
        .unwrap_or_else(crate::repositories::now_rfc3339);

        items.push(json!({
            "project_id": normalized_project_id,
            "contact_id": contact.id,
            "agent_id": contact.agent_id,
            "agent_name_snapshot": contact.agent_name_snapshot,
            "contact_status": contact.status,
            "link_status": link
                .map(|v| v.status.clone())
                .unwrap_or_else(|| "active".to_string()),
            "latest_session_id": link.and_then(|v| v.latest_session_id.clone()),
            "last_bound_at": link.map(|v| v.last_bound_at.clone()),
            "last_message_at": link.and_then(|v| v.last_message_at.clone()),
            "created_at": contact.created_at,
            "updated_at": updated_at,
        }));
    }

    (StatusCode::OK, Json(json!({"items": items})))
}

pub(super) async fn sync_project_agent_link(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<SyncProjectAgentLinkRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let project_id = normalize_project_scope_id(req.project_id.as_deref());
    let Some(agent_id) = normalize_optional_text(req.agent_id.as_deref()) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "agent_id is required"})),
        );
    };
    let contact_id = normalize_optional_text(req.contact_id.as_deref());
    if let Some(contact_id) = contact_id.as_deref() {
        match contacts_repo::get_contact_by_id(&state.pool, contact_id).await {
            Ok(Some(contact)) => {
                if contact.user_id != scope_user_id {
                    return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
                }
            }
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "contact not found"})),
                )
            }
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "load contact failed", "detail": err})),
                )
            }
        }
    }

    match projects_repo::get_project_by_user_and_project_id(
        &state.pool,
        scope_user_id.as_str(),
        project_id.as_str(),
    )
    .await
    {
        Ok(Some(_)) => {}
        Ok(None) => {
            let _ = projects_repo::upsert_project(
                &state.pool,
                projects_repo::UpsertMemoryProjectInput {
                    user_id: scope_user_id.clone(),
                    project_id: project_id.clone(),
                    name: default_project_name(project_id.as_str()),
                    root_path: None,
                    description: None,
                    status: Some("active".to_string()),
                    is_virtual: Some(if project_id == "0" { 1 } else { 0 }),
                },
            )
            .await;
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "load project failed", "detail": err})),
            )
        }
    }

    match project_agent_links_repo::upsert_project_agent_link(
        &state.pool,
        project_agent_links_repo::UpsertProjectAgentLinkInput {
            user_id: scope_user_id,
            project_id,
            agent_id,
            contact_id,
            latest_session_id: normalize_optional_text(req.session_id.as_deref()),
            last_message_at: normalize_optional_text(req.last_message_at.as_deref()),
            status: normalize_optional_text(req.status.as_deref()),
        },
    )
    .await
    {
        Ok(Some(link)) => (StatusCode::OK, Json(json!(link))),
        Ok(None) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync project-agent link failed", "detail": "link not found after upsert"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync project-agent link failed", "detail": err})),
        ),
    }
}
