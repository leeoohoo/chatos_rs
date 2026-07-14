// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    extract::{Multipart, Path, Query},
    http::StatusCode,
    Json,
};
use serde_json::Value;
use tracing::warn;

use crate::api::local_connectors::{
    local_connector_display_path, parse_local_connector_root_path,
    reconcile_local_connector_project,
};
use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::core::user_scope::resolve_user_id;
use crate::core::user_visible_path::display_path;
use crate::core::validation::normalize_non_empty;
use crate::models::project::{Project, ProjectService};
use crate::services::chatos_memory_mappings;
use crate::services::realtime::publish_projects_updated;
use crate::services::terminal_manager::get_terminal_manager;

use super::super::fs::policy::{FsPathPolicy, FsPolicyError};
use super::contracts::{CreateProjectRequest, ProjectQuery, UpdateProjectRequest};
use super::memory_sync::{sync_active_project, sync_archived_project};
use super::session_resolver::resolve_project_contact_session_id;

fn fs_policy_error_tuple(err: FsPolicyError) -> (StatusCode, Json<Value>) {
    (
        err.status_code(),
        Json(serde_json::json!({ "error": err.message() })),
    )
}

async fn authorize_project_root(
    auth: &AuthUser,
    raw: &str,
    empty_message: &str,
    invalid_message: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    if raw.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": empty_message })),
        ));
    }
    let policy = FsPathPolicy::for_user(auth)
        .await
        .map_err(fs_policy_error_tuple)?;
    let authorized = policy
        .authorize_existing_dir(raw, invalid_message, invalid_message)
        .map_err(fs_policy_error_tuple)?;
    policy
        .require_write(&authorized)
        .map_err(fs_policy_error_tuple)?;
    Ok(authorized.path.to_string_lossy().to_string())
}

async fn authorize_optional_project_root(
    auth: &AuthUser,
    raw: Option<String>,
    invalid_message: &str,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    authorize_project_root(auth, raw.as_str(), invalid_message, invalid_message)
        .await
        .map(Some)
}

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

fn project_value(project: Project) -> Value {
    let is_local_connector = parse_local_connector_root_path(project.root_path.as_str()).is_some();
    let internal_root_path = project.root_path.clone();
    let display_root_path = local_connector_display_path(project.root_path.as_str())
        .unwrap_or_else(|| display_path(project.root_path.as_str()));
    let mut value = serde_json::to_value(project).unwrap_or(Value::Null);
    if let Value::Object(ref mut map) = value {
        let response_root_path = if is_local_connector {
            internal_root_path
        } else {
            display_root_path.clone()
        };
        map.insert(
            "root_path".to_string(),
            Value::String(response_root_path.clone()),
        );
        map.insert("rootPath".to_string(), Value::String(response_root_path));
        map.insert(
            "display_root_path".to_string(),
            Value::String(display_root_path),
        );
    }
    value
}

fn project_list_value(projects: Vec<Project>) -> Value {
    Value::Array(projects.into_iter().map(project_value).collect())
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
            let mut reconciled = Vec::with_capacity(list.len());
            for project in list {
                reconciled.push(reconcile_local_connector_project(project).await);
            }
            let list = attach_project_session_ids(reconciled).await;
            (StatusCode::OK, Json(project_list_value(list)))
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
    let root_path = match authorize_project_root(
        &auth,
        root_path.as_deref().unwrap_or(""),
        "项目目录不能为空",
        "项目目录不存在或不是目录",
    )
    .await
    {
        Ok(path) => path,
        Err(err) => return err,
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
        .unwrap_or(Project {
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
        Json(project_value(saved))
    })
}

pub(super) async fn create_cloud_project(
    auth: AuthUser,
    multipart: Multipart,
) -> (StatusCode, Json<Value>) {
    let input = match parse_cloud_project_multipart(multipart).await {
        Ok(input) => input,
        Err(err) => return err,
    };
    let Some(name) = normalize_non_empty(Some(input.name)) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "项目名称不能为空"})),
        );
    };
    if input
        .git_url
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && input
            .zip
            .as_ref()
            .is_some_and(|(_, bytes)| !bytes.is_empty())
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Git 地址和 ZIP 文件不能同时填写"})),
        );
    }

    let saved =
        match ProjectService::create_cloud(name, input.git_url, input.zip, input.description).await
        {
            Ok(project) => project,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": err})),
                );
            }
        };
    if let Err(err) = sync_active_project(&saved).await {
        warn!(
            project_id = saved.id.as_str(),
            error = err.as_str(),
            "sync memory project failed after cloud create"
        );
    }

    publish_projects_updated(
        auth.user_id.as_str(),
        "project_created",
        Some(saved.id.as_str()),
        Some(saved.clone()),
    );
    (StatusCode::CREATED, Json(project_value(saved)))
}

pub(super) async fn get_project(
    auth: AuthUser,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match ensure_owned_project(&id, &auth).await {
        Ok(project) => {
            let project = reconcile_local_connector_project(project).await;
            let project = attach_project_session_id(project).await;
            (StatusCode::OK, Json(project_value(project)))
        }
        Err(err) => map_project_access_error(err),
    }
}

struct CloudProjectCreateInput {
    name: String,
    git_url: Option<String>,
    zip: Option<(String, Vec<u8>)>,
    description: Option<String>,
}

async fn parse_cloud_project_multipart(
    mut multipart: Multipart,
) -> Result<CloudProjectCreateInput, (StatusCode, Json<Value>)> {
    let mut name = None;
    let mut git_url = None;
    let mut description = None;
    let mut zip = None;

    while let Some(field) = multipart.next_field().await.map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("invalid multipart form: {err}")})),
        )
    })? {
        let field_name = field.name().unwrap_or_default().to_string();
        match field_name.as_str() {
            "name" | "project_name" => {
                name = Some(read_multipart_text(field).await?);
            }
            "git_url" | "source_git_url" => {
                git_url = normalize_non_empty(Some(read_multipart_text(field).await?));
            }
            "description" => {
                description = normalize_non_empty(Some(read_multipart_text(field).await?));
            }
            "zip" | "archive" | "file" => {
                let filename = field
                    .file_name()
                    .map(str::to_string)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "project.zip".to_string());
                let bytes = field.bytes().await.map_err(|err| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(
                            serde_json::json!({"error": format!("read zip upload failed: {err}")}),
                        ),
                    )
                })?;
                if !bytes.is_empty() {
                    zip = Some((filename, bytes.to_vec()));
                }
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }

    Ok(CloudProjectCreateInput {
        name: name.unwrap_or_default(),
        git_url,
        zip,
        description,
    })
}

async fn read_multipart_text(
    field: axum::extract::multipart::Field<'_>,
) -> Result<String, (StatusCode, Json<Value>)> {
    field.text().await.map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("read multipart text field failed: {err}")})),
        )
    })
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

    let root_path =
        match authorize_optional_project_root(&auth, root_path, "项目目录不存在或不是目录").await
        {
            Ok(path) => path,
            Err(err) => return err,
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
            (StatusCode::OK, Json(project_value(project)))
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
