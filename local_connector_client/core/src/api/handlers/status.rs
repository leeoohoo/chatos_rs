// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::config::default_filesystem_root;
use crate::workspace::trust::workspace_project_config_trust_is_current;
use crate::LocalRuntime;

use super::super::types::LocalApiError;

pub(crate) async fn local_status(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    Ok(Json(status_payload(&runtime).await))
}

pub(crate) async fn status_payload(runtime: &LocalRuntime) -> Value {
    let state = runtime.state.read().await.clone();
    let connector_running = runtime
        .connector_task
        .lock()
        .await
        .as_ref()
        .map(|task| task.is_running())
        .unwrap_or(false);
    let workspaces = state
        .workspaces
        .iter()
        .map(|workspace| {
            let trust_configured = workspace.project_config_trust.is_some();
            let trust_current = workspace_project_config_trust_is_current(workspace);
            json!({
                "id": workspace.id,
                "absolute_root": workspace.absolute_root,
                "alias": workspace.alias,
                "fingerprint": workspace.fingerprint,
                "project_config_trusted": trust_current,
                "project_config_trust_stale": trust_configured && !trust_current,
                "project_config_trusted_at": workspace
                    .project_config_trust
                    .as_ref()
                    .map(|trust| trust.trusted_at.as_str()),
            })
        })
        .collect::<Vec<_>>();
    let default_workspace_id = state
        .workspaces
        .iter()
        .find(|workspace| workspace.absolute_root == default_filesystem_root())
        .map(|workspace| workspace.id.as_str());
    json!({
        "configured": state.auth.is_some(),
        "connector_running": connector_running,
        "developer_mode": state.runtime_settings.developer_mode,
        "browser_full_cdp_access_enabled": state.runtime_settings.browser_full_cdp_access_enabled,
        "developer_cloud_base_url": state.runtime_settings.developer_cloud_base_url,
        "developer_user_service_base_url": state.runtime_settings.developer_user_service_base_url,
        "developer_chatos_web_url": state.runtime_settings.developer_chatos_web_url,
        "cloud_base_url": state.auth.as_ref().map(|auth| auth.cloud_base_url.as_str()),
        "user_service_base_url": state.auth.as_ref().map(|auth| auth.user_service_base_url.as_str()),
        "device_id": state.device_id,
        "device_name": state.auth.as_ref().map(|auth| auth.device_name.as_str()),
        "user": state.auth.as_ref().and_then(|auth| auth.user.clone()),
        "default_workspace_id": default_workspace_id,
        "workspaces": workspaces,
    })
}
