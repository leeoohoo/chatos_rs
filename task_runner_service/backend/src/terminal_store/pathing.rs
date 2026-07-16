// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

pub(super) fn canonicalize_existing(path: &Path) -> Result<PathBuf, String> {
    chatos_terminal_runtime::canonicalize_existing(path)
        .map_err(|_| "workspace path is not available".to_string())
}

pub(super) fn resolve_target_path(root: &Path, path_input: &str) -> Result<PathBuf, String> {
    chatos_terminal_runtime::resolve_target_path(root, path_input).map_err(|error| match error {
        chatos_terminal_runtime::TerminalPathError::Unavailable(_) => {
            "workspace path is not available".to_string()
        }
        chatos_terminal_runtime::TerminalPathError::EscapesWorkspace => error.to_string(),
    })
}

pub(super) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub(super) fn display_workspace_path(root: &Path, path: &Path) -> String {
    chatos_terminal_runtime::display_workspace_path(root, path)
}
