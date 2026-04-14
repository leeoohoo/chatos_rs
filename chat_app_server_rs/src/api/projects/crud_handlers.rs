use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use serde_json::Value;

use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::core::user_scope::resolve_user_id;
use crate::core::validation::{
    normalize_non_empty, validate_existing_dir, validate_existing_dir_if_present,
};
use crate::models::project::{Project, ProjectService};

use super::contracts::{CreateProjectRequest, ProjectQuery, UpdateProjectRequest};
use super::memory_sync::{sync_active_project, sync_archived_project};

pub(super) async fn list_projects(
    auth: AuthUser,
    Query(query): Query<ProjectQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    match ProjectService::list(Some(user_id)).await {
        Ok(list) => (
            StatusCode::OK,
            Json(serde_json::to_value(list).unwrap_or(Value::Null)),
        ),
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
            )
        }
    };

    let project = Project::new(name, root_path, description, Some(user_id));
    if let Err(err) = ProjectService::create(project.clone()).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        );
    }
    let saved = ProjectService::get_by_id(&project.id)
        .await
        .ok()
        .flatten()
        .unwrap_or(project);
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

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(saved).unwrap_or(Value::Null)),
    )
}

pub(super) async fn get_project(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match ensure_owned_project(&id, &auth).await {
        Ok(project) => (
            StatusCode::OK,
            Json(serde_json::to_value(project).unwrap_or(Value::Null)),
        ),
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
        description,
    } = req;

    let root_path = match validate_existing_dir_if_present(root_path, "项目目录不存在或不是目录")
    {
        Ok(path) => path,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": err})),
            )
        }
    };

    if let Err(err) = ProjectService::update(&id, name, root_path, description).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        );
    }
    match ProjectService::get_by_id(&id).await {
        Ok(Some(project)) => {
            if let Err(err) = sync_active_project(&project).await {
                eprintln!(
                    "[PROJECTS] sync memory project failed after update: project_id={} err={}",
                    project.id, err
                );
            }
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
    match ProjectService::delete(&id).await {
        Ok(_) => {
            if let Err(err) = sync_archived_project(&project).await {
                eprintln!(
                    "[PROJECTS] sync memory project failed after delete: project_id={} err={}",
                    project.id, err
                );
            }
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
