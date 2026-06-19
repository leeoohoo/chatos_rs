use axum::http::StatusCode;
use axum::{
    Json, Router,
    extract::{Path, Query},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::core::auth::AuthUser;
use crate::core::user_scope::resolve_user_id;
use crate::models::memory_mapping_types::{
    SyncMemoryProjectRequestDto, SyncProjectAgentLinkRequestDto,
};
use crate::repositories::projects::get_project_by_id;
use crate::services::chatos_memory_mappings;

#[derive(Debug, Deserialize)]
struct ListMemoryProjectsQuery {
    user_id: Option<String>,
    status: Option<String>,
    include_virtual: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ListProjectContactsQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/memory/projects", get(list_memory_projects))
        .route("/api/memory/projects/sync", post(sync_memory_project))
        .route(
            "/api/memory/projects/:project_id/contacts",
            get(list_memory_project_contacts),
        )
        .route(
            "/api/memory/project-agent-links/sync",
            post(sync_project_agent_link),
        )
}

async fn list_memory_projects(
    auth: AuthUser,
    Query(query): Query<ListMemoryProjectsQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };

    match chatos_memory_mappings::list_memory_projects(
        user_id.as_str(),
        query.status.as_deref(),
        query.include_virtual,
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list memory projects failed", "detail": err})),
        ),
    }
}

async fn sync_memory_project(
    auth: AuthUser,
    Json(mut req): Json<SyncMemoryProjectRequestDto>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id.clone(), &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };
    req.user_id = Some(user_id);

    match chatos_memory_mappings::sync_memory_project(&req).await {
        Ok(project) => (StatusCode::OK, Json(json!(project))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync memory project failed", "detail": err})),
        ),
    }
}

async fn list_memory_project_contacts(
    auth: AuthUser,
    Path(project_id): Path<String>,
    Query(query): Query<ListProjectContactsQuery>,
) -> (StatusCode, Json<Value>) {
    let owner_user_id = match load_project_owner_user_id(project_id.as_str()).await {
        Ok(value) => value,
        Err(err) => return err,
    };
    if owner_user_id != auth.user_id {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"})));
    }

    match chatos_memory_mappings::list_project_contacts(
        project_id.as_str(),
        query.limit,
        query.offset.unwrap_or(0),
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!(items))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list project contacts failed", "detail": err})),
        ),
    }
}

async fn sync_project_agent_link(
    auth: AuthUser,
    Json(mut req): Json<SyncProjectAgentLinkRequestDto>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(req.user_id.clone(), &auth) {
        Ok(value) => value,
        Err(err) => return err,
    };
    req.user_id = Some(user_id);

    match chatos_memory_mappings::sync_project_agent_link(&req).await {
        Ok(link) => (StatusCode::OK, Json(json!(link))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync project-agent link failed", "detail": err})),
        ),
    }
}

async fn load_project_owner_user_id(project_id: &str) -> Result<String, (StatusCode, Json<Value>)> {
    match get_project_by_id(project_id).await {
        Ok(Some(project)) => project
            .user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "project owner is missing"})),
                )
            }),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "project not found"})),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load project failed", "detail": err})),
        )),
    }
}
