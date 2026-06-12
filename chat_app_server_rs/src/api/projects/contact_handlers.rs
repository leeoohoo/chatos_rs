use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::core::validation::normalize_non_empty;
use crate::models::memory_mapping_types::{MemoryContactDto, SyncProjectAgentLinkRequestDto};
use crate::services::chatos_memory_mappings;
use crate::services::realtime::publish_project_members_updated;

use super::contracts::{AddProjectContactRequest, ProjectContactsQuery};

pub(super) async fn list_project_contacts(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<ProjectContactsQuery>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_project(&id, &auth).await {
        return map_project_access_error(err);
    }

    match chatos_memory_mappings::list_project_contacts(
        id.as_str(),
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (
            StatusCode::OK,
            Json(serde_json::to_value(items).unwrap_or(Value::Null)),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "list project contacts failed", "detail": err})),
        ),
    }
}

pub(super) async fn add_project_contact(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<AddProjectContactRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_project(&id, &auth).await {
        return map_project_access_error(err);
    }

    let Some(contact_id) = normalize_non_empty(req.contact_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "contact_id is required"})),
        );
    };

    let page_size = 500;
    let mut matched_contact: Option<MemoryContactDto> = None;
    for page in 0..20 {
        let offset = page * page_size;
        let rows = match chatos_memory_mappings::list_memory_contacts(
            Some(auth.user_id.as_str()),
            Some(page_size),
            offset,
        )
        .await
        {
            Ok(items) => items,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "load contacts failed", "detail": err})),
                );
            }
        };
        if let Some(contact) = rows.into_iter().find(|item| item.id == contact_id) {
            matched_contact = Some(contact);
            break;
        }
    }

    let Some(contact) = matched_contact else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "contact not found"})),
        );
    };

    match chatos_memory_mappings::sync_project_agent_link(&SyncProjectAgentLinkRequestDto {
        user_id: Some(auth.user_id.clone()),
        project_id: Some(id.clone()),
        agent_id: Some(contact.agent_id),
        contact_id: Some(contact.id),
        session_id: None,
        last_message_at: None,
        status: Some("active".to_string()),
    })
    .await
    {
        Ok(link) => {
            publish_project_members_updated(
                auth.user_id.as_str(),
                id.as_str(),
                "project_contact_added",
                Some(contact_id.as_str()),
            );
            (
                StatusCode::OK,
                Json(serde_json::to_value(link).unwrap_or(Value::Null)),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "add project contact failed", "detail": err})),
        ),
    }
}

pub(super) async fn remove_project_contact(
    auth: AuthUser,
    Path((id, contact_id)): Path<(String, String)>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_project(&id, &auth).await {
        return map_project_access_error(err);
    }

    let linked_rows = match chatos_memory_mappings::list_project_contacts(id.as_str(), Some(500), 0)
        .await
    {
        Ok(items) => items,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "load project contacts failed", "detail": err})),
            );
        }
    };

    let Some(linked) = linked_rows
        .into_iter()
        .find(|item| item.contact_id == contact_id)
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "project contact not found"})),
        );
    };

    match chatos_memory_mappings::sync_project_agent_link(&SyncProjectAgentLinkRequestDto {
        user_id: Some(auth.user_id.clone()),
        project_id: Some(id.clone()),
        agent_id: Some(linked.agent_id),
        contact_id: Some(linked.contact_id),
        session_id: None,
        last_message_at: None,
        status: Some("archived".to_string()),
    })
    .await
    {
        Ok(_) => {
            publish_project_members_updated(
                auth.user_id.as_str(),
                id.as_str(),
                "project_contact_removed",
                Some(contact_id.as_str()),
            );
            (StatusCode::OK, Json(serde_json::json!({"success": true})))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "remove project contact failed", "detail": err})),
        ),
    }
}
