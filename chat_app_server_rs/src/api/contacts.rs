// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, patch},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::models::memory_mapping_types::{
    CreateMemoryContactRequestDto, UpdateContactTaskRunnerConfigRequestDto,
};
use crate::services::chatos_memory_mappings;
use crate::services::realtime::publish_contacts_updated;

#[derive(Debug, Deserialize)]
struct ListContactsQuery {
    user_id: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreateContactRequest {
    user_id: Option<String>,
    agent_id: Option<String>,
    agent_name_snapshot: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContactMemoryQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/contacts", get(list_contacts).post(create_contact))
        .route(
            "/api/contacts/{contact_id}",
            get(get_contact).delete(delete_contact),
        )
        .route(
            "/api/contacts/{contact_id}/task-runner",
            patch(update_contact_task_runner),
        )
        .route(
            "/api/contacts/{contact_id}/project-memories",
            get(list_contact_project_memories_by_contact),
        )
        .route(
            "/api/contacts/{contact_id}/projects",
            get(list_contact_projects),
        )
        .route(
            "/api/contacts/{contact_id}/project-memories/{project_id}",
            get(list_contact_project_memories),
        )
        .route(
            "/api/contacts/{contact_id}/agent-recalls",
            get(list_contact_agent_recalls),
        )
}

async fn list_contacts(
    auth: AuthUser,
    Query(query): Query<ListContactsQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };

    match chatos_memory_mappings::list_memory_contacts(
        Some(user_id.as_str()),
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list contacts failed", "detail": err})),
        ),
    }
}

async fn get_contact(auth: AuthUser, Path(contact_id): Path<String>) -> (StatusCode, Json<Value>) {
    match chatos_memory_mappings::get_memory_contact(contact_id.as_str()).await {
        Ok(Some(contact)) => {
            if contact.user_id != auth.user_id {
                return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
            }
            (StatusCode::OK, Json(json!(contact)))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "contact not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get contact failed", "detail": err})),
        ),
    }
}

async fn create_contact(
    auth: AuthUser,
    Json(req): Json<CreateContactRequest>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id, &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };
    let agent_id = req.agent_id.unwrap_or_default().trim().to_string();
    if agent_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "agent_id 为必填项"})),
        );
    }

    let payload = CreateMemoryContactRequestDto {
        user_id: Some(user_id),
        agent_id,
        agent_name_snapshot: req
            .agent_name_snapshot
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    };

    match chatos_memory_mappings::create_memory_contact(&payload).await {
        Ok(result) => {
            publish_contacts_updated(
                auth.user_id.as_str(),
                if result.created {
                    "contact_created"
                } else {
                    "contact_upserted"
                },
                Some(result.contact.id.as_str()),
                Some(result.contact.clone()),
            );
            let status = if result.created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            (status, Json(json!(result)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "create contact failed", "detail": err})),
        ),
    }
}

async fn delete_contact(
    auth: AuthUser,
    Path(contact_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let user_id = auth.user_id.clone();
    match chatos_memory_mappings::delete_memory_contact(contact_id.as_str()).await {
        Ok(true) => {
            publish_contacts_updated(
                user_id.as_str(),
                "contact_deleted",
                Some(contact_id.as_str()),
                None,
            );
            (StatusCode::OK, Json(json!({"success": true})))
        }
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

async fn update_contact_task_runner(
    auth: AuthUser,
    Path(contact_id): Path<String>,
    Json(req): Json<UpdateContactTaskRunnerConfigRequestDto>,
) -> (StatusCode, Json<Value>) {
    match chatos_memory_mappings::get_memory_contact(contact_id.as_str()).await {
        Ok(Some(contact)) => {
            if contact.user_id != auth.user_id {
                return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
            }
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "contact not found"})),
            );
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "get contact failed", "detail": err})),
            );
        }
    }

    match chatos_memory_mappings::update_contact_task_runner_config(contact_id.as_str(), &req).await
    {
        Ok(Some(contact)) => {
            publish_contacts_updated(
                auth.user_id.as_str(),
                "contact_updated",
                Some(contact.id.as_str()),
                Some(contact.clone()),
            );
            (StatusCode::OK, Json(json!(contact)))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "contact not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "update contact task runner config failed", "detail": err})),
        ),
    }
}

async fn list_contact_project_memories(
    _auth: AuthUser,
    Path((contact_id, project_id)): Path<(String, String)>,
    Query(query): Query<ContactMemoryQuery>,
) -> (StatusCode, Json<Value>) {
    match chatos_memory_mappings::list_contact_project_memories(
        contact_id.as_str(),
        project_id.as_str(),
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list contact project memories failed", "detail": err})),
        ),
    }
}

async fn list_contact_project_memories_by_contact(
    _auth: AuthUser,
    Path(contact_id): Path<String>,
    Query(query): Query<ContactMemoryQuery>,
) -> (StatusCode, Json<Value>) {
    match chatos_memory_mappings::list_contact_project_memories_by_contact(
        contact_id.as_str(),
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list contact project memories failed", "detail": err})),
        ),
    }
}

async fn list_contact_projects(
    _auth: AuthUser,
    Path(contact_id): Path<String>,
    Query(query): Query<ContactMemoryQuery>,
) -> (StatusCode, Json<Value>) {
    match chatos_memory_mappings::list_contact_projects(
        contact_id.as_str(),
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list contact projects failed", "detail": err})),
        ),
    }
}

async fn list_contact_agent_recalls(
    _auth: AuthUser,
    Path(contact_id): Path<String>,
    Query(query): Query<ContactMemoryQuery>,
) -> (StatusCode, Json<Value>) {
    match chatos_memory_mappings::list_contact_agent_recalls(
        contact_id.as_str(),
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list contact agent recalls failed", "detail": err})),
        ),
    }
}
