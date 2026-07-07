// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use sha2::{Digest, Sha256};

const CHATOS_DIR_NAME: &str = ".chatos";
const CACHE_DIR_NAME: &str = "cache";
const LOCAL_CONNECTOR_ROOT_PREFIX: &str = "local://connector/";

pub fn is_local_connector_project_root(project_root: &str) -> bool {
    let trimmed = project_root.trim();
    trimmed == "local://connector" || trimmed.starts_with(LOCAL_CONNECTOR_ROOT_PREFIX)
}

fn normalize_cache_relative_path(relative_path: &str) -> Result<PathBuf, String> {
    let trimmed = relative_path.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err("cache relative path cannot be empty".to_string());
    }

    let candidate = Path::new(trimmed.as_str());
    let mut normalized = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("cache relative path is invalid".to_string());
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err("cache relative path cannot be empty".to_string());
    }
    Ok(normalized)
}

pub fn project_cache_root(project_root: &str) -> PathBuf {
    Path::new(project_root)
        .join(CHATOS_DIR_NAME)
        .join(CACHE_DIR_NAME)
}

pub fn project_cache_file_path(project_root: &str, relative_path: &str) -> Result<PathBuf, String> {
    if is_local_connector_project_root(project_root) {
        return Err("local connector project cache is not stored on the server".to_string());
    }
    let normalized_relative = normalize_cache_relative_path(relative_path)?;
    Ok(project_cache_root(project_root).join(normalized_relative))
}

pub fn read_cache_json<T>(project_root: &str, relative_path: &str) -> Result<Option<T>, String>
where
    T: DeserializeOwned,
{
    if is_local_connector_project_root(project_root) {
        return Ok(None);
    }
    let path = project_cache_file_path(project_root, relative_path)?;
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    serde_json::from_slice::<T>(&bytes)
        .map(Some)
        .map_err(|err| err.to_string())
}

pub fn write_cache_json<T>(project_root: &str, relative_path: &str, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    if is_local_connector_project_root(project_root) {
        return Ok(());
    }
    let path = project_cache_file_path(project_root, relative_path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|err| err.to_string())?;
    fs::write(path, bytes).map_err(|err| err.to_string())
}

pub fn remove_cache_file(project_root: &str, relative_path: &str) -> Result<(), String> {
    if is_local_connector_project_root(project_root) {
        return Ok(());
    }
    let path = project_cache_file_path(project_root, relative_path)?;
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(path).map_err(|err| err.to_string())
}

pub fn cache_key(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.trim().as_bytes());
    let hex = hex::encode(hasher.finalize());
    hex.chars().take(24).collect()
}

pub fn is_project_local_cache_relative_path(path: &str) -> bool {
    let normalized = path
        .trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string();
    normalized == ".chatos/cache" || normalized.starts_with(".chatos/cache/")
}

pub fn is_project_runtime_relative_path(path: &str) -> bool {
    let normalized = path
        .trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string();
    normalized == ".chatos/project-run"
        || normalized.starts_with(".chatos/project-run/")
        || is_project_local_cache_relative_path(normalized.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_connector_roots_do_not_resolve_to_server_cache_paths() {
        let root = "local://connector/device-1/workspace-1/apps/web";

        assert!(is_local_connector_project_root(root));
        assert!(project_cache_file_path(root, "project_run/catalog.json").is_err());
        assert!(
            read_cache_json::<serde_json::Value>(root, "project_run/catalog.json")
                .unwrap()
                .is_none()
        );
        write_cache_json(
            root,
            "project_run/catalog.json",
            &serde_json::json!({"ok": true}),
        )
        .unwrap();
        remove_cache_file(root, "project_run/catalog.json").unwrap();
    }

    #[test]
    fn normal_project_roots_still_resolve_project_cache_paths() {
        let path =
            project_cache_file_path("/tmp/example-project", "project_run/catalog.json").unwrap();
        assert_eq!(
            path,
            Path::new("/tmp/example-project")
                .join(".chatos")
                .join("cache")
                .join("project_run")
                .join("catalog.json")
        );
    }
}
