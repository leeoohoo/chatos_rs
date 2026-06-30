use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde_json::Value;
use tracing::warn;

use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::core::user_scope::resolve_user_id;
use crate::core::validation::{
    normalize_non_empty, validate_existing_dir, validate_existing_dir_if_present,
};
use crate::models::project::{Project, ProjectService};
use crate::services::chatos_memory_mappings;
use crate::services::realtime::publish_projects_updated;
use crate::services::terminal_manager::get_terminal_manager;

use super::contracts::{CreateProjectRequest, ProjectQuery, UpdateProjectRequest};
use super::memory_sync::{sync_active_project, sync_archived_project};
use super::session_resolver::resolve_project_contact_session_id;

async fn attach_project_session_id(mut project: Project) -> Project {
    let project_id = project.id.clone();
    if let Ok(rows) =
        chatos_memory_mappings::list_project_contacts(project_id.as_str(), Some(500), 0).await
    {
        let Some(user_id) = project.user_id.as_deref() else {
            return project;
        };
        if let Some(row) = rows.into_iter().next() {
            if let Some((session_id, last_message_at)) =
                resolve_project_contact_session_id(user_id, &project.id, &row.contact_id).await
            {
                project.latest_session_id = Some(session_id);
                project.last_message_at = row.last_message_at.or(last_message_at);
            }
        }
    }
    project
}

async fn attach_project_session_ids(projects: Vec<Project>) -> Vec<Project> {
    let mut out = Vec::with_capacity(projects.len());
    for project in projects {
        out.push(attach_project_session_id(project).await);
    }
    out
}

pub(super) async fn list_projects(
    auth: AuthUser,
    Query(query): Query<ProjectQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    match ProjectService::list(Some(user_id)).await {
        Ok(list) => {
            let list = attach_project_session_ids(list).await;
            (
                StatusCode::OK,
                Json(serde_json::to_value(list).unwrap_or(Value::Null)),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

pub(super) async fn create_project(
    auth: AuthUser,
    Json(req): Json<CreateProjectRequest>,
) -> (StatusCode, Json<Value>) {
    let CreateProjectRequest {
        name,
        root_path,
        git_url,
        description,
        user_id,
    } = req;
    let user_id = match resolve_user_id(user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };

    let Some(name) = normalize_non_empty(name) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "项目名称不能为空"})),
        );
    };
    let root_path = match validate_existing_dir(
        root_path.as_deref().unwrap_or(""),
        "项目目录不能为空",
        "项目目录不存在或不是目录",
    ) {
        Ok(path) => path,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": err})),
            );
        }
    };

    let project = Project::new(name, root_path, git_url, description, Some(user_id));
    let saved_id = match ProjectService::create(project.clone()).await {
        Ok(id) => id,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": err})),
            );
        }
    };
    let saved = ProjectService::get_by_id(&saved_id)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| Project {
            id: saved_id,
            ..project
        });
    if let Err(err) = sync_active_project(&saved).await {
        let _ = ProjectService::delete(saved.id.as_str()).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "sync memory project failed",
                "detail": err,
            })),
        );
    }

    (StatusCode::CREATED, {
        publish_projects_updated(
            auth.user_id.as_str(),
            "project_created",
            Some(saved.id.as_str()),
            Some(saved.clone()),
        );
        Json(serde_json::to_value(saved).unwrap_or(Value::Null))
    })
}

pub(super) async fn get_project(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match ensure_owned_project(&id, &auth).await {
        Ok(project) => {
            let project = attach_project_session_id(project).await;
            (
                StatusCode::OK,
                Json(serde_json::to_value(project).unwrap_or(Value::Null)),
            )
        }
        Err(err) => map_project_access_error(err),
    }
}

pub(super) async fn update_project(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectRequest>,
) -> (StatusCode, Json<Value>) {
    if let Err(err) = ensure_owned_project(&id, &auth).await {
        return map_project_access_error(err);
    }

    let UpdateProjectRequest {
        name,
        root_path,
        git_url,
        description,
    } = req;

    let root_path = match validate_existing_dir_if_present(root_path, "项目目录不存在或不是目录")
    {
        Ok(path) => path,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": err})),
            );
        }
    };

    if let Err(err) = ProjectService::update(&id, name, root_path, git_url, description).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        );
    }
    match ProjectService::get_by_id(&id).await {
        Ok(Some(project)) => {
            if let Err(err) = sync_active_project(&project).await {
                warn!(
                    project_id = project.id.as_str(),
                    error = err.as_str(),
                    "sync memory project failed after update"
                );
            }
            publish_projects_updated(
                auth.user_id.as_str(),
                "project_updated",
                Some(project.id.as_str()),
                Some(project.clone()),
            );
            (
                StatusCode::OK,
                Json(serde_json::to_value(project).unwrap_or(Value::Null)),
            )
        }
        Ok(None) => (StatusCode::OK, Json(Value::Null)),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

pub(super) async fn delete_project(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    let manager = get_terminal_manager();
    let _ = manager
        .close_project_run_terminals(
            project.user_id.as_deref().or(Some(auth.user_id.as_str())),
            project.id.as_str(),
        )
        .await;
    match ProjectService::delete(&id).await {
        Ok(_) => {
            if let Err(err) = sync_archived_project(&project).await {
                warn!(
                    project_id = project.id.as_str(),
                    error = err.as_str(),
                    "sync memory project failed after delete"
                );
            }
            publish_projects_updated(
                auth.user_id.as_str(),
                "project_deleted",
                Some(project.id.as_str()),
                None,
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({"success": true, "message": "项目已删除"})),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}
