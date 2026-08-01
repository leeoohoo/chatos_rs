// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use super::error;
pub(crate) type LocalConnectorRootRef = chatos_project_execution::LocalConnectorWorkspaceRef;

pub(super) fn sanitize_optional_local_relative_path(
    value: Option<&str>,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some(normalized) = normalize_local_relative_path(Some(value)) else {
        return Ok(None);
    };
    if chatos_project_execution::local_connector_relative_path_is_safe(normalized.as_str()) {
        Ok(Some(normalized))
    } else {
        Err(error(
            StatusCode::BAD_REQUEST,
            "本地目录路径不能包含 .. 或绝对路径",
        ))
    }
}

pub(super) fn sanitize_required_local_relative_path(
    value: Option<&str>,
    field: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    match sanitize_optional_local_relative_path(value)? {
        Some(value) => Ok(value),
        None => Err(error(StatusCode::BAD_REQUEST, format!("{field} 不能为空"))),
    }
}

pub(super) fn normalize_local_relative_path(value: Option<&str>) -> Option<String> {
    chatos_project_execution::normalize_local_connector_relative_path(value)
}

pub(super) fn local_relative_basename(path: &str) -> Option<String> {
    normalize_local_relative_path(Some(path)).and_then(|path| {
        path.rsplit('/')
            .find(|part| !part.trim().is_empty())
            .map(ToOwned::to_owned)
    })
}

pub(crate) fn parse_local_connector_root_path(root_path: &str) -> Option<LocalConnectorRootRef> {
    chatos_project_execution::parse_local_connector_workspace_root(root_path)
}

pub(crate) fn local_connector_root_path(
    device_id: &str,
    workspace_id: &str,
    relative_path: Option<&str>,
) -> String {
    chatos_project_execution::local_connector_workspace_root(device_id, workspace_id, relative_path)
        .expect("Local Connector root parts must be validated before formatting")
}

pub(crate) fn local_connector_display_path(root_path: &str) -> Option<String> {
    let root_ref = parse_local_connector_root_path(root_path)?;
    Some(match root_ref.relative_path {
        Some(relative_path) => format!("/{relative_path}"),
        None => "/".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{local_connector_display_path, local_connector_root_path};

    #[test]
    fn display_path_hides_connector_routing_ids() {
        let root = local_connector_root_path("device-1", "workspace-1", Some("apps/my backend"));
        assert_eq!(
            local_connector_display_path(root.as_str()).as_deref(),
            Some("/apps/my backend")
        );
        assert_eq!(
            local_connector_display_path("local://connector/device-1/workspace-1").as_deref(),
            Some("/")
        );
    }
}
