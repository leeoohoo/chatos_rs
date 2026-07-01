// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;

pub(super) fn ensure_sqlite_parent_dir(database_url: &str) -> Result<(), String> {
    let Some(path) = sqlite_database_path(database_url) else {
        return Ok(());
    };
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|err| err.to_string())
}

fn sqlite_database_path(database_url: &str) -> Option<PathBuf> {
    let normalized = database_url.trim();
    if normalized.is_empty() || normalized == "sqlite::memory:" {
        return None;
    }
    let path = normalized
        .strip_prefix("sqlite://")
        .or_else(|| normalized.strip_prefix("sqlite:"))?;
    let path = path.split('?').next().unwrap_or(path).trim();
    if path.is_empty() || path == ":memory:" {
        None
    } else {
        Some(PathBuf::from(path))
    }
}
