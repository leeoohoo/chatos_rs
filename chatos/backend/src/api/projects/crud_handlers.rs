// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::{
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::Value;
use tracing::warn;

use crate::api::local_connectors::{
    close_local_terminal_session, local_connector_display_path, parse_local_connector_root_path,
    reconcile_local_connector_project,
};
use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::core::project_execution::{
    ensure_project_visible_on_request, project_is_visible_on_request,
};
use crate::core::user_scope::resolve_user_id;
use crate::core::user_visible_path::display_path;
use crate::models::project::{Project, ProjectService};
use crate::models::terminal::TerminalService;
use crate::models::terminal_log::TerminalLogService;
use crate::services::chatos_memory_mappings;
use crate::services::realtime::publish_projects_updated;

use super::contracts::{ProjectQuery, UpdateProjectRequest};
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
    headers: HeaderMap,
    Query(query): Query<ProjectQuery>,
) -> (StatusCode, Json<Value>) {
    let user_id = match resolve_user_id(query.user_id, &auth) {
        Ok(user_id) => user_id,
        Err(err) => return err,
    };
    match ProjectService::list(Some(user_id)).await {
        Ok(list) => {
            let mut reconciled = Vec::with_capacity(list.len());
            for project in list
                .into_iter()
                .filter(|project| project_is_visible_on_request(project, &headers))
            {
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

pub(super) async fn get_project(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    match ensure_owned_project(&id, &auth).await {
        Ok(project) => {
            if let Err(err) = ensure_project_visible_on_request(&project, &headers) {
                return err;
            }
            let project = reconcile_local_connector_project(project).await;
            let project = attach_project_session_id(project).await;
            (StatusCode::OK, Json(project_value(project)))
        }
        Err(err) => map_project_access_error(err),
    }
}

pub(super) async fn update_project(
    auth: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectRequest>,
) -> (StatusCode, Json<Value>) {
    let existing = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    if let Err(err) = ensure_project_visible_on_request(&existing, &headers) {
        return err;
    }

    let UpdateProjectRequest {
        name,
        git_url,
        description,
    } = req;

    if let Err(err) = ProjectService::update(&id, name, None, git_url, description).await {
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
    headers: HeaderMap,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let project = match ensure_owned_project(&id, &auth).await {
        Ok(project) => project,
        Err(err) => return map_project_access_error(err),
    };
    if let Err(err) = ensure_project_visible_on_request(&project, &headers) {
        return err;
    }
    cleanup_project_run_terminals(&project, auth.user_id.as_str()).await;
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

async fn cleanup_project_run_terminals(project: &Project, fallback_user_id: &str) {
    let user_id = project.user_id.as_deref().unwrap_or(fallback_user_id);
    let terminals = match TerminalService::list_project_runs_by_project_id(
        Some(user_id.to_string()),
        project.id.as_str(),
    )
    .await
    {
        Ok(terminals) => terminals,
        Err(err) => {
            warn!(
                project_id = project.id.as_str(),
                error = err.as_str(),
                "list project-run terminal metadata before project deletion failed"
            );
            return;
        }
    };

    for terminal in terminals {
        if let Some(root_ref) = parse_local_connector_root_path(terminal.cwd.as_str()) {
            if let Err((status, Json(body))) = close_local_terminal_session(
                root_ref.device_id.as_str(),
                root_ref.workspace_id.as_str(),
                terminal.id.as_str(),
            )
            .await
            {
                warn!(
                    project_id = project.id.as_str(),
                    terminal_id = terminal.id.as_str(),
                    status = %status,
                    error = %body,
                    "close Local Connector project-run terminal before deletion failed"
                );
            }
        }
        let _ = TerminalLogService::delete_by_terminal(terminal.id.as_str()).await;
        if let Err(err) = TerminalService::delete(terminal.id.as_str()).await {
            warn!(
                project_id = project.id.as_str(),
                terminal_id = terminal.id.as_str(),
                error = err.as_str(),
                "delete project-run terminal metadata failed"
            );
        }
    }
}
