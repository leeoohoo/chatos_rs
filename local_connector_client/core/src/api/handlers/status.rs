// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use chatos_sandbox_contract::{PermissionProfileId, SandboxBackendCapability, SandboxBackendKind};
use serde_json::{json, Value};

use crate::sandbox::docker::docker_status_struct;
use crate::sandbox::process::native_process_sandbox_capability;
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
    let sandbox_backend = state.sandbox.default_backend;
    let effective_permission_profile = state.sandbox.effective_default_permission_profile();
    let effective_permission_profile_name =
        state.sandbox.effective_default_permission_profile_name();
    let effective_permission_configuration_result =
        state.sandbox.effective_permission_profile_configuration();
    let permission_configuration_error = effective_permission_configuration_result
        .as_ref()
        .err()
        .cloned();
    let effective_permission_configuration = effective_permission_configuration_result.ok();
    let effective_policy = state.sandbox.effective_policy_defaults();
    let effective_permissions = state.sandbox.effective_permissions(
        Some(effective_permission_profile_name.as_str()),
        &effective_policy,
        Vec::new(),
    );
    let process_capability = if sandbox_backend == SandboxBackendKind::LocalProcess {
        Some(native_process_sandbox_capability().await)
    } else {
        None
    };
    let isolation = sandbox_isolation_status(
        sandbox_backend,
        effective_permission_profile,
        process_capability.as_ref(),
    );
    let connector_running = runtime
        .connector_task
        .lock()
        .await
        .as_ref()
        .map(|handle| !handle.is_finished())
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
        "workspaces": workspaces,
        "sandbox": {
            "enabled": state.sandbox.enabled,
            "backend": sandbox_backend,
            "default_backend": sandbox_backend,
            "isolation": isolation.legacy_isolation,
            "filesystem_isolation": isolation.filesystem_isolation,
            "network_isolation": isolation.network_isolation,
            "process_tree_control": isolation.process_tree_control,
            "isolation_note": isolation.note,
            "default_permission_profile_id": effective_permission_profile,
            "default_permission_profile_name": effective_permission_profile_name,
            "default_permission_profile_provenance": effective_permission_configuration
                .as_ref()
                .map(|effective| effective.default_provenance),
            "permission_configuration_error": permission_configuration_error,
            "custom_permission_profiles": state.sandbox.permission_profiles,
            "effective_custom_permission_profiles": effective_permission_configuration
                .as_ref()
                .map(|effective| &effective.configuration.profiles),
            "managed_permission_profiles": effective_permission_configuration
                .as_ref()
                .map(|effective| &effective.managed_profile_names),
            "default_approval_policy": state.sandbox.default_approval_policy,
            "default_approval_reviewer": state.sandbox.default_approval_reviewer,
            "default_network_requirements": state.sandbox.default_network_requirements,
            "configured_allowed_permission_profiles": state.sandbox.allowed_permission_profiles,
            "allowed_permission_profiles": effective_permission_configuration
                .as_ref()
                .and_then(|effective| effective.configuration.allowed_permission_profiles.as_ref()),
            "permission_profiles": state.sandbox.permission_profile_catalog(),
            "policy_revision": state.sandbox.effective_policy_revision(),
            "effective_policy": effective_policy,
            "effective_permissions": effective_permissions,
            "selected_image_ref": state.sandbox.selected_image_ref,
        },
        "docker": docker_status_struct().await,
    })
}

#[derive(Debug, Clone)]
struct SandboxIsolationStatus {
    legacy_isolation: &'static str,
    filesystem_isolation: bool,
    network_isolation: bool,
    process_tree_control: bool,
    note: String,
}

fn sandbox_isolation_status(
    backend: SandboxBackendKind,
    permission_profile: PermissionProfileId,
    process_capability: Option<&SandboxBackendCapability>,
) -> SandboxIsolationStatus {
    match backend {
        SandboxBackendKind::Docker => SandboxIsolationStatus {
            legacy_isolation: "local_docker",
            filesystem_isolation: true,
            network_isolation: false,
            process_tree_control: true,
            note: "Docker 容器文件系统/进程边界已启用；bridge 网络不提供出站网络隔离".to_string(),
        },
        SandboxBackendKind::LocalProcess => {
            let capability = process_capability;
            if permission_profile == PermissionProfileId::FullAccess {
                return SandboxIsolationStatus {
                    legacy_isolation: "local_process",
                    filesystem_isolation: false,
                    network_isolation: false,
                    process_tree_control: capability
                        .is_some_and(|value| value.process_tree_control),
                    note: "本机进程以完全访问模式运行；仅保留进程树回收，不施加文件系统或网络沙箱"
                        .to_string(),
                };
            }
            SandboxIsolationStatus {
                legacy_isolation: "local_process",
                filesystem_isolation: capability.is_some_and(|value| value.filesystem_isolation),
                network_isolation: capability.is_some_and(|value| value.network_isolation),
                process_tree_control: capability.is_some_and(|value| value.process_tree_control),
                note: capability
                    .map(|value| value.message.clone())
                    .unwrap_or_else(|| "本机进程隔离能力尚未探测".to_string()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_status_does_not_claim_network_isolation() {
        let status = sandbox_isolation_status(
            SandboxBackendKind::Docker,
            PermissionProfileId::WorkspaceWrite,
            None,
        );

        assert_eq!(status.legacy_isolation, "local_docker");
        assert!(status.filesystem_isolation);
        assert!(!status.network_isolation);
        assert!(status.process_tree_control);
        assert!(status.note.contains("bridge"));
    }

    #[test]
    fn local_process_status_reflects_native_capability() {
        let capability = SandboxBackendCapability {
            backend: SandboxBackendKind::LocalProcess,
            status: chatos_sandbox_contract::SandboxBackendReadinessStatus::Ready,
            selectable: true,
            filesystem_isolation: true,
            network_isolation: true,
            process_tree_control: true,
            message: "Seatbelt ready".to_string(),
        };
        let status = sandbox_isolation_status(
            SandboxBackendKind::LocalProcess,
            PermissionProfileId::WorkspaceWrite,
            Some(&capability),
        );

        assert_eq!(status.legacy_isolation, "local_process");
        assert!(status.filesystem_isolation);
        assert!(status.network_isolation);
        assert!(status.process_tree_control);
        assert!(status.note.contains("Seatbelt"));
    }

    #[test]
    fn local_process_full_access_does_not_claim_filesystem_or_network_isolation() {
        let capability = SandboxBackendCapability {
            backend: SandboxBackendKind::LocalProcess,
            status: chatos_sandbox_contract::SandboxBackendReadinessStatus::Ready,
            selectable: true,
            filesystem_isolation: true,
            network_isolation: true,
            process_tree_control: true,
            message: "Seatbelt ready".to_string(),
        };
        let status = sandbox_isolation_status(
            SandboxBackendKind::LocalProcess,
            PermissionProfileId::FullAccess,
            Some(&capability),
        );

        assert!(!status.filesystem_isolation);
        assert!(!status.network_isolation);
        assert!(status.process_tree_control);
        assert!(status.note.contains("完全访问"));
    }
}
