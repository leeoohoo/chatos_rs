use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::info;

use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::services::memory_server_client;

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
struct UpdateContactBuiltinMcpGrantsRequest {
    authorized_builtin_mcp_ids: Vec<String>,
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
            "/api/contacts/:contact_id",
            axum::routing::delete(delete_contact),
        )
        .route(
            "/api/contacts/:contact_id/builtin-mcp-grants",
            get(get_contact_builtin_mcp_grants).patch(update_contact_builtin_mcp_grants),
        )
        .route(
            "/api/contacts/:contact_id/project-memories",
            get(list_contact_project_memories_by_contact),
        )
        .route(
            "/api/contacts/:contact_id/projects",
            get(list_contact_projects),
        )
        .route(
            "/api/contacts/:contact_id/project-memories/:project_id",
            get(list_contact_project_memories),
        )
        .route(
            "/api/contacts/:contact_id/agent-recalls",
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

    match memory_server_client::list_memory_contacts(
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

    let payload = memory_server_client::CreateMemoryContactRequestDto {
        user_id: Some(user_id),
        agent_id,
        agent_name_snapshot: req
            .agent_name_snapshot
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        authorized_builtin_mcp_ids: Vec::new(),
    };

    match memory_server_client::create_memory_contact(&payload).await {
        Ok(result) => {
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

async fn get_contact_builtin_mcp_grants(
    _auth: AuthUser,
    Path(contact_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match memory_server_client::get_contact_builtin_mcp_grants(contact_id.as_str()).await {
        Ok(Some(result)) => {
            info!(
                "contact builtin MCP grants loaded: contact_id={} grants={}",
                result.contact_id,
                result.authorized_builtin_mcp_ids.join(", ")
            );
            (StatusCode::OK, Json(json!(result)))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "contact not found"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "get contact builtin mcp grants failed", "detail": err})),
        ),
    }
}

async fn update_contact_builtin_mcp_grants(
    _auth: AuthUser,
    Path(contact_id): Path<String>,
    Json(req): Json<UpdateContactBuiltinMcpGrantsRequest>,
) -> (StatusCode, Json<Value>) {
    match memory_server_client::update_contact_builtin_mcp_grants(
        contact_id.as_str(),
        &memory_server_client::UpdateContactBuiltinMcpGrantsRequestDto {
            authorized_builtin_mcp_ids: req.authorized_builtin_mcp_ids,
        },
    )
    .await
    {
        Ok(result) => {
            info!(
                "contact builtin MCP grants updated: contact_id={} grants={}",
                result.contact_id,
                result.authorized_builtin_mcp_ids.join(", ")
            );
            (StatusCode::OK, Json(json!(result)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "update contact builtin mcp grants failed", "detail": err})),
        ),
    }
}

async fn delete_contact(
    auth: AuthUser,
    Path(contact_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let _ = auth;
    match memory_server_client::delete_memory_contact(contact_id.as_str()).await {
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

async fn list_contact_project_memories(
    _auth: AuthUser,
    Path((contact_id, project_id)): Path<(String, String)>,
    Query(query): Query<ContactMemoryQuery>,
) -> (StatusCode, Json<Value>) {
    match memory_server_client::list_contact_project_memories(
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
    match memory_server_client::list_contact_project_memories_by_contact(
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
    match memory_server_client::list_contact_projects(
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
    match memory_server_client::list_contact_agent_recalls(
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
