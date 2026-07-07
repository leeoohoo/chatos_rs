// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};

use crate::relay::RelayRequest;
use crate::WorkspaceState;

use super::normalize::normalize_request_workspace_relative_path;

#[cfg(test)]
pub(crate) fn resolve_request_workspace_path(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    requested: &str,
) -> Result<PathBuf> {
    let combined = normalize_request_workspace_relative_path(workspace, request, requested)?;
    resolve_workspace_path(workspace, combined.as_str())
}

pub(crate) fn resolve_request_workspace_dir(
    workspace: &WorkspaceState,
    request: &RelayRequest,
    requested: &str,
) -> Result<PathBuf> {
    let combined = normalize_request_workspace_relative_path(workspace, request, requested)?;
    resolve_workspace_dir(workspace, combined.as_str())
}

pub(crate) fn resolve_workspace_path(
    workspace: &WorkspaceState,
    requested: &str,
) -> Result<PathBuf> {
    let root = canonicalize_existing_dir(workspace.absolute_root.as_path())?;
    let safe_requested = requested.trim_start_matches('/');
    let requested_path = Path::new(safe_requested);
    if requested_path.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err(anyhow!(
            "write path contains unsupported parent/root component"
        ));
    }
    let candidate = root.join(requested_path);
    let canonical = candidate
        .canonicalize()
        .with_context(|| format!("resolve workspace path {}", candidate.display()))?;
    if !canonical.starts_with(root.as_path()) {
        return Err(anyhow!("path escapes authorized workspace"));
    }
    Ok(canonical)
}

pub(crate) fn resolve_workspace_dir(
    workspace: &WorkspaceState,
    requested: &str,
) -> Result<PathBuf> {
    let dir = resolve_workspace_path(workspace, requested)?;
    if !dir.is_dir() {
        return Err(anyhow!("cwd is not a directory: {}", dir.display()));
    }
    Ok(dir)
}

pub(crate) fn canonicalize_existing_dir(path: &Path) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("canonicalize workspace path {}", path.display()))?;
    if !canonical.is_dir() {
        return Err(anyhow!(
            "workspace path is not a directory: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

pub(crate) fn relative_to_workspace(workspace: &WorkspaceState, path: &Path) -> String {
    path.strip_prefix(workspace.absolute_root.as_path())
        .ok()
        .map(|path| path.display().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| ".".to_string())
}

pub(crate) fn workspace_fingerprint(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.display().to_string().as_bytes());
    hex::encode(hasher.finalize())
}
