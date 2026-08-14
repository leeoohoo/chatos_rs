// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[cfg(unix)]
pub(super) fn prepare_sandbox_workspace_owner(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::{chown, PermissionsExt};

    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|error| {
            format!(
                "scan sandbox workspace ownership under {} failed: {error}",
                path.display()
            )
        })?;
        if entry.file_type().is_symlink() {
            continue;
        }
        let entry_path = entry.path();
        let chown_error = chown(entry_path, Some(1000), Some(1000)).err();
        if let Some(error) = chown_error.as_ref() {
            if error.kind() != std::io::ErrorKind::PermissionDenied {
                return Err(format!(
                    "set sandbox workspace owner for {} failed: {error}",
                    entry_path.display()
                ));
            }
        }
        let metadata = std::fs::metadata(entry_path).map_err(|error| error.to_string())?;
        let mut permissions = metadata.permissions();
        let mode = if chown_error.is_some() {
            if metadata.is_dir() {
                0o777
            } else {
                0o666
            }
        } else if metadata.is_dir() {
            0o700
        } else {
            let executable = permissions.mode() & 0o111 != 0;
            if executable {
                0o700
            } else {
                0o600
            }
        };
        permissions.set_mode(mode);
        std::fs::set_permissions(entry_path, permissions).map_err(|error| {
            format!(
                "make sandbox workspace {} accessible: {error}",
                entry_path.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn prepare_sandbox_workspace_owner(_path: &Path) -> Result<(), String> {
    Ok(())
}

pub(in crate::service::manager) fn validate_requested_network_policy(
    config: &AppConfig,
    network: &NetworkPolicy,
) -> Result<(), ApiError> {
    let requested = network.mode.trim();
    let configured = configured_network_mode(config);
    if requested_network_mode_is_allowed(requested, configured) {
        return Ok(());
    }
    Err(ApiError::bad_request(format!(
        "sandbox network mode {requested:?} is not allowed for lease requests; omit network.mode to use the configured default"
    )))
}

pub(super) fn configured_network_mode(config: &AppConfig) -> Option<&str> {
    match config.backend {
        ManagerBackendKind::Docker => Some(config.docker_network_mode.trim()),
        ManagerBackendKind::Kata => Some(config.kata_network_mode.trim()),
        ManagerBackendKind::Mock => None,
    }
    .filter(|value| !value.is_empty())
}

pub(super) fn requested_network_mode_is_allowed(requested: &str, configured: Option<&str>) -> bool {
    let requested = requested.trim();
    requested.is_empty()
        || requested.eq_ignore_ascii_case("bridge")
        || configured
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some_and(|value| value.eq_ignore_ascii_case(requested))
}

pub(in crate::service::manager) fn sandbox_manager_effective_policy(
    request: &SandboxLeasePolicyRequest,
) -> EffectiveSandboxPolicy {
    EffectiveSandboxPolicy {
        sandbox_mode: SandboxBackendKind::Docker,
        // The Docker manager currently exposes a writable run workspace. It does not implement
        // read-only file policy or host full-access escalation.
        permission_profile_id: PermissionProfileId::WorkspaceWrite,
        // The cloud Sandbox Manager has no user/AI approval loop in the MCP proxy. Report the
        // actual behavior so Task Runner can fail closed when a task explicitly requires approval.
        approval_policy: ApprovalPolicy::Never,
        approval_reviewer: ApprovalReviewer::User,
        policy_revision: request.policy_revision.clone(),
        additional_writable_roots: Vec::new(),
    }
}

pub(in crate::service::manager) fn sandbox_manager_effective_permissions(
    policy: &EffectiveSandboxPolicy,
    runtime_workspace_roots: Vec<String>,
) -> EffectivePermissionSnapshot {
    let mut permissions = legacy_policy_permission_snapshot(policy, runtime_workspace_roots);
    // Project execution runs inside an isolated container/network namespace and needs outbound
    // access for package managers and source dependencies. Filesystem access remains constrained
    // to the managed workspace by the workspace permission profile.
    permissions.network = NetworkPermissionPolicy::Unrestricted;
    permissions
}
