// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use crate::workspace::paths::{canonicalize_existing_dir, normalize_relative_workspace_path};
use crate::{LocalRuntime, WorkspaceState, LOCAL_CONNECTOR_ROOT_PREFIX};

use super::error::LocalRuntimeApiError;

#[derive(Debug, Clone)]
pub(super) struct LocalWorkspacePath {
    pub(super) device_id: String,
    pub(super) workspace: WorkspaceState,
    pub(super) relative_path: String,
    pub(super) path: PathBuf,
}

impl LocalWorkspacePath {
    pub(super) fn logical_path(&self) -> String {
        logical_workspace_path(
            self.device_id.as_str(),
            self.workspace.id.as_str(),
            self.relative_path.as_str(),
        )
    }

    pub(super) fn logical_child(&self, relative_path: &str) -> String {
        logical_workspace_path(
            self.device_id.as_str(),
            self.workspace.id.as_str(),
            relative_path,
        )
    }
}

pub(super) async fn resolve_local_workspace_path(
    runtime: &LocalRuntime,
    raw_path: &str,
    allow_missing: bool,
) -> Result<LocalWorkspacePath, LocalRuntimeApiError> {
    let raw_path = raw_path.trim();
    let Some(path_suffix) = raw_path.strip_prefix(LOCAL_CONNECTOR_ROOT_PREFIX) else {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_path_required",
            "A local://connector path is required",
        ));
    };
    let mut parts = path_suffix.split('/');
    let device_id = decode_segment(parts.next(), "device id")?;
    let workspace_id = decode_segment(parts.next(), "workspace id")?;
    let relative_path = parts
        .map(|part| {
            urlencoding::decode(part)
                .map(|value| value.into_owned())
                .map_err(|_| path_error("Local path contains invalid encoding"))
        })
        .collect::<Result<Vec<_>, _>>()?
        .join("/");
    let relative_path = normalize_relative_workspace_path(relative_path.as_str())
        .map_err(|error| path_error(error.to_string()))?;

    let (local_device_id, workspace) = {
        let state = runtime.state.read().await;
        let local_device_id = state
            .device_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                LocalRuntimeApiError::conflict(
                    "local_runtime_device_unavailable",
                    "Local device identity is unavailable",
                )
            })?;
        if local_device_id != device_id {
            return Err(path_error("Local path targets another device"));
        }
        let workspace = state
            .workspace_by_id(workspace_id.as_str())
            .cloned()
            .ok_or_else(|| {
                LocalRuntimeApiError::not_found(
                    "local_runtime_workspace_not_found",
                    "The local workspace is not registered on this device",
                )
            })?;
        (local_device_id.to_string(), workspace)
    };

    let root = canonicalize_existing_dir(workspace.absolute_root.as_path())
        .map_err(|error| path_error(error.to_string()))?;
    let candidate = if relative_path == "." {
        root.clone()
    } else {
        root.join(relative_path.as_str())
    };
    let path = if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|error| path_error(error.to_string()))?
    } else if allow_missing {
        ensure_missing_path_stays_within_root(root.as_path(), candidate.as_path())?;
        candidate
    } else {
        return Err(LocalRuntimeApiError::not_found(
            "local_runtime_path_not_found",
            "Local path does not exist",
        ));
    };
    if !path.starts_with(root.as_path()) {
        return Err(path_error("Local path escapes the authorized workspace"));
    }

    Ok(LocalWorkspacePath {
        device_id: local_device_id,
        workspace,
        relative_path,
        path,
    })
}

pub(super) fn logical_workspace_path(
    device_id: &str,
    workspace_id: &str,
    relative_path: &str,
) -> String {
    let base = format!(
        "{LOCAL_CONNECTOR_ROOT_PREFIX}{}/{}",
        urlencoding::encode(device_id),
        urlencoding::encode(workspace_id),
    );
    let relative = relative_path.trim().trim_matches('/');
    if relative.is_empty() || relative == "." {
        return base;
    }
    let encoded = relative
        .split('/')
        .map(|part| urlencoding::encode(part).into_owned())
        .collect::<Vec<_>>()
        .join("/");
    format!("{base}/{encoded}")
}

fn decode_segment(value: Option<&str>, label: &str) -> Result<String, LocalRuntimeApiError> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| path_error(format!("Local path is missing {label}")))?;
    urlencoding::decode(value)
        .map(|value| value.into_owned())
        .map_err(|_| path_error(format!("Local path contains an invalid {label}")))
}

fn ensure_missing_path_stays_within_root(
    root: &Path,
    candidate: &Path,
) -> Result<(), LocalRuntimeApiError> {
    let mut cursor = candidate.parent();
    while let Some(parent) = cursor {
        if parent.exists() {
            let canonical = parent
                .canonicalize()
                .map_err(|error| path_error(error.to_string()))?;
            return if canonical.starts_with(root) {
                Ok(())
            } else {
                Err(path_error("Local path escapes the authorized workspace"))
            };
        }
        cursor = parent.parent();
    }
    Err(path_error("Local path has no authorized parent"))
}

fn path_error(message: impl Into<String>) -> LocalRuntimeApiError {
    LocalRuntimeApiError::bad_request("local_runtime_path_invalid", message)
}
