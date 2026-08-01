// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::core::project_access::{ensure_owned_project, map_project_access_error};
use crate::models::project::{Project, PUBLIC_PROJECT_ID};
use crate::models::session::Session;
use crate::modules::conversation_runtime::session_scope::resolve_session_project_scope;

pub const CHATOS_CLIENT_SURFACE_HEADER: &str = "x-chatos-client-surface";
pub const CHATOS_CLIENT_SURFACE_COMPAT_HEADER: &str = "x-requested-with";
pub const LOCAL_CONNECTOR_DESKTOP_SURFACE: &str = "local-connector-desktop";

pub fn request_is_local_connector_desktop(headers: &HeaderMap) -> bool {
    [
        CHATOS_CLIENT_SURFACE_HEADER,
        CHATOS_CLIENT_SURFACE_COMPAT_HEADER,
    ]
    .into_iter()
    .filter_map(|header| headers.get(header))
    .any(|value| {
        value
            .to_str()
            .ok()
            .map(str::trim)
            .is_some_and(|value| value.eq_ignore_ascii_case(LOCAL_CONNECTOR_DESKTOP_SURFACE))
    })
}

pub fn require_local_connector_desktop(
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<Value>)> {
    if request_is_local_connector_desktop(headers) {
        return Ok(());
    }
    Err((
        StatusCode::FORBIDDEN,
        Json(json!({
            "code": "desktop_client_required",
            "error": "Local Connector 功能只能在 Chat OS 桌面客户端中使用",
        })),
    ))
}

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
    let session_project_id =
        resolve_session_project_scope(session.project_id.as_deref(), session.metadata.as_ref());
    if session_project_id != PUBLIC_PROJECT_ID {
        project_ids.insert(session_project_id);
    }
    if let Some(project_id) = requested_project_id
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "0" && *value != PUBLIC_PROJECT_ID)
    {
        project_ids.insert(project_id.to_string());
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
        project_is_visible_on_request, request_is_local_connector_desktop,
        CHATOS_CLIENT_SURFACE_COMPAT_HEADER, CHATOS_CLIENT_SURFACE_HEADER,
        LOCAL_CONNECTOR_DESKTOP_SURFACE,
    };
    use crate::models::project::Project;

    fn project(source_type: &str, execution_plane: Option<&str>) -> Project {
        let mut project = Project::new(
            "Project".to_string(),
            "/workspace/project".to_string(),
            None,
            None,
            Some("user-1".to_string()),
        );
        project.source_type = Some(source_type.to_string());
        project.execution_plane = execution_plane.map(ToOwned::to_owned);
        project
    }

    #[test]
    fn local_workspace_projects_are_cloud_business_records_visible_on_every_surface() {
        let local = project("local_connector", Some("local_connector"));
        let cloud = project("cloud", Some("cloud"));
        let browser_headers = HeaderMap::new();
        assert!(project_is_visible_on_request(&local, &browser_headers));
        assert!(project_is_visible_on_request(&cloud, &browser_headers));

        let mut desktop_headers = HeaderMap::new();
        desktop_headers.insert(
            CHATOS_CLIENT_SURFACE_HEADER,
            HeaderValue::from_static(LOCAL_CONNECTOR_DESKTOP_SURFACE),
        );
        assert!(project_is_visible_on_request(&local, &desktop_headers));

        let mut compatibility_headers = HeaderMap::new();
        compatibility_headers.insert(
            CHATOS_CLIENT_SURFACE_COMPAT_HEADER,
            HeaderValue::from_static(LOCAL_CONNECTOR_DESKTOP_SURFACE),
        );
        assert!(project_is_visible_on_request(
            &local,
            &compatibility_headers
        ));
    }

    #[test]
    fn desktop_surface_requires_the_expected_value_on_either_supported_header() {
        for header in [
            CHATOS_CLIENT_SURFACE_HEADER,
            CHATOS_CLIENT_SURFACE_COMPAT_HEADER,
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(
                header,
                HeaderValue::from_static(LOCAL_CONNECTOR_DESKTOP_SURFACE),
            );
            assert!(request_is_local_connector_desktop(&headers));

            headers.insert(header, HeaderValue::from_static("XMLHttpRequest"));
            assert!(!request_is_local_connector_desktop(&headers));
        }
    }
}
