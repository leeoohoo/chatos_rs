// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::models::project::Project;
use crate::models::session::Session;
use crate::modules::conversation_runtime::session_scope::resolve_session_project_scope;

pub const CHATOS_CLIENT_SURFACE_HEADER: &str = "x-chatos-client-surface";
pub const LOCAL_CONNECTOR_DESKTOP_SURFACE: &str = "local-connector-desktop";

pub fn project_is_visible_on_request(_project: &Project, _headers: &HeaderMap) -> bool {
    true
}

pub fn ensure_project_visible_on_request(
    project: &Project,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<Value>)> {
    if project_is_visible_on_request(project, headers) {
        return Ok(());
    }
    Err((
        StatusCode::NOT_FOUND,
        Json(json!({
            "code": "project_not_found",
            "error": "项目不存在",
        })),
    ))
}

pub async fn ensure_cloud_session_execution(
    session: &Session,
    requested_project_id: Option<&str>,
    auth: &AuthUser,
    _headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<Value>)> {
    let mut project_ids = BTreeSet::new();
    if let Some(session_project_id) =
        resolve_session_project_scope(session.project_id.as_deref(), session.metadata.as_ref())
    {
        project_ids.insert(session_project_id);
    }
    if let Some(project_id) =
        crate::modules::conversation_runtime::session_scope::normalize_project_scope(
            requested_project_id,
        )
    {
        project_ids.insert(project_id);
    }

    for project_id in project_ids {
        ensure_owned_project(project_id.as_str(), auth)
            .await
            .map_err(map_project_access_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::{
        project_is_visible_on_request, CHATOS_CLIENT_SURFACE_HEADER,
        LOCAL_CONNECTOR_DESKTOP_SURFACE,
    };
    use crate::models::project::Project;

    fn project() -> Project {
        Project::new(
            "Project".to_string(),
            "/workspace/project".to_string(),
            None,
            None,
            Some("user-1".to_string()),
        )
    }

    #[test]
    fn local_workspace_projects_are_cloud_business_records_visible_on_every_surface() {
        let local = project();
        let browser_headers = HeaderMap::new();
        assert!(project_is_visible_on_request(&local, &browser_headers));

        let mut desktop_headers = HeaderMap::new();
        desktop_headers.insert(
            CHATOS_CLIENT_SURFACE_HEADER,
            HeaderValue::from_static(LOCAL_CONNECTOR_DESKTOP_SURFACE),
        );
        assert!(project_is_visible_on_request(&local, &desktop_headers));
    }
}
