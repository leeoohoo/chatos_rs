// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path as FsPath;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::workspace::paths::{
    canonicalize_existing_dir, normalize_relative_workspace_path, relative_to_workspace,
    resolve_workspace_dir,
};
use crate::{LocalRuntime, WorkspaceState};

use super::context::owner_context;
use super::error::LocalRuntimeApiError;

#[derive(Debug, Serialize)]
pub(super) struct LocalDeviceResponse {
    id: String,
    display_name: String,
    status: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct LocalWorkspaceResponse {
    id: String,
    device_id: String,
    display_name: String,
    local_path_alias: String,
    status: &'static str,
}

#[derive(Debug, Serialize)]
pub(super) struct LocalDirectoryEntryResponse {
    name: String,
    path: String,
    is_dir: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct LocalDirectoryListResponse {
    path: String,
    parent: Option<String>,
    entries: Vec<LocalDirectoryEntryResponse>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalDirectoryQuery {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateLocalDirectoryRequest {
    path: String,
}

#[derive(Debug, Serialize)]
pub(super) struct CreateLocalDirectoryResponse {
    path: String,
    created: bool,
}

pub(super) async fn list_devices(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Vec<LocalDeviceResponse>>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let state = runtime.state.read().await;
    let display_name = state
        .auth
        .as_ref()
        .map(|auth| auth.device_name.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("Local Connector")
        .to_string();
    Ok(Json(vec![LocalDeviceResponse {
        id: owner.device_id,
        display_name,
        status: "online",
    }]))
}

pub(super) async fn list_workspaces(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Vec<LocalWorkspaceResponse>>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let state = runtime.state.read().await;
    Ok(Json(
        state
            .workspaces
            .iter()
            .map(|workspace| LocalWorkspaceResponse {
                id: workspace.id.clone(),
                device_id: owner.device_id.clone(),
                display_name: workspace.alias.clone(),
                local_path_alias: workspace.alias.clone(),
                status: "active",
            })
            .collect(),
    ))
}

pub(super) async fn list_directory(
    Path(workspace_id): Path<String>,
    Query(query): Query<LocalDirectoryQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalDirectoryListResponse>, LocalRuntimeApiError> {
    owner_context(&runtime).await?;
    let workspace = workspace(&runtime, workspace_id.as_str()).await?;
    list_workspace_directory(&workspace, query.path.as_deref().unwrap_or("."))
        .map(Json)
        .map_err(workspace_error)
}

pub(super) async fn create_directory(
    Path(workspace_id): Path<String>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<CreateLocalDirectoryRequest>,
) -> Result<Json<CreateLocalDirectoryResponse>, LocalRuntimeApiError> {
    owner_context(&runtime).await?;
    let workspace = workspace(&runtime, workspace_id.as_str()).await?;
    let path =
        create_workspace_directory(&workspace, request.path.as_str()).map_err(workspace_error)?;
    Ok(Json(CreateLocalDirectoryResponse {
        path,
        created: true,
    }))
}

async fn workspace(
    runtime: &LocalRuntime,
    workspace_id: &str,
) -> Result<WorkspaceState, LocalRuntimeApiError> {
    let workspace_id = workspace_id.trim();
    let state = runtime.state.read().await;
    state.workspace_by_id(workspace_id).cloned().ok_or_else(|| {
        LocalRuntimeApiError::bad_request(
            "local_runtime_workspace_not_found",
            "The selected workspace is not registered on this device",
        )
    })
}

fn workspace_error(error: anyhow::Error) -> LocalRuntimeApiError {
    LocalRuntimeApiError::bad_request("local_runtime_workspace_path_invalid", error.to_string())
}

fn list_workspace_directory(
    workspace: &WorkspaceState,
    requested_path: &str,
) -> anyhow::Result<LocalDirectoryListResponse> {
    let directory = resolve_workspace_dir(workspace, requested_path)?;
    let path = relative_to_workspace(workspace, directory.as_path());
    let parent = directory
        .parent()
        .filter(|parent| parent.starts_with(workspace.absolute_root.as_path()))
        .map(|parent| relative_to_workspace(workspace, parent))
        .filter(|parent| parent != &path);
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory.as_path())? {
        let entry = entry?;
        let metadata = entry.file_type()?;
        if !metadata.is_dir() || metadata.is_symlink() {
            continue;
        }
        entries.push(LocalDirectoryEntryResponse {
            name: entry.file_name().to_string_lossy().to_string(),
            path: relative_to_workspace(workspace, entry.path().as_path()),
            is_dir: true,
        });
    }
    entries.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(LocalDirectoryListResponse {
        path,
        parent,
        entries,
    })
}

fn create_workspace_directory(
    workspace: &WorkspaceState,
    requested_path: &str,
) -> anyhow::Result<String> {
    let normalized = normalize_relative_workspace_path(requested_path)?;
    if normalized == "." {
        anyhow::bail!("directory path must not be the workspace root");
    }
    let root = canonicalize_existing_dir(workspace.absolute_root.as_path())?;
    let mut current = root;
    for component in FsPath::new(normalized.as_str()).components() {
        let std::path::Component::Normal(segment) = component else {
            anyhow::bail!("directory path contains an unsupported component");
        };
        current.push(segment);
        match fs::symlink_metadata(current.as_path()) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                anyhow::bail!("directory path crosses a symbolic link");
            }
            Ok(metadata) if !metadata.is_dir() => {
                anyhow::bail!("directory path contains a non-directory entry");
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(current.as_path())?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::{create_workspace_directory, list_workspace_directory};
    use crate::WorkspaceState;

    fn workspace(root: PathBuf) -> WorkspaceState {
        WorkspaceState {
            id: "workspace-1".to_string(),
            absolute_root: root,
            alias: "work".to_string(),
            fingerprint: "fingerprint".to_string(),
            project_config_trust: None,
        }
    }

    use std::path::PathBuf;

    #[test]
    fn lists_and_creates_directories_relative_to_the_authorized_workspace() {
        let root = std::env::temp_dir().join(format!(
            "chatos-local-workspace-api-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join("apps/backend")).expect("create test directories");
        let workspace = workspace(root.canonicalize().expect("canonical workspace"));

        let listing = list_workspace_directory(&workspace, "apps").expect("list workspace");
        assert_eq!(listing.path, "apps");
        assert_eq!(listing.parent.as_deref(), Some("."));
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].path, "apps/backend");

        let created =
            create_workspace_directory(&workspace, "apps/frontend/src").expect("create directory");
        assert_eq!(created, "apps/frontend/src");
        assert!(root.join("apps/frontend/src").is_dir());
        assert!(create_workspace_directory(&workspace, "../outside").is_err());

        let _ = std::fs::remove_dir_all(root);
    }
}
