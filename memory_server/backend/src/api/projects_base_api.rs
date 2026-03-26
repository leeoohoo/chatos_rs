use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::repositories::projects as projects_repo;

use super::{
    default_project_name, normalize_optional_text, normalize_project_scope_id, require_auth,
    resolve_scope_user_id, SharedState,
};

#[derive(Debug, Deserialize)]
pub(super) struct ListProjectsQuery {
    user_id: Option<String>,
    status: Option<String>,
    include_virtual: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SyncProjectRequest {
    user_id: Option<String>,
    project_id: Option<String>,
    name: Option<String>,
    root_path: Option<String>,
    description: Option<String>,
    status: Option<String>,
    is_virtual: Option<bool>,
}

pub(super) async fn list_projects(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(q): Query<ListProjectsQuery>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };
    let scope_user_id = resolve_scope_user_id(&auth, q.user_id);
    let limit = q.limit.unwrap_or(200);
    let offset = q.offset.unwrap_or(0);
    let include_virtual = q.include_virtual.unwrap_or(true);
    let status = normalize_optional_text(q.status.as_deref());

    match projects_repo::list_projects(
        &state.pool,
        scope_user_id.as_str(),
        status.as_deref(),
        include_virtual,
        limit,
        offset,
    )
    .await
    {
        Ok(items) => (StatusCode::OK, Json(json!({"items": items}))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "list projects failed", "detail": err})),
        ),
    }
}

pub(super) async fn sync_project(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<SyncProjectRequest>,
) -> (StatusCode, Json<Value>) {
    let auth = match require_auth(&state, &headers) {
        Ok(v) => v,
        Err(err) => return err,
    };

    let scope_user_id = resolve_scope_user_id(&auth, req.user_id);
    let project_id = normalize_project_scope_id(req.project_id.as_deref());
    let is_virtual = req.is_virtual.map(|v| if v { 1 } else { 0 }).or_else(|| {
        if project_id == "0" {
            Some(1)
        } else {
            None
        }
    });
    let name = normalize_optional_text(req.name.as_deref())
        .unwrap_or_else(|| default_project_name(project_id.as_str()));

    match projects_repo::upsert_project(
        &state.pool,
        projects_repo::UpsertMemoryProjectInput {
            user_id: scope_user_id,
            project_id,
            name,
            root_path: normalize_optional_text(req.root_path.as_deref()),
            description: normalize_optional_text(req.description.as_deref()),
            status: normalize_optional_text(req.status.as_deref()),
            is_virtual,
        },
    )
    .await
    {
        Ok(Some(project)) => (StatusCode::OK, Json(json!(project))),
        Ok(None) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                json!({"error": "sync project failed", "detail": "project not found after upsert"}),
            ),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "sync project failed", "detail": err})),
        ),
    }
}
