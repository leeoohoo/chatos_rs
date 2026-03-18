use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path as StdPath;

use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::core::user_scope::resolve_user_id;
use crate::core::validation::{
    normalize_non_empty, validate_existing_dir, validate_existing_dir_if_present,
};
use crate::models::project::{Project, ProjectService};
use crate::repositories::change_logs;
use crate::services::memory_server_client;

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
        .route(
            "/api/projects/:id/changes/summary",
            get(get_project_change_summary),
        )
        .route(
            "/api/projects/:id/changes/confirm",
            post(confirm_project_changes),
        )
}

#[derive(Debug, Deserialize)]
struct ProjectChangeQuery {
    path: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ConfirmProjectChangesRequest {
    mode: Option<String>,
    paths: Option<Vec<String>>,
    change_ids: Option<Vec<String>>,
}

async fn list_projects(
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

async fn create_project(
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
    if let Err(err) = memory_server_client::sync_memory_project(
        &memory_server_client::SyncMemoryProjectRequestDto {
            user_id: saved.user_id.clone(),
            project_id: Some(saved.id.clone()),
            name: Some(saved.name.clone()),
            root_path: Some(saved.root_path.clone()),
            description: saved.description.clone(),
            status: Some("active".to_string()),
            is_virtual: Some(false),
        },
    )
    .await
    {
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

async fn get_project(auth: AuthUser, Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    match ensure_owned_project(&id, &auth).await {
        Ok(project) => (
            StatusCode::OK,
            Json(serde_json::to_value(project).unwrap_or(Value::Null)),
        ),
        Err(err) => map_project_access_error(err),
    }
}

async fn update_project(
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
            if let Err(err) = memory_server_client::sync_memory_project(
                &memory_server_client::SyncMemoryProjectRequestDto {
                    user_id: project.user_id.clone(),
                    project_id: Some(project.id.clone()),
                    name: Some(project.name.clone()),
                    root_path: Some(project.root_path.clone()),
                    description: project.description.clone(),
                    status: Some("active".to_string()),
                    is_virtual: Some(false),
                },
            )
            .await
            {
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

async fn delete_project(auth: AuthUser, Path(id): Path<String>) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    match ProjectService::delete(&id).await {
        Ok(_) => {
            if let Err(err) = memory_server_client::sync_memory_project(
                &memory_server_client::SyncMemoryProjectRequestDto {
                    user_id: project.user_id.clone(),
                    project_id: Some(project.id.clone()),
                    name: Some(project.name.clone()),
                    root_path: Some(project.root_path.clone()),
                    description: project.description.clone(),
                    status: Some("archived".to_string()),
                    is_virtual: Some(false),
                },
            )
            .await
            {
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

async fn list_project_changes(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(query): Query<ProjectChangeQuery>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
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

async fn get_project_change_summary(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };

    match change_logs::list_unconfirmed_project_changes(&project.id, &project.root_path).await {
        Ok(records) => {
            let summary = change_logs::summarize_project_changes(&records);
            (
                StatusCode::OK,
                Json(serde_json::to_value(summary).unwrap_or(Value::Null)),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": err})),
        ),
    }
}

async fn confirm_project_changes(
    auth: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<ConfirmProjectChangesRequest>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };

    let records = match change_logs::list_unconfirmed_project_changes(
        &project.id,
        &project.root_path,
    )
    .await
    {
        Ok(records) => records,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": err})),
            )
        }
    };

    let mode = req
        .mode
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            if req
                .change_ids
                .as_ref()
                .map(|items| !items.is_empty())
                .unwrap_or(false)
            {
                Some("change_ids".to_string())
            } else if req
                .paths
                .as_ref()
                .map(|items| !items.is_empty())
                .unwrap_or(false)
            {
                Some("paths".to_string())
            } else {
                Some("all".to_string())
            }
        })
        .unwrap_or_else(|| "all".to_string());

    let candidate_ids: Vec<String> = match mode.as_str() {
        "all" => records.iter().map(|item| item.id.clone()).collect(),
        "paths" => {
            let paths = req.paths.unwrap_or_default();
            if paths.is_empty() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "paths 不能为空"})),
                );
            }
            collect_change_ids_for_paths(&records, &project.root_path, &paths)
        }
        "change_ids" => {
            let requested = req.change_ids.unwrap_or_default();
            if requested.is_empty() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "change_ids 不能为空"})),
                );
            }
            let allowed_ids: HashSet<String> = records.iter().map(|item| item.id.clone()).collect();
            requested
                .into_iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty() && allowed_ids.contains(value))
                .collect()
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "mode 仅支持 all / paths / change_ids"})),
            )
        }
    };

    match change_logs::confirm_change_logs_by_ids(&candidate_ids, Some(auth.user_id.as_str())).await
    {
        Ok(confirmed) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "confirmed": confirmed,
                "requested": candidate_ids.len(),
                "mode": mode,
            })),
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

fn collect_change_ids_for_paths(
    records: &[change_logs::ProjectScopedChangeRecord],
    project_root: &str,
    paths: &[String],
) -> Vec<String> {
    let targets = normalize_confirm_targets(project_root, paths);
    if targets.is_empty() {
        return Vec::new();
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for record in records {
        let record_path = normalize_path_text(&record.path);
        if record_path.is_empty() {
            continue;
        }
        if targets
            .iter()
            .any(|target| path_eq_or_descendant(&record_path, target))
            && seen.insert(record.id.clone())
        {
            out.push(record.id.clone());
        }
    }
    out
}

fn normalize_confirm_targets(project_root: &str, paths: &[String]) -> Vec<String> {
    let root = normalize_path_text(project_root);
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();

    for raw in paths {
        let normalized = normalize_path_text(raw);
        if normalized.is_empty() {
            continue;
        }
        let absolute = if is_absolute_path_like(&normalized) || root.is_empty() {
            normalized
        } else {
            join_paths(&root, &normalized)
        };
        let absolute = normalize_path_text(&absolute);
        if absolute.is_empty() {
            continue;
        }
        if seen.insert(absolute.clone()) {
            out.push(absolute);
        }
    }
    out
}

fn path_eq_or_descendant(path: &str, prefix: &str) -> bool {
    let path = normalize_path_text(path);
    let prefix = normalize_path_text(prefix);
    if path.is_empty() || prefix.is_empty() {
        return false;
    }
    if cfg!(windows) {
        let path_lower = path.to_ascii_lowercase();
        let prefix_lower = prefix.to_ascii_lowercase();
        path_lower == prefix_lower || path_lower.starts_with(&format!("{prefix_lower}/"))
    } else {
        path == prefix || path.starts_with(&format!("{prefix}/"))
    }
}

fn is_absolute_path_like(path: &str) -> bool {
    if StdPath::new(path).is_absolute() {
        return true;
    }
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
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
