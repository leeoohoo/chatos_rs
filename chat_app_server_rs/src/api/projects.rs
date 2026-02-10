use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::get,
    Json, Router,
};
use pathdiff::diff_paths;
use serde::Deserialize;
use serde_json::Value;
use std::path::Path as FsPath;
use std::path::Path as StdPath;

use crate::models::project::{Project, ProjectService};
use crate::repositories::change_logs;

#[derive(Debug, Deserialize)]
struct ProjectQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateProjectRequest {
    name: Option<String>,
    root_path: Option<String>,
    description: Option<String>,
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateProjectRequest {
    name: Option<String>,
    root_path: Option<String>,
    description: Option<String>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/:id",
            get(get_project).put(update_project).delete(delete_project),
        )
        .route("/api/projects/:id/changes", get(list_project_changes))
}

#[derive(Debug, Deserialize)]
struct ProjectChangeQuery {
    path: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list_projects(Query(query): Query<ProjectQuery>) -> (StatusCode, Json<Value>) {
    match ProjectService::list(query.user_id).await {
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

async fn create_project(Json(req): Json<CreateProjectRequest>) -> (StatusCode, Json<Value>) {
    let name = req.name.unwrap_or_default();
    let root_path = req.root_path.unwrap_or_default();
    if name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "项目名称不能为空"})),
        );
    }
    if root_path.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "项目目录不能为空"})),
        );
    }
    let p = FsPath::new(root_path.trim());
    if !p.exists() || !p.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "项目目录不存在或不是目录"})),
        );
    }
    let project = Project::new(name, root_path, req.description, req.user_id);
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
    (
        StatusCode::CREATED,
        Json(serde_json::to_value(saved).unwrap_or(Value::Null)),
    )
}

async fn get_project(Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    match ProjectService::get_by_id(&id).await {
        Ok(Some(project)) => (
            StatusCode::OK,
            Json(serde_json::to_value(project).unwrap_or(Value::Null)),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "项目不存在"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

async fn update_project(
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectRequest>,
) -> (StatusCode, Json<Value>) {
    if let Some(ref root_path) = req.root_path {
        let p = FsPath::new(root_path.trim());
        if !p.exists() || !p.is_dir() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "项目目录不存在或不是目录"})),
            );
        }
    }
    if let Err(err) = ProjectService::update(
        &id,
        req.name.clone(),
        req.root_path.clone(),
        req.description.clone(),
    )
    .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        );
    }
    match ProjectService::get_by_id(&id).await {
        Ok(Some(project)) => (
            StatusCode::OK,
            Json(serde_json::to_value(project).unwrap_or(Value::Null)),
        ),
        Ok(None) => (StatusCode::OK, Json(Value::Null)),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

async fn delete_project(Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    match ProjectService::delete(&id).await {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "message": "项目已删除"})),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

async fn list_project_changes(
    Path(id): Path<String>,
    Query(query): Query<ProjectChangeQuery>,
) -> (StatusCode, Json<Value>) {
    let project = match ProjectService::get_by_id(&id).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "项目不存在"})),
            )
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": err})),
            )
        }
    };

    let paths = build_change_paths(&project, query.path);
    let limit = query.limit.or(Some(100));
    let offset = query.offset.unwrap_or(0);

    match change_logs::list_project_change_logs(&project.id, paths, limit, offset).await {
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

fn build_change_paths(project: &Project, raw: Option<String>) -> Option<Vec<String>> {
    let raw = raw.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })?;
    let mut out = vec![raw.clone()];
    let root = project.root_path.trim();
    if !root.is_empty() {
        let root_path = StdPath::new(root);
        let target = StdPath::new(raw.as_str());
        if target.is_absolute() {
            if target.starts_with(root_path) {
                if let Some(rel) = diff_paths(target, root_path) {
                    out.push(rel.to_string_lossy().to_string());
                }
            }
        } else {
            let abs = root_path.join(target);
            out.push(abs.to_string_lossy().to_string());
        }
    }
    out.sort();
    out.dedup();
    Some(out)
}
