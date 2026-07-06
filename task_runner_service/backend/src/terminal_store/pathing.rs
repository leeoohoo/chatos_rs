// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

pub(super) fn canonicalize_existing(path: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(path).map_err(|_| "workspace path is not available".to_string())
}

pub(super) fn resolve_target_path(root: &Path, path_input: &str) -> Result<PathBuf, String> {
    let trimmed = path_input.trim();
    let joined = if trimmed.is_empty() || trimmed == "." {
        root.to_path_buf()
    } else {
        let path = PathBuf::from(trimmed);
        if path.is_absolute() {
            path
        } else {
            root.join(path)
        }
    };
    let canonical = canonicalize_existing(joined.as_path())?;
    if !canonical.starts_with(root) {
        return Err("target path escapes workspace root".to_string());
    }
    Ok(canonical)
}

pub(super) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub(super) fn display_workspace_path(root: &Path, path: &Path) -> String {
    if path == root {
        return "/workspace".to_string();
    }
    if let Ok(relative) = path.strip_prefix(root) {
        let relative = relative.to_string_lossy().replace('\\', "/");
        if relative.is_empty() {
            "/workspace".to_string()
        } else {
            format!("/workspace/{}", relative.trim_start_matches('/'))
        }
    } else {
        "/workspace".to_string()
    }
}
