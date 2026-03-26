use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::repositories::memories as memories_repo;

use super::super::SharedState;
use super::contracts::ListContactMemoriesQuery;
use super::support::{internal_error, resolve_contact};

pub(in crate::api) async fn list_contact_project_memories(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(contact_id): Path<String>,
    Query(q): Query<ListContactMemoriesQuery>,
) -> (StatusCode, Json<Value>) {
    let contact = match resolve_contact(&state, &headers, contact_id.as_str()).await {
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
        Err(err) => internal_error("list project memories failed", err),
    }
}

pub(in crate::api) async fn list_contact_project_memories_by_project(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((contact_id, project_id)): Path<(String, String)>,
    Query(q): Query<ListContactMemoriesQuery>,
) -> (StatusCode, Json<Value>) {
    let contact = match resolve_contact(&state, &headers, contact_id.as_str()).await {
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
        Err(err) => internal_error("list project memories failed", err),
    }
}

pub(in crate::api) async fn list_contact_agent_recalls(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path(contact_id): Path<String>,
    Query(q): Query<ListContactMemoriesQuery>,
) -> (StatusCode, Json<Value>) {
    let contact = match resolve_contact(&state, &headers, contact_id.as_str()).await {
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
        Err(err) => internal_error("list agent recalls failed", err),
    }
}
