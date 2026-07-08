// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::relay::RelayRequest;
use crate::{WorkspaceState, LOCAL_CONNECTOR_ROOT_PREFIX};

use super::request_cwd;
use super::resolve::canonicalize_existing_dir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestedPathOrigin {
    EmptyOrCurrent,
    ProjectRelative,
    WorkspaceAbsolute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedRequestedPath {
    relative_path: String,
    origin: RequestedPathOrigin,
}

pub(crate) fn normalize_request_workspace_relative_path(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    requested: &str,
) -> Result<String> {
    let base = request_cwd(request)
        .map(normalize_relative_workspace_path)
        .transpose()?
        .filter(|value| value != ".");
    let requested = normalize_requested_path(workspace, request, requested)?;
    combine_request_path(base.as_deref(), requested)
}

fn normalize_requested_path(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    requested: &str,
) -> Result<NormalizedRequestedPath> {
    let trimmed = requested.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == "/" {
        return Ok(NormalizedRequestedPath {
            relative_path: ".".to_string(),
            origin: RequestedPathOrigin::EmptyOrCurrent,
        });
    }

    if let Some(relative_path) = connector_uri_workspace_relative_path(request, trimmed)? {
        return Ok(NormalizedRequestedPath {
            relative_path: normalize_relative_workspace_path(relative_path.as_str())?,
            origin: RequestedPathOrigin::WorkspaceAbsolute,
        });
    }

    if Path::new(trimmed).is_absolute() {
        return Ok(NormalizedRequestedPath {
            relative_path: absolute_workspace_relative_path(workspace, trimmed)?,
            origin: RequestedPathOrigin::WorkspaceAbsolute,
        });
    }

    Ok(NormalizedRequestedPath {
        relative_path: normalize_relative_workspace_path(trimmed)?,
        origin: RequestedPathOrigin::ProjectRelative,
    })
}

fn connector_uri_workspace_relative_path(
    request: &RelayRequest,
    requested: &str,
) -> Result<Option<String>> {
    let Some(stripped) = requested.strip_prefix(LOCAL_CONNECTOR_ROOT_PREFIX) else {
        return Ok(None);
    };
    let mut parts = stripped.split('/');
    let device_id = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("local connector path is missing device id"))?;
    let workspace_id = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("local connector path is missing workspace id"))?;
    if request
        .device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|request_device_id| request_device_id != device_id)
    {
        return Err(anyhow!("local connector path targets another device"));
    }
    if workspace_id != request.workspace_id {
        return Err(anyhow!("local connector path targets another workspace"));
    }
    Ok(Some(parts.collect::<Vec<_>>().join("/")))
}

fn absolute_workspace_relative_path(workspace: &WorkspaceState, requested: &str) -> Result<String> {
    let root = canonicalize_existing_dir(workspace.absolute_root.as_path())?;
    let requested_path = normalize_absolute_path_for_workspace(Path::new(requested.trim()));
    if !requested_path.starts_with(root.as_path()) {
        return Err(anyhow!("absolute path is outside authorized workspace"));
    }
    let relative = requested_path
        .strip_prefix(root.as_path())
        .map_err(|_| anyhow!("absolute path is outside authorized workspace"))?;
    normalize_relative_workspace_path(relative.to_string_lossy().as_ref())
}

fn normalize_absolute_path_for_workspace(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }

    let mut suffix = PathBuf::new();
    let mut cursor = path;
    while let Some(parent) = cursor.parent() {
        if let Some(file_name) = cursor.file_name() {
            let mut next_suffix = PathBuf::from(file_name);
            next_suffix.push(suffix);
            suffix = next_suffix;
        }
        if let Ok(canonical_parent) = parent.canonicalize() {
            return canonical_parent.join(suffix);
        }
        cursor = parent;
    }
    path.to_path_buf()
}

pub(crate) fn normalize_relative_workspace_path(value: &str) -> Result<String> {
    let normalized = value.trim().replace('\\', "/");
    let stripped = normalized.trim_start_matches('/');
    if stripped.is_empty() || stripped == "." {
        return Ok(".".to_string());
    }

    let mut clean = PathBuf::new();
    for component in Path::new(stripped).components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(part) => clean.push(part),
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(anyhow!(
                    "workspace path contains unsupported parent/root component"
                ));
            }
        }
    }
    let value = clean.to_string_lossy().replace('\\', "/");
    Ok(if value.is_empty() {
        ".".to_string()
    } else {
        value
    })
}

fn combine_request_path(base: Option<&str>, requested: NormalizedRequestedPath) -> Result<String> {
    let requested_path = requested.relative_path.as_str();
    let Some(base) = base
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != ".")
    else {
        return Ok(requested.relative_path);
    };

    if requested_path == "." {
        return Ok(base.to_string());
    }
    if requested_path == base || requested_path.starts_with(format!("{base}/").as_str()) {
        return Ok(requested.relative_path);
    }
    if requested.origin == RequestedPathOrigin::WorkspaceAbsolute {
        return Err(anyhow!("path is outside current local project"));
    }
    Ok(format!("{base}/{requested_path}"))
}
