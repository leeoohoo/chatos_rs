// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::project_access::{
    ensure_owned_project, map_project_access_error, ProjectAccessError,
};
use crate::core::project_execution::request_is_local_connector_desktop;
use crate::core::validation::normalize_non_empty;
use crate::models::memory_mapping_types::{MemoryContactDto, SyncProjectAgentLinkRequestDto};
use crate::modules::conversation_runtime::messages as conversation_messages;
use crate::services::realtime::publish_project_members_updated;
use crate::services::{chatos_memory_mappings, chatos_sessions};

use super::contracts::{AddProjectContactRequest, LocalProjectRequestQuery, ProjectContactsQuery};
use super::session_resolver::resolve_project_contact_session_id;

fn value_has_items(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::String(raw)) => !raw.trim().is_empty(),
        _ => false,
    }
}

fn task_status(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
}

fn task_status_is_active(value: Option<&Value>) -> bool {
    task_status(value).is_some_and(|status| {
        matches!(
            status.as_str(),
            "pending" | "queued" | "running" | "processing" | "in_progress"
        )
    })
}

fn task_status_is_terminal(value: Option<&Value>) -> bool {
    task_status(value).is_some_and(|status| {
        matches!(
            status.as_str(),
            "completed" | "succeeded" | "failed" | "blocked" | "cancelled" | "canceled"
        )
    })
}

fn metadata_has_running_task_marker(metadata: Option<&Value>) -> bool {
    let Some(task_runner_async) = metadata.and_then(|value| value.get("task_runner_async")) else {
        return false;
    };

    if task_status_is_active(task_runner_async.get("overall_status"))
        || task_status_is_active(task_runner_async.get("status"))
    {
        return true;
    }

    if task_status_is_terminal(task_runner_async.get("overall_status"))
        || task_status_is_terminal(task_runner_async.get("status"))
    {
        return false;
    }

    if value_has_items(task_runner_async.get("running_task_ids"))
        || value_has_items(task_runner_async.get("queued_task_ids"))
        || value_has_items(task_runner_async.get("pending_task_ids"))
    {
        return true;
    }

    false
}

fn turn_slice_has_running_task(slice: &memory_engine_sdk::TurnRecordSlice) -> bool {
    metadata_has_running_task_marker(slice.user_record.metadata.as_ref())
        || slice
            .final_assistant_record
            .as_ref()
            .is_some_and(|record| metadata_has_running_task_marker(record.metadata.as_ref()))
}

async fn project_has_running_user_message_tasks(
    user_id: &str,
    project_id: &str,
) -> Result<bool, String> {
    let sessions =
        chatos_sessions::list_sessions(Some(user_id), Some(project_id), Some(500), 0, false, false)
            .await?;

    for session in sessions {
        let mut before_turn_id: Option<String> = None;
        for _ in 0..20 {
            let page = conversation_messages::list_compact_turns(
                session.id.as_str(),
                Some(100),
                before_turn_id.as_deref(),
            )
            .await?;
            if page.items.iter().any(turn_slice_has_running_task) {
                return Ok(true);
            }
            if !page.has_more {
                break;
            }
            let Some(next_before) = page.next_before else {
                break;
            };
            before_turn_id = Some(next_before);
        }
    }

    Ok(false)
}

fn project_contact_locked_response() -> (StatusCode, Json<Value>) {
    (
        StatusCode::CONFLICT,
        Json(serde_json::json!({
            "error": "project contact is locked while user-message tasks are running",
            "locked": true,
        })),
    )
}

fn local_project_contact_fallback_allowed(headers: &HeaderMap, requested: bool) -> bool {
    requested && request_is_local_connector_desktop(headers)
}

async fn project_contact_owner_user_id(
    project_id: &str,
    auth: &AuthUser,
    headers: &HeaderMap,
    local_runtime: bool,
) -> Result<String, (StatusCode, Json<Value>)> {
    match ensure_owned_project(project_id, auth).await {
        Ok(project) => Ok(project.user_id.unwrap_or_else(|| auth.user_id.clone())),
        Err(ProjectAccessError::NotFound)
            if local_project_contact_fallback_allowed(headers, local_runtime) =>
        {
            Ok(auth.user_id.clone())
        }
        Err(err) => Err(map_project_access_error(err)),
    }
}

pub(super) async fn get_project_contact_lock(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<LocalProjectRequestQuery>,
) -> (StatusCode, Json<Value>) {
    if let Err(response) =
        project_contact_owner_user_id(&id, &auth, &headers, query.local_runtime).await
    {
        return response;
    }

    match project_has_running_user_message_tasks(auth.user_id.as_str(), id.as_str()).await {
        Ok(locked) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "locked": locked,
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "check project contact lock failed",
                "detail": err,
            })),
        ),
    }
}

pub(super) async fn list_project_contacts(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<ProjectContactsQuery>,
) -> (StatusCode, Json<Value>) {
    let project_user_id =
        match project_contact_owner_user_id(&id, &auth, &headers, query.local_runtime).await {
            Ok(user_id) => user_id,
            Err(response) => return response,
        };

    match chatos_memory_mappings::list_project_contacts_for_owner(
        project_user_id.as_str(),
        id.as_str(),
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(mut items) => {
            for item in &mut items {
                if let Some((session_id, last_message_at)) = resolve_project_contact_session_id(
                    project_user_id.as_str(),
                    &id,
                    &item.contact_id,
                )
                .await
                {
                    item.latest_session_id = Some(session_id);
                    item.last_message_at = item.last_message_at.clone().or(last_message_at);
                } else {
                    item.latest_session_id = None;
                }
            }
            (
                StatusCode::OK,
                Json(serde_json::to_value(items).unwrap_or(Value::Null)),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "list project contacts failed", "detail": err})),
        ),
    }
}

pub(super) async fn add_project_contact(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<LocalProjectRequestQuery>,
    Json(req): Json<AddProjectContactRequest>,
) -> (StatusCode, Json<Value>) {
    let project_user_id =
        match project_contact_owner_user_id(&id, &auth, &headers, query.local_runtime).await {
            Ok(user_id) => user_id,
            Err(response) => return response,
        };

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

    let linked_rows = match chatos_memory_mappings::list_project_contacts_for_owner(
        project_user_id.as_str(),
        id.as_str(),
        Some(500),
        0,
    )
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
    let same_binding =
        !linked_rows.is_empty() && linked_rows.iter().all(|item| item.contact_id == contact_id);
    if !same_binding {
        match project_has_running_user_message_tasks(auth.user_id.as_str(), id.as_str()).await {
            Ok(false) => {}
            Ok(true) => return project_contact_locked_response(),
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": "check project contact lock failed",
                        "detail": err,
                    })),
                );
            }
        }
    }

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
    headers: HeaderMap,
    Path((id, contact_id)): Path<(String, String)>,
    Query(query): Query<LocalProjectRequestQuery>,
) -> (StatusCode, Json<Value>) {
    let project_user_id =
        match project_contact_owner_user_id(&id, &auth, &headers, query.local_runtime).await {
            Ok(user_id) => user_id,
            Err(response) => return response,
        };

    let linked_rows = match chatos_memory_mappings::list_project_contacts_for_owner(
        project_user_id.as_str(),
        id.as_str(),
        Some(500),
        0,
    )
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

    match project_has_running_user_message_tasks(auth.user_id.as_str(), id.as_str()).await {
        Ok(false) => {}
        Ok(true) => return project_contact_locked_response(),
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "check project contact lock failed",
                    "detail": err,
                })),
            );
        }
    }

    match chatos_memory_mappings::delete_project_contact_link(
        auth.user_id.as_str(),
        id.as_str(),
        linked.contact_id.as_str(),
    )
    .await
    {
        Ok(true) => {
            publish_project_members_updated(
                auth.user_id.as_str(),
                id.as_str(),
                "project_contact_removed",
                Some(contact_id.as_str()),
            );
            (StatusCode::OK, Json(serde_json::json!({"success": true})))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "project contact not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "remove project contact failed", "detail": err})),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use serde_json::json;

    #[test]
    fn local_project_contact_fallback_requires_desktop_and_explicit_local_route() {
        let mut headers = HeaderMap::new();
        assert!(!local_project_contact_fallback_allowed(&headers, true));
        headers.insert(
            "x-requested-with",
            HeaderValue::from_static("local-connector-desktop"),
        );
        assert!(!local_project_contact_fallback_allowed(&headers, false));
        assert!(local_project_contact_fallback_allowed(&headers, true));
    }

    #[test]
    fn running_marker_detects_active_statuses() {
        let metadata = json!({
            "task_runner_async": {
                "overall_status": "processing"
            }
        });

        assert!(metadata_has_running_task_marker(Some(&metadata)));
    }

    #[test]
    fn running_marker_uses_running_ids_without_terminal_status() {
        let metadata = json!({
            "task_runner_async": {
                "running_task_ids": ["task-1"]
            }
        });

        assert!(metadata_has_running_task_marker(Some(&metadata)));
    }

    #[test]
    fn running_marker_does_not_lock_when_status_is_terminal() {
        let metadata = json!({
            "task_runner_async": {
                "overall_status": "completed",
                "running_task_ids": ["stale-task"]
            }
        });

        assert!(!metadata_has_running_task_marker(Some(&metadata)));
    }
}
