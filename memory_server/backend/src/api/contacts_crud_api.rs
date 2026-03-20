use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::CreateContactRequest;
use crate::repositories::{contacts as contacts_repo, sessions};

use super::{
    ensure_agent_read_access, ensure_contact_access, require_auth, resolve_scope_user_id,
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

    let create_req = CreateContactRequest {
        user_id: scope_user_id,
        agent_id,
        agent_name_snapshot: req
            .agent_name_snapshot
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| Some(agent.name)),
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

    let contact = match ensure_contact_access(state.as_ref(), &auth, contact_id.as_str()).await {
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
