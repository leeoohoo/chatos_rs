// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use axum::extract::{Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::workspace::paths::relative_to_workspace;
use crate::LocalRuntime;

use super::super::error::LocalRuntimeApiError;
use super::super::workspace_path::resolve_local_workspace_path;

const MAX_PREVIEW_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub(super) struct PathQuery {
    pub(super) path: String,
}

pub(super) async fn list_entries(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<PathQuery>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(&runtime, query.path.as_str(), false).await?;
    if !resolved.path.is_dir() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_directory_required",
            "Local path is not a directory",
        ));
    }
    let directory = resolved.clone();
    let entries = tokio::task::spawn_blocking(move || {
        let mut entries = Vec::new();
        for item in fs::read_dir(directory.path.as_path())? {
            let item = item?;
            let metadata = fs::symlink_metadata(item.path())?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            let relative = relative_to_workspace(&directory.workspace, item.path().as_path());
            entries.push(json!({
                "name": item.file_name().to_string_lossy(),
                "path": directory.logical_child(relative.as_str()),
                "display_path": directory.logical_child(relative.as_str()),
                "is_dir": metadata.is_dir(),
                "writable": !metadata.permissions().readonly(),
                "size": metadata.is_file().then_some(metadata.len()),
                "modified_at": modified_at(&metadata),
            }));
        }
        entries.sort_by(|left, right| {
            let left_dir = left.get("is_dir").and_then(Value::as_bool).unwrap_or(false);
            let right_dir = right
                .get("is_dir")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            right_dir.cmp(&left_dir).then_with(|| {
                left.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .cmp(
                        &right
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_ascii_lowercase(),
                    )
            })
        });
        Ok::<_, std::io::Error>(entries)
    })
    .await
    .map_err(|error| LocalRuntimeApiError::internal(error.to_string()))?
    .map_err(|error| {
        LocalRuntimeApiError::bad_request("local_runtime_fs_list_failed", error.to_string())
    })?;

    let parent = resolved
        .path
        .parent()
        .filter(|parent| parent.starts_with(resolved.workspace.absolute_root.as_path()))
        .map(|parent| relative_to_workspace(&resolved.workspace, parent))
        .filter(|parent| parent != &resolved.relative_path)
        .map(|parent| resolved.logical_child(parent.as_str()));
    Ok(Json(json!({
        "path": resolved.logical_path(),
        "display_path": resolved.logical_path(),
        "parent": parent,
        "writable": true,
        "entries": entries,
        "roots": [],
        "local_runtime": true,
    })))
}

pub(super) async fn read_file(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<PathQuery>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(&runtime, query.path.as_str(), false).await?;
    if !resolved.path.is_file() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_file_required",
            "Local path is not a file",
        ));
    }
    let path = resolved.path.clone();
    let (metadata, bytes) = tokio::task::spawn_blocking(move || {
        let metadata = fs::metadata(path.as_path())?;
        if metadata.len() > MAX_PREVIEW_BYTES {
            return Ok::<_, std::io::Error>((metadata, Vec::new()));
        }
        let bytes = fs::read(path.as_path())?;
        Ok((metadata, bytes))
    })
    .await
    .map_err(|error| LocalRuntimeApiError::internal(error.to_string()))?
    .map_err(|error| {
        LocalRuntimeApiError::bad_request("local_runtime_fs_read_failed", error.to_string())
    })?;
    let is_binary = metadata.len() > MAX_PREVIEW_BYTES || looks_binary(bytes.as_slice());
    let content = (!is_binary).then(|| String::from_utf8_lossy(bytes.as_slice()).to_string());
    Ok(Json(json!({
        "path": resolved.logical_path(),
        "display_path": resolved.logical_path(),
        "name": resolved.path.file_name().and_then(|value| value.to_str()).unwrap_or(""),
        "size": metadata.len(),
        "content_type": content_type(resolved.path.as_path(), is_binary),
        "is_binary": is_binary,
        "writable": !metadata.permissions().readonly(),
        "modified_at": modified_at(&metadata),
        "content": content,
    })))
}

fn modified_at(metadata: &fs::Metadata) -> Option<String> {
    metadata
        .modified()
        .ok()
        .map(|value: SystemTime| value)
        .map(|value| DateTime::<Utc>::from(value).to_rfc3339())
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|byte| *byte == 0)
}

pub(super) fn content_type(path: &Path, binary: bool) -> String {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" => "text/javascript",
        "md" | "txt" | "rs" | "py" | "go" | "java" | "kt" | "toml" | "yaml" | "yml" => "text/plain",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        _ if binary => "application/octet-stream",
        _ => "text/plain",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use super::*;
    use crate::local_runtime::{database_path_for_state, LocalDatabase};
    use crate::{LocalState, WorkspaceState};

    #[tokio::test]
    async fn lists_and_reads_files_without_a_cloud_connector_round_trip() {
        let root =
            std::env::temp_dir().join(format!("chatos-local-fs-api-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("project/src")).expect("create project");
        fs::write(root.join("project/src/main.rs"), "fn main() {}").expect("write source");
        let state_path = root.join("state.json");
        let database = LocalDatabase::open(database_path_for_state(state_path.as_path()))
            .await
            .expect("open database");
        let state = LocalState {
            device_id: Some("device-1".to_string()),
            workspaces: vec![WorkspaceState {
                id: "workspace-1".to_string(),
                absolute_root: root.canonicalize().expect("canonical root"),
                alias: "workspace".to_string(),
                fingerprint: "fingerprint".to_string(),
                project_config_trust: None,
            }],
            ..Default::default()
        };
        let runtime = LocalRuntime::new(
            state_path,
            Arc::new(RwLock::new(state)),
            reqwest::Client::new(),
            database,
        );
        let project_root = "local://connector/device-1/workspace-1/project";

        let Json(list) = list_entries(
            State(runtime.clone()),
            Query(PathQuery {
                path: project_root.to_string(),
            }),
        )
        .await
        .expect("list entries");
        assert_eq!(list["entries"][0]["name"], "src");

        let Json(file) = read_file(
            State(runtime.clone()),
            Query(PathQuery {
                path: format!("{project_root}/src/main.rs"),
            }),
        )
        .await
        .expect("read file");
        assert_eq!(file["content"], "fn main() {}");

        runtime.local_database().expect("database").close().await;
        fs::remove_dir_all(root).expect("cleanup project");
    }
}
