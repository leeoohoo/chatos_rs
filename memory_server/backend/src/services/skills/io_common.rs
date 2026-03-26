use std::collections::HashSet;
use std::fs;
use std::path::{Path as FsPath, PathBuf};

use tokio::task;

use super::io_helpers::{normalize_repo_relative_path, sanitize_user_segment};

pub fn resolve_skill_state_root(user_id: &str) -> PathBuf {
    let user_segment = sanitize_user_segment(user_id);
    if let Ok(raw) = std::env::var("MEMORY_SKILL_STATE_ROOT") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join(user_segment);
        }
    }

    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    home.join(".chatos")
        .join("memory_skill_center")
        .join(user_segment)
}

pub async fn ensure_dir_async(path: PathBuf) -> Result<(), String> {
    run_blocking_result(move || ensure_dir(path.as_path())).await
}

pub fn normalize_plugin_source(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    normalized.trim_matches('/').to_string()
}

pub fn resolve_plugin_root_from_cache(
    plugins_root: &FsPath,
    cache_path: Option<&str>,
    source: &str,
) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = cache_path
        .map(normalize_repo_relative_path)
        .filter(|value| !value.is_empty())
    {
        candidates.push(value);
    }
    let normalized = normalize_plugin_source(source);
    if !normalized.is_empty() {
        candidates.push(normalized.clone());
        if let Some(stripped) = normalized.strip_prefix("plugins/") {
            candidates.push(stripped.to_string());
        } else {
            candidates.push(format!("plugins/{}", normalized));
        }
    }
    for rel in unique_strings(candidates) {
        let path = plugins_root.join(rel.as_str());
        if path.exists() && path.is_dir() {
            return Some(path);
        }
    }
    None
}

pub fn unique_strings(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in values {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.clone()) {
            out.push(trimmed);
        }
    }
    out
}

pub(super) async fn run_blocking_result<T, F>(func: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    task::spawn_blocking(func)
        .await
        .map_err(|err| format!("blocking task join failed: {}", err))?
}

pub(super) fn ensure_dir(path: &FsPath) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    fs::create_dir_all(path).map_err(|err| err.to_string())
}
