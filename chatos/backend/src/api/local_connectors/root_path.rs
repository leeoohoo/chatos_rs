// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use super::error;
pub(crate) type LocalConnectorRootRef = chatos_local_workspace::LocalConnectorWorkspaceRef;

pub(super) fn sanitize_optional_local_relative_path(
    value: Option<&str>,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some(normalized) = normalize_local_relative_path(Some(value)) else {
        return Ok(None);
    };
    if chatos_local_workspace::local_connector_relative_path_is_safe(normalized.as_str()) {
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
    chatos_local_workspace::normalize_local_connector_relative_path(value)
}

pub(crate) fn parse_local_connector_root_path(root_path: &str) -> Option<LocalConnectorRootRef> {
    chatos_local_workspace::parse_local_connector_workspace_root(root_path)
}

pub(crate) fn local_connector_root_path(
    device_id: &str,
    workspace_id: &str,
    relative_path: Option<&str>,
) -> String {
    match chatos_local_workspace::local_connector_workspace_root(
        device_id,
        workspace_id,
        relative_path,
    ) {
        Some(root_path) => root_path,
        None => {
            tracing::error!("validated Local Connector root parts could not be formatted");
            "local://connector/invalid".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{local_connector_root_path, parse_local_connector_root_path};

    #[test]
    fn invalid_root_parts_fail_closed_to_an_unparseable_local_reference() {
        let root = local_connector_root_path("", "workspace-1", None);
        assert_eq!(root, "local://connector/invalid");
        assert!(parse_local_connector_root_path(root.as_str()).is_none());
    }
}
