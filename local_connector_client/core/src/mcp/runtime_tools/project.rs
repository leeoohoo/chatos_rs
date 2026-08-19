// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::relay::RelayRequest;
use crate::workspace::paths::{
    canonicalize_existing_dir, normalize_relative_workspace_path,
    normalize_request_workspace_relative_path, request_cwd, request_default_tool_root,
    resolve_workspace_dir,
};
use crate::WorkspaceState;

pub(crate) fn request_project_root(
    workspace: &WorkspaceState,
    request: &RelayRequest,
) -> Result<PathBuf> {
    let root = canonicalize_existing_dir(workspace.absolute_root.as_path())?;
    let Some(cwd) = request_cwd(request) else {
        return Ok(root);
    };
    resolve_workspace_dir(workspace, normalize_relative_workspace_path(cwd)?.as_str())
}

pub(crate) fn normalize_request_project_relative_path(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    requested: &str,
) -> Result<String> {
    let workspace_relative =
        normalize_request_workspace_relative_path(workspace, request, requested)?;
    let Some(base) = request_cwd(request)
        .map(normalize_relative_workspace_path)
        .transpose()?
        .filter(|value| value != ".")
    else {
        return Ok(workspace_relative);
    };
    if workspace_relative == base {
        return Ok(".".to_string());
    }
    workspace_relative
        .strip_prefix(format!("{base}/").as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("path is outside current local project"))
}

pub(crate) fn normalize_request_task_project_relative_path(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    requested: &str,
) -> Result<String> {
    let project_relative = normalize_request_project_relative_path(workspace, request, requested)?;
    apply_default_tool_root(request, project_relative)
}

pub(crate) fn normalize_request_task_existing_dir_relative_path(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    project_root: &Path,
    requested: &str,
) -> Result<String> {
    let project_relative = normalize_request_project_relative_path(workspace, request, requested)?;
    let scoped = apply_default_tool_root(request, project_relative.clone())?;
    let Some(default_root) = request_default_tool_root(request)? else {
        return Ok(scoped);
    };
    if scoped == default_root && !project_root.join(default_root.as_str()).is_dir() {
        return Ok(project_relative);
    }
    Ok(scoped)
}

fn apply_default_tool_root(request: &RelayRequest, project_relative: String) -> Result<String> {
    let Some(default_root) = request_default_tool_root(request)? else {
        return Ok(project_relative);
    };
    let default_root = default_root.trim();
    if default_root.is_empty() || default_root == "." {
        return Ok(project_relative);
    }
    if project_relative == "." {
        return Ok(default_root.to_string());
    }
    if project_relative == default_root
        || project_relative.starts_with(format!("{default_root}/").as_str())
    {
        return Ok(project_relative);
    }
    Ok(format!("{default_root}/{project_relative}"))
}
