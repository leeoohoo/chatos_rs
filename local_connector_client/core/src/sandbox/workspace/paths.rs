// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use uuid::Uuid;

use crate::WorkspaceState;

pub(crate) fn local_sandbox_baseline_workspace(run_workspace: &Path) -> Result<PathBuf> {
    let run_root = run_workspace
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| anyhow!("invalid local sandbox run_workspace"))?;
    Ok(run_root.join("baseline").join("workspace"))
}

pub(super) fn local_sandbox_workspace_root(workspace: &WorkspaceState) -> Result<PathBuf> {
    let root = workspace.absolute_root.join(".chatos").join("task-runner");
    fs::create_dir_all(root.as_path())
        .with_context(|| format!("create local sandbox workspace root {}", root.display()))?;
    Ok(root)
}

pub(super) fn sanitize_path_segment(value: &str) -> String {
    let sanitized = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches(['-', '.', '_']);
    if sanitized.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        sanitized.to_string()
    }
}
