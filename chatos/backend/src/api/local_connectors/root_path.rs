// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use super::error;
pub(crate) const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local://connector/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalConnectorRootRef {
    pub(crate) device_id: String,
    pub(crate) workspace_id: String,
    pub(crate) relative_path: Option<String>,
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
    let value = value.trim_matches('/');
    if value.is_empty() || value == "." {
        return None;
    }
    let parts = value
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn local_relative_path_is_safe(path: &str) -> bool {
    let path = path.trim();
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && path.split('/').all(|part| {
            let part = part.trim();
            !part.is_empty() && part != "." && part != ".."
        })
}

pub(super) fn local_relative_basename(path: &str) -> Option<String> {
    normalize_local_relative_path(Some(path)).and_then(|path| {
        path.rsplit('/')
            .find(|part| !part.trim().is_empty())
            .map(ToOwned::to_owned)
    })
}

fn encode_local_connector_relative_path(path: &str) -> String {
    path.split('/')
        .filter(|part| !part.trim().is_empty())
        .map(|part| urlencoding::encode(part).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn decode_local_connector_relative_path(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in path.split('/').filter(|part| !part.trim().is_empty()) {
        let decoded = urlencoding::decode(part).ok()?.into_owned();
        parts.push(decoded);
    }
    let joined = parts.join("/");
    normalize_local_relative_path(Some(joined.as_str()))
        .filter(|path| local_relative_path_is_safe(path))
}

pub(crate) fn parse_local_connector_root_path(root_path: &str) -> Option<LocalConnectorRootRef> {
    let rest = root_path.trim().strip_prefix(LOCAL_CONNECTOR_ROOT_PREFIX)?;
    let mut parts = rest.splitn(3, '/');
    let device_id = parts.next()?.trim();
    let workspace_id = parts.next()?.trim();
    if device_id.is_empty() || workspace_id.is_empty() {
        return None;
    }
    let relative_path = match parts.next() {
        Some(path) => Some(decode_local_connector_relative_path(path)?),
        None => None,
    };
    Some(LocalConnectorRootRef {
        device_id: device_id.to_string(),
        workspace_id: workspace_id.to_string(),
        relative_path,
    })
}

pub(crate) fn local_connector_root_path(
    device_id: &str,
    workspace_id: &str,
    relative_path: Option<&str>,
) -> String {
    let base = format!("{LOCAL_CONNECTOR_ROOT_PREFIX}{device_id}/{workspace_id}");
    match relative_path.and_then(|value| normalize_local_relative_path(Some(value))) {
        Some(relative_path) => format!(
            "{base}/{}",
            encode_local_connector_relative_path(relative_path.as_str())
        ),
        None => base,
    }
}
