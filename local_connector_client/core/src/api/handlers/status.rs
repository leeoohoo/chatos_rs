// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use chatos_sandbox_contract::SandboxBackendKind;
use serde_json::{json, Value};

use crate::sandbox::docker::docker_status_struct;
use crate::LocalRuntime;

use super::super::types::LocalApiError;

pub(crate) async fn local_status(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    Ok(Json(status_payload(&runtime).await))
}

pub(crate) async fn status_payload(runtime: &LocalRuntime) -> Value {
    let state = runtime.state.read().await.clone();
    let sandbox_backend = state.sandbox.default_backend;
    let isolation = sandbox_isolation_status(sandbox_backend);
    let connector_running = runtime
        .connector_task
        .lock()
        .await
        .as_ref()
        .map(|handle| !handle.is_finished())
        .unwrap_or(false);
    json!({
        "configured": state.auth.is_some(),
        "connector_running": connector_running,
        "developer_mode": state.runtime_settings.developer_mode,
        "developer_cloud_base_url": state.runtime_settings.developer_cloud_base_url,
        "developer_user_service_base_url": state.runtime_settings.developer_user_service_base_url,
        "developer_chatos_web_url": state.runtime_settings.developer_chatos_web_url,
        "cloud_base_url": state.auth.as_ref().map(|auth| auth.cloud_base_url.as_str()),
        "user_service_base_url": state.auth.as_ref().map(|auth| auth.user_service_base_url.as_str()),
        "device_id": state.device_id,
        "device_name": state.auth.as_ref().map(|auth| auth.device_name.as_str()),
        "user": state.auth.as_ref().and_then(|auth| auth.user.clone()),
        "workspaces": state.workspaces,
        "sandbox": {
            "enabled": state.sandbox.enabled,
            "backend": sandbox_backend,
            "default_backend": sandbox_backend,
            "isolation": isolation.legacy_isolation,
            "filesystem_isolation": isolation.filesystem_isolation,
            "network_isolation": isolation.network_isolation,
            "process_tree_control": isolation.process_tree_control,
            "isolation_note": isolation.note,
            "default_permission_profile_id": state.sandbox.default_permission_profile_id,
            "default_approval_policy": state.sandbox.default_approval_policy,
            "default_approval_reviewer": state.sandbox.default_approval_reviewer,
            "policy_revision": state.sandbox.policy_revision,
            "effective_policy": state.sandbox.effective_policy_defaults(),
            "selected_image_ref": state.sandbox.selected_image_ref,
        },
        "docker": docker_status_struct().await,
    })
}

#[derive(Debug, Clone, Copy)]
struct SandboxIsolationStatus {
    legacy_isolation: &'static str,
    filesystem_isolation: bool,
    network_isolation: bool,
    process_tree_control: bool,
    note: &'static str,
}

fn sandbox_isolation_status(backend: SandboxBackendKind) -> SandboxIsolationStatus {
    match backend {
        SandboxBackendKind::Docker => SandboxIsolationStatus {
            legacy_isolation: "local_docker",
            filesystem_isolation: true,
            network_isolation: false,
            process_tree_control: true,
            note: "Docker 容器文件系统/进程边界已启用；bridge 网络不提供出站网络隔离",
        },
        SandboxBackendKind::LocalProcess => SandboxIsolationStatus {
            legacy_isolation: "local_process",
            filesystem_isolation: false,
            network_isolation: false,
            process_tree_control: false,
            note: "本机进程隔离仍在开发中，当前不可选择",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_status_does_not_claim_network_isolation() {
        let status = sandbox_isolation_status(SandboxBackendKind::Docker);

        assert_eq!(status.legacy_isolation, "local_docker");
        assert!(status.filesystem_isolation);
        assert!(!status.network_isolation);
        assert!(status.process_tree_control);
        assert!(status.note.contains("bridge"));
    }

    #[test]
    fn local_process_status_is_not_ready_or_isolated_yet() {
        let status = sandbox_isolation_status(SandboxBackendKind::LocalProcess);

        assert_eq!(status.legacy_isolation, "local_process");
        assert!(!status.filesystem_isolation);
        assert!(!status.network_isolation);
        assert!(!status.process_tree_control);
    }
}
