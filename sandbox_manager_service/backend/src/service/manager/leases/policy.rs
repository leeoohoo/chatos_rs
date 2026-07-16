// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[cfg(unix)]
pub(super) fn prepare_sandbox_workspace_owner(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::{chown, PermissionsExt};

    let chown_error = chown(path, Some(1000), Some(1000)).err();
    if let Some(err) = chown_error.as_ref() {
        if err.kind() != std::io::ErrorKind::PermissionDenied {
            return Err(format!(
                "set sandbox workspace owner for {} failed: {err}",
                path.display()
            ));
        }
    }
    let mut permissions = std::fs::metadata(path)
        .map_err(|metadata_err| metadata_err.to_string())?
        .permissions();
    permissions.set_mode(if chown_error.is_some() { 0o777 } else { 0o700 });
    std::fs::set_permissions(path, permissions).map_err(|permissions_err| {
        format!(
            "make sandbox workspace {} accessible{}: {permissions_err}",
            path.display(),
            chown_error
                .map(|err| format!(" after chown failed ({err})"))
                .unwrap_or_default()
        )
    })?;
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn prepare_sandbox_workspace_owner(_path: &Path) -> Result<(), String> {
    Ok(())
}

pub(super) fn validate_requested_network_policy(
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

pub(super) fn sandbox_manager_effective_policy(
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
