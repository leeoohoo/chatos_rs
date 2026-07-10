// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};
pub(super) fn sandbox_workspace_root(workspace_dir: &str) -> Result<PathBuf, String> {
    let root = Path::new(workspace_dir).join(".chatos").join("task-runner");
    fs::create_dir_all(&root).map_err(|err| {
        format!(
            "create sandbox workspace root {} failed: {err}",
            root.display()
        )
    })?;
    Ok(root)
}

pub(super) fn is_local_connector_sandbox_manager(base_url: &str) -> bool {
    base_url.contains("/api/local-connectors/sandbox-facade/")
}

pub(super) fn sandbox_baseline_workspace(run_workspace: &str) -> Result<String, String> {
    let run_workspace = Path::new(run_workspace);
    let run_root = run_workspace
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "invalid sandbox run workspace path".to_string())?;
    Ok(run_root
        .join("baseline")
        .join("workspace")
        .to_string_lossy()
        .to_string())
}

pub(super) fn copy_workspace_to_sandbox(source: &str, destination: &str) -> Result<(), String> {
    super::super::workspace_snapshot::copy_workspace_snapshot(source, destination)
}
