// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;

use anyhow::{anyhow, Result};

use crate::relay::RelayRequest;
use crate::workspace::paths::{
    canonicalize_existing_dir, normalize_relative_workspace_path,
    normalize_request_workspace_relative_path, request_cwd, resolve_workspace_dir,
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
