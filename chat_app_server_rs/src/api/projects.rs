use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path as StdPath;

use crate::core::validation::{
    normalize_non_empty, validate_existing_dir, validate_existing_dir_if_present,
};
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
    let CreateProjectRequest {
        name,
        root_path,
        description,
        user_id,
    } = req;

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

    let project = Project::new(name, root_path, description, user_id);
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

    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    add_path_variants(&mut out, &mut seen, &raw);

    let root_raw = project.root_path.trim();
    let root_norm = normalize_path_text(root_raw);
    let raw_norm = normalize_path_text(&raw);
    let raw_is_absolute = StdPath::new(raw.as_str()).is_absolute();

    if !root_norm.is_empty() {
        if raw_is_absolute {
            if let Some(rel) = strip_path_prefix(&raw_norm, &root_norm) {
                let rel = rel.trim_matches('/').to_string();
                if !rel.is_empty() {
                    add_path_variants(&mut out, &mut seen, &rel);

                    if let Some(project_dir) = project_dir_name(root_raw) {
                        let prefixed = format!("{project_dir}/{rel}");
                        add_path_variants(&mut out, &mut seen, &prefixed);
                    }
                }
            }
        } else {
            let abs = join_paths(&root_norm, &raw_norm);
            add_path_variants(&mut out, &mut seen, &abs);
        }

        if let Some(project_dir) = project_dir_name(root_raw) {
            if let Some(stripped) = strip_path_prefix(&raw_norm, &project_dir) {
                let stripped = stripped.trim_matches('/').to_string();
                if !stripped.is_empty() {
                    add_path_variants(&mut out, &mut seen, &stripped);
                }
            } else {
                let prefixed = join_paths(&project_dir, &raw_norm);
                add_path_variants(&mut out, &mut seen, &prefixed);
            }
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn add_path_variants(out: &mut Vec<String>, seen: &mut HashSet<String>, raw: &str) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }

    push_candidate(out, seen, trimmed.to_string());

    let normalized = normalize_path_text(trimmed);
    if normalized.is_empty() {
        return;
    }

    push_candidate(out, seen, normalized.clone());

    let without_dot = normalized
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_string();
    if !without_dot.is_empty() {
        push_candidate(out, seen, without_dot.clone());
        push_candidate(out, seen, without_dot.replace('/', "\\"));
    }

    push_candidate(out, seen, normalized.replace('/', "\\"));
}

fn push_candidate(out: &mut Vec<String>, seen: &mut HashSet<String>, value: String) {
    let candidate = value.trim();
    if candidate.is_empty() {
        return;
    }
    if seen.insert(candidate.to_string()) {
        out.push(candidate.to_string());
    }
}

fn normalize_path_text(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.contains("//") {
        normalized = normalized.replace("//", "/");
    }
    if normalized.len() > 1 {
        normalized = normalized.trim_end_matches('/').to_string();
    }
    normalized
}

fn join_paths(base: &str, tail: &str) -> String {
    let base = base.trim_end_matches('/');
    let tail = tail.trim_start_matches('/');
    if base.is_empty() {
        return tail.to_string();
    }
    if tail.is_empty() {
        return base.to_string();
    }
    format!("{base}/{tail}")
}

fn project_dir_name(root: &str) -> Option<String> {
    let normalized = normalize_path_text(root);
    normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .last()
        .map(|part| part.to_string())
}

fn strip_path_prefix(value: &str, prefix: &str) -> Option<String> {
    let value_parts: Vec<&str> = value.split('/').filter(|part| !part.is_empty()).collect();
    let prefix_parts: Vec<&str> = prefix.split('/').filter(|part| !part.is_empty()).collect();

    if prefix_parts.len() > value_parts.len() {
        return None;
    }

    let matched = value_parts
        .iter()
        .zip(prefix_parts.iter())
        .all(|(value_part, prefix_part)| path_part_eq(value_part, prefix_part));

    if !matched {
        return None;
    }

    Some(value_parts[prefix_parts.len()..].join("/"))
}

fn path_part_eq(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}
