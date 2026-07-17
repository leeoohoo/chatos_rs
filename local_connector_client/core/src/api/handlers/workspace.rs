// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::PathBuf;

use axum::extract::{Path as AxumPath, Query, State};
use axum::Json;
use reqwest::StatusCode;
use serde_json::Value;

use crate::config::{api_url, home_dir, normalize_optional, ClientConfig};
use crate::registration::{ensure_device_registered, ensure_success, ensure_workspace_registered};
use crate::workspace::paths::canonicalize_existing_dir;
use crate::workspace::trust::workspace_project_config_trust_fingerprint;
use crate::{local_now_rfc3339, LocalRuntime, WorkspaceProjectConfigTrust};

use super::super::types::{
    AddWorkspaceRequest, FsEntry, FsListQuery, FsListResponse, LocalApiError,
    UpdateWorkspaceProjectConfigTrustRequest,
};
use super::helpers::normalize_required;
use super::status::status_payload;

pub(crate) async fn local_fs_list_handler(
    Query(query): Query<FsListQuery>,
) -> Result<Json<FsListResponse>, LocalApiError> {
    let path = normalize_optional(query.path.as_deref())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().unwrap_or_else(|| PathBuf::from("/")));
    let canonical = canonicalize_existing_dir(path.as_path())
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    let parent = canonical
        .parent()
        .map(|path| path.display().to_string())
        .filter(|parent| parent != &canonical.display().to_string());
    let mut entries = Vec::new();
    for entry in fs::read_dir(canonical.as_path())
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?
    {
        let entry = entry.map_err(|err| LocalApiError::bad_request(err.to_string()))?;
        let metadata = entry
            .metadata()
            .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
        if metadata.is_dir() {
            entries.push(FsEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry.path().display().to_string(),
                is_dir: true,
            });
        }
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    Ok(Json(FsListResponse {
        path: canonical.display().to_string(),
        parent,
        entries,
    }))
}

pub(crate) async fn local_add_workspace(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<AddWorkspaceRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let workspace_path = PathBuf::from(normalize_required(req.path.as_str(), "path")?);
    let config = {
        let state = runtime.state.read().await;
        ClientConfig::from_state(&state, runtime.state_path.clone())
            .ok_or_else(|| LocalApiError::bad_request("please login first"))?
    };
    {
        let mut state = runtime.state.write().await;
        let device_id = ensure_device_registered(&runtime.http_client, &config, &mut state).await?;
        let workspace_config = ClientConfig {
            workspace_alias: normalize_optional(req.alias.as_deref()),
            ..config.clone()
        };
        ensure_workspace_registered(
            &runtime.http_client,
            &workspace_config,
            &mut state,
            device_id.as_str(),
            workspace_path,
            false,
        )
        .await?;
        state.save(runtime.state_path.as_path())?;
    }
    runtime.start_connector_if_configured().await?;
    Ok(Json(status_payload(&runtime).await))
}

pub(crate) async fn local_remove_workspace(
    State(runtime): State<LocalRuntime>,
    AxumPath(workspace_id): AxumPath<String>,
) -> Result<Json<Value>, LocalApiError> {
    let (cloud_base_url, access_token) = {
        let state = runtime.state.read().await;
        let auth = state
            .auth
            .as_ref()
            .ok_or_else(|| LocalApiError::bad_request("please login first"))?;
        (auth.cloud_base_url.clone(), auth.access_token.clone())
    };
    let response = runtime
        .http_client
        .delete(
            api_url(
                cloud_base_url.as_str(),
                format!(
                    "/api/local-connectors/workspaces/{}",
                    urlencoding::encode(workspace_id.as_str())
                )
                .as_str(),
            )
            .as_str(),
        )
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    if !response.status().is_success() && response.status() != StatusCode::NOT_FOUND {
        ensure_success(response.status(), "delete workspace")
            .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    }
    {
        let mut state = runtime.state.write().await;
        state
            .workspaces
            .retain(|workspace| workspace.id != workspace_id);
        state.save(runtime.state_path.as_path())?;
    }
    Ok(Json(status_payload(&runtime).await))
}

pub(crate) async fn local_update_workspace_project_config_trust(
    State(runtime): State<LocalRuntime>,
    AxumPath(workspace_id): AxumPath<String>,
    Json(req): Json<UpdateWorkspaceProjectConfigTrustRequest>,
) -> Result<Json<Value>, LocalApiError> {
    {
        let mut state = runtime.state.write().await;
        let workspace = state
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.id == workspace_id)
            .ok_or_else(|| LocalApiError::bad_request("workspace not found"))?;
        update_workspace_project_config_trust(workspace, &req)?;
        state.save(runtime.state_path.as_path())?;
    }
    Ok(Json(status_payload(&runtime).await))
}

fn update_workspace_project_config_trust(
    workspace: &mut crate::WorkspaceState,
    req: &UpdateWorkspaceProjectConfigTrustRequest,
) -> Result<(), LocalApiError> {
    if req.trusted && !req.risk_acknowledged {
        return Err(LocalApiError::conflict_code(
            "workspace_project_config_trust_ack_required",
            "trusting workspace project configuration requires explicit risk acknowledgement",
        ));
    }
    workspace.project_config_trust = if req.trusted {
        Some(WorkspaceProjectConfigTrust {
            identity_fingerprint: workspace_project_config_trust_fingerprint(
                workspace.absolute_root.as_path(),
            )
            .map_err(|err| LocalApiError::bad_request(err.to_string()))?,
            trusted_at: local_now_rfc3339(),
        })
    } else {
        None
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::trust::workspace_project_config_trust_is_current;

    #[cfg(unix)]
    #[test]
    fn project_config_trust_requires_ack_and_can_be_revoked() {
        let root = std::env::temp_dir().join(format!(
            "chatos-workspace-trust-api-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create workspace");
        let mut workspace = crate::WorkspaceState {
            id: "workspace-test".to_string(),
            absolute_root: root.canonicalize().expect("canonical workspace"),
            alias: "test".to_string(),
            fingerprint: "path-fingerprint".to_string(),
            project_config_trust: None,
        };

        let error = update_workspace_project_config_trust(
            &mut workspace,
            &UpdateWorkspaceProjectConfigTrustRequest {
                trusted: true,
                risk_acknowledged: false,
            },
        )
        .expect_err("trust requires acknowledgement");
        assert!(error.message().contains("risk acknowledgement"));

        update_workspace_project_config_trust(
            &mut workspace,
            &UpdateWorkspaceProjectConfigTrustRequest {
                trusted: true,
                risk_acknowledged: true,
            },
        )
        .expect("trust workspace");
        assert!(workspace_project_config_trust_is_current(&workspace));

        update_workspace_project_config_trust(
            &mut workspace,
            &UpdateWorkspaceProjectConfigTrustRequest {
                trusted: false,
                risk_acknowledged: false,
            },
        )
        .expect("revoke trust");
        assert!(workspace.project_config_trust.is_none());
        let _ = fs::remove_dir_all(root);
    }
}
