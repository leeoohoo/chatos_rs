// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use super::error;
const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local://connector/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalConnectorRootRef {
    pub device_id: String,
    pub workspace_id: String,
    pub relative_path: Option<String>,
}

pub(super) fn sanitize_optional_local_relative_path(
    value: Option<&str>,
) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some(normalized) = normalize_local_relative_path(Some(value)) else {
        return Ok(None);
    };
    if local_relative_path_is_safe(normalized.as_str()) {
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
    let value = value?.trim().replace('\\', "/");
    let parts = value
        .trim_matches('/')
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("/"))
}

pub(crate) fn parse_local_connector_root_path(root_path: &str) -> Option<LocalConnectorRootRef> {
    let rest = root_path.trim().strip_prefix(LOCAL_CONNECTOR_ROOT_PREFIX)?;
    let mut parts = rest.splitn(3, '/');
    let device_id = normalized_text(parts.next()?)?.to_string();
    let workspace_id = normalized_text(parts.next()?)?.to_string();
    let relative_path = match parts.next() {
        Some(path) => Some(decode_relative_path(path)?),
        None => None,
    };
    Some(LocalConnectorRootRef {
        device_id,
        workspace_id,
        relative_path,
    })
}

pub(crate) fn local_connector_root_path(
    device_id: &str,
    workspace_id: &str,
    relative_path: Option<&str>,
) -> String {
    match format_local_root(device_id, workspace_id, relative_path) {
        Some(root_path) => root_path,
        None => {
            tracing::error!("validated Local Connector root parts could not be formatted");
            "local://connector/invalid".to_string()
        }
    }
}

fn format_local_root(
    device_id: &str,
    workspace_id: &str,
    relative_path: Option<&str>,
) -> Option<String> {
    let device_id = normalized_text(device_id)?;
    let workspace_id = normalized_text(workspace_id)?;
    let base = format!("{LOCAL_CONNECTOR_ROOT_PREFIX}{device_id}/{workspace_id}");
    let Some(path) = normalize_local_relative_path(relative_path) else {
        return Some(base);
    };
    if !local_relative_path_is_safe(path.as_str()) {
        return None;
    }
    let encoded = path
        .split('/')
        .map(|part| urlencoding::encode(part).into_owned())
        .collect::<Vec<_>>()
        .join("/");
    Some(format!("{base}/{encoded}"))
}

fn decode_relative_path(path: &str) -> Option<String> {
    let decoded = path
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .map(|part| urlencoding::decode(part).map(|part| part.into_owned()))
        .collect::<Result<Vec<_>, _>>()
        .ok()?
        .join("/");
    let normalized = normalize_local_relative_path(Some(decoded.as_str()));
    normalized.filter(|path| local_relative_path_is_safe(path))
}

fn local_relative_path_is_safe(path: &str) -> bool {
    let path = path.trim();
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && path
            .split('/')
            .all(|part| !part.trim().is_empty() && part != "." && part != "..")
}

fn normalized_text(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
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
