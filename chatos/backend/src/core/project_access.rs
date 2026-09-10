// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::core::auth::AuthUser;
use crate::modules::conversation_runtime::session_scope::resolve_session_project_scope;
use crate::services::chatos_sessions;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

#[derive(Debug)]
pub enum ProjectAccessError {
    NotFound,
    Internal(String),
}

pub async fn resolve_owned_project_root(
    project_id: &str,
    auth: &AuthUser,
) -> Result<String, ProjectAccessError> {
    let project_id = project_id.trim();
    if project_id.is_empty() {
        return Err(ProjectAccessError::NotFound);
    }
    let sessions = chatos_sessions::list_sessions(
        Some(auth.user_id.as_str()),
        Some(project_id),
        Some(1),
        0,
        true,
        true,
    )
    .await
    .map_err(ProjectAccessError::Internal)?;
    let session = sessions
        .into_iter()
        .find(|session| {
            resolve_session_project_scope(session.project_id.as_deref(), session.metadata.as_ref())
                .as_deref()
                == Some(project_id)
        })
        .ok_or(ProjectAccessError::NotFound)?;
    let runtime = session
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("chat_runtime"));
    let root_path = runtime
        .and_then(|runtime| {
            runtime
                .get("project_root")
                .or_else(|| runtime.get("projectRoot"))
        })
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string();
    Ok(root_path)
}

pub fn map_project_access_error(err: ProjectAccessError) -> (StatusCode, Json<Value>) {
    match err {
        ProjectAccessError::NotFound => {
            (StatusCode::NOT_FOUND, Json(json!({"error": "项目不存在"})))
        }
        ProjectAccessError::Internal(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": err})),
        ),
    }
}
