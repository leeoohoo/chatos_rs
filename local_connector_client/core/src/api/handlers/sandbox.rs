// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use chatos_sandbox_contract::{
    parse_codex_permission_profile_toml, ApprovalReviewer, NetworkDomainPermission,
    NetworkPermissionPolicy, NetworkRequirements, PermissionProfileId, SandboxBackendCapability,
    SandboxBackendKind, SandboxBackendReadinessStatus,
};
use serde_json::{json, Value};

use crate::sandbox::lease::shutdown_local_sandboxes;
use crate::sandbox::local_connector_execution_capability;
use crate::sandbox::types::LocalSandboxNetworkAccess;
use crate::{local_now_rfc3339, LocalRuntime};

use super::super::types::{LocalApiError, ToggleSandboxRequest, UpdateSandboxSettingsRequest};
use super::status::status_payload;

pub(crate) async fn local_shutdown_sandboxes(State(runtime): State<LocalRuntime>) -> Json<Value> {
    Json(shutdown_local_sandboxes(&runtime.sandbox_runtime).await)
}

pub(crate) async fn local_toggle_sandbox(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<ToggleSandboxRequest>,
) -> Result<Json<Value>, LocalApiError> {
    if req.enabled {
        ensure_current_sandbox_backend_ready_for_enable(&runtime).await?;
    }
    {
        let mut state = runtime.state.write().await;
        state.sandbox.enabled = req.enabled;
        state.save(runtime.state_path.as_path())?;
    }
    runtime.start_connector_if_configured().await?;
    Ok(Json(status_payload(&runtime).await))
}

async fn ensure_current_sandbox_backend_ready_for_enable(
    runtime: &LocalRuntime,
) -> Result<(), LocalApiError> {
    let backend = {
        let state = runtime.state.read().await;
        state.sandbox.default_backend
    };
    ensure_sandbox_backend_ready(backend).await
}

pub(crate) async fn local_sandbox_capabilities() -> Json<Value> {
    Json(json!({
        "backends": local_sandbox_backend_capabilities().await,
    }))
}

pub(crate) async fn local_sandbox_settings(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    let state = runtime.state.read().await;
    Ok(Json(sandbox_settings_payload(&state.sandbox)))
}

pub(crate) async fn local_update_sandbox_settings(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<UpdateSandboxSettingsRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let req = normalize_sandbox_settings_update(req)?;
    {
        let state = runtime.state.read().await;
        validate_sandbox_settings_update(&req, &state.sandbox)?;
    }
    let next_backend = {
        let state = runtime.state.read().await;
        req.default_backend.unwrap_or(state.sandbox.default_backend)
    };
    if req.default_backend.is_some() || req.enabled == Some(true) {
        ensure_sandbox_backend_ready(next_backend).await?;
    }

    let response = {
        let mut state = runtime.state.write().await;
        let policy_changed = sandbox_policy_fields_changed(&req, &state.sandbox);
        if let Some(enabled) = req.enabled {
            state.sandbox.enabled = enabled;
        }
        if let Some(default_backend) = req.default_backend {
            state.sandbox.default_backend = default_backend;
        }
        if let Some(profiles) = req.permission_profiles {
            state.sandbox.permission_profiles = profiles;
        }
        if let Some(allowed) = req.allowed_permission_profiles {
            state.sandbox.allowed_permission_profiles = Some(allowed);
        }
        if let Some(profile) = req.default_permission_profile_id {
            state.sandbox.default_permission_profile_id = profile;
            state.sandbox.default_permission_profile_name = Some(profile.codex_name().to_string());
        }
        if let Some(profile_name) = req.default_permission_profile_name {
            state.sandbox.default_permission_profile_name = Some(profile_name.clone());
            state.sandbox.default_permission_profile_id = state
                .sandbox
                .resolve_permission_profile(profile_name.as_str(), Vec::new())
                .map_err(LocalApiError::bad_request)?
                .permission_profile_id;
        }
        if let Some(policy) = req.default_approval_policy {
            state.sandbox.default_approval_policy = policy;
        }
        if let Some(reviewer) = req.default_approval_reviewer {
            state.sandbox.default_approval_reviewer = reviewer;
        }
        if let Some(access) = req.default_network_access {
            state.sandbox.default_network_access = Some(access);
        }
        if let Some(network) = req.default_network_requirements {
            state.sandbox.default_network_requirements = network;
        }
        if policy_changed {
            state.sandbox.policy_revision = Some(format!("local-{}", local_now_rfc3339()));
        }
        state.save(runtime.state_path.as_path())?;
        sandbox_settings_payload(&state.sandbox)
    };
    runtime.start_connector_if_configured().await?;
    Ok(Json(response))
}

fn normalize_sandbox_settings_update(
    mut req: UpdateSandboxSettingsRequest,
) -> Result<UpdateSandboxSettingsRequest, LocalApiError> {
    let Some(source) = req.permission_profiles_toml.take() else {
        return Ok(req);
    };
    if source.len() > 1024 * 1024 {
        return Err(LocalApiError::bad_request(
            "permission profile TOML must not exceed 1 MiB",
        ));
    }
    if req.permission_profiles.is_some()
        || req.allowed_permission_profiles.is_some()
        || req.default_permission_profile_name.is_some()
        || req.default_permission_profile_id.is_some()
    {
        return Err(LocalApiError::bad_request(
            "permissionProfilesToml cannot be combined with explicit permission profile fields",
        ));
    }
    let document =
        parse_codex_permission_profile_toml(source.as_str()).map_err(LocalApiError::bad_request)?;
    req.permission_profiles = Some(document.configuration.profiles);
    req.allowed_permission_profiles = document.configuration.allowed_permission_profiles;
    req.default_permission_profile_name = document.default_permissions;
    Ok(req)
}

fn validate_sandbox_settings_update(
    req: &UpdateSandboxSettingsRequest,
    current: &crate::sandbox::types::LocalSandboxState,
) -> Result<(), LocalApiError> {
    if req
        .default_backend
        .is_some_and(|backend| backend != SandboxBackendKind::LocalProcess)
    {
        return Err(LocalApiError::bad_request(
            "the local client only supports the local_process sandbox backend",
        ));
    }
    let prospective = prospective_sandbox_state(req, current);
    let effective = prospective
        .effective_permission_profile_configuration()
        .map_err(LocalApiError::bad_request)?;
    validate_managed_profile_api_immutability(req, current, &effective)?;
    for profile in effective.configuration.profiles.values() {
        if let Some(network) = profile.network.as_ref() {
            validate_network_requirements(network)?;
        }
    }

    if req.allowed_permission_profiles.is_some() {
        let previously_allowed =
            current.permission_profile_name_allowed(PermissionProfileId::FullAccess.codex_name());
        let next_allows_full = prospective
            .permission_profile_name_allowed(PermissionProfileId::FullAccess.codex_name());
        if next_allows_full && !previously_allowed && !req.risk_acknowledged {
            return Err(LocalApiError::conflict_code(
                "sandbox_risk_ack_required",
                "allowing the full-access permission profile requires explicit risk acknowledgement",
            ));
        }
    }
    let explicitly_selected_profile_name =
        req.default_permission_profile_name.clone().or_else(|| {
            req.default_permission_profile_id
                .map(|profile| profile.codex_name().to_string())
        });
    if let Some(profile_name) = explicitly_selected_profile_name.as_deref() {
        if !effective.configuration.profile_allowed(profile_name) {
            return Err(LocalApiError::conflict_code(
                "sandbox_permission_profile_not_allowed",
                format!(
                    "permission profile {profile_name} is not enabled by effective allowed_permission_profiles"
                ),
            ));
        }
        prospective
            .resolve_permission_profile(profile_name, Vec::new())
            .map_err(LocalApiError::bad_request)?;
    }
    let effective_profile_name = effective.default_profile_name;
    let resolved_profile = prospective
        .resolve_permission_profile(effective_profile_name.as_str(), Vec::new())
        .map_err(LocalApiError::bad_request)?;
    if resolved_profile.permission_profile_id == PermissionProfileId::FullAccess
        && current.effective_default_permission_profile() != PermissionProfileId::FullAccess
        && !req.risk_acknowledged
    {
        return Err(LocalApiError::conflict_code(
            "sandbox_risk_ack_required",
            "switching sandbox permission profile to full access requires explicit risk acknowledgement",
        ));
    }
    if req
        .default_approval_reviewer
        .is_some_and(|reviewer| reviewer == ApprovalReviewer::AutoReview)
        && current.default_approval_reviewer != ApprovalReviewer::AutoReview
        && !req.risk_acknowledged
    {
        return Err(LocalApiError::conflict_code(
            "sandbox_risk_ack_required",
            "switching sandbox approval reviewer to auto review requires explicit risk acknowledgement",
        ));
    }
    if let Some(network) = req.default_network_requirements.as_ref() {
        validate_network_requirements(network)?;
    }
    let profile_network = match &resolved_profile.effective_permissions.network {
        NetworkPermissionPolicy::Restricted { requirements }
            if !effective_profile_name.starts_with(':') =>
        {
            Some(requirements)
        }
        _ => None,
    };
    let effective_network_access = prospective.effective_default_network_access();
    let effective_network_requirements = prospective.effective_default_network_requirements();
    let effective_network = profile_network.unwrap_or(&effective_network_requirements);
    let (previous_network_unrestricted, previous_network) =
        current_effective_network_requirements(current);
    if !previous_network_unrestricted
        && effective_network_access == LocalSandboxNetworkAccess::Host
        && !req.risk_acknowledged
    {
        return Err(LocalApiError::conflict_code(
            "sandbox_risk_ack_required",
            "enabling host sandbox network access requires explicit risk acknowledgement",
        ));
    }
    if !previous_network_unrestricted
        && effective_network_access != LocalSandboxNetworkAccess::Host
        && network_risk_increases(effective_network, &previous_network)
        && !req.risk_acknowledged
    {
        return Err(LocalApiError::conflict_code(
            "sandbox_risk_ack_required",
            "enabling or widening sandbox network access requires explicit risk acknowledgement",
        ));
    }
    Ok(())
}

fn validate_managed_profile_api_immutability(
    req: &UpdateSandboxSettingsRequest,
    current: &crate::sandbox::types::LocalSandboxState,
    prospective_effective: &crate::sandbox::permission_layers::EffectivePermissionProfileConfiguration,
) -> Result<(), LocalApiError> {
    if req.permission_profiles.is_none() {
        return Ok(());
    }
    let current_effective = current
        .effective_permission_profile_configuration()
        .map_err(LocalApiError::bad_request)?;
    let mut locked = current_effective.api_locked_profile_names();
    locked.extend(prospective_effective.api_locked_profile_names());
    for profile_name in locked {
        if current_effective.configuration.profiles.get(&profile_name)
            != prospective_effective
                .configuration
                .profiles
                .get(&profile_name)
        {
            return Err(LocalApiError::conflict_code(
                "sandbox_managed_profile_immutable",
                format!(
                    "permission profile {profile_name} is managed directly or inherited by a managed profile and cannot be changed through the API"
                ),
            ));
        }
    }
    Ok(())
}

fn prospective_sandbox_state(
    req: &UpdateSandboxSettingsRequest,
    current: &crate::sandbox::types::LocalSandboxState,
) -> crate::sandbox::types::LocalSandboxState {
    let mut prospective = current.clone();
    if let Some(profiles) = req.permission_profiles.as_ref() {
        prospective.permission_profiles = profiles.clone();
    }
    if let Some(allowed) = req.allowed_permission_profiles.as_ref() {
        prospective.allowed_permission_profiles = Some(allowed.clone());
    }
    if let Some(profile) = req.default_permission_profile_id {
        prospective.default_permission_profile_id = profile;
        prospective.default_permission_profile_name = Some(profile.codex_name().to_string());
    }
    if let Some(profile_name) = req.default_permission_profile_name.as_ref() {
        prospective.default_permission_profile_name = Some(profile_name.clone());
    }
    if let Some(access) = req.default_network_access {
        prospective.default_network_access = Some(access);
    }
    if let Some(network) = req.default_network_requirements.as_ref() {
        prospective.default_network_requirements = network.clone();
    }
    prospective
}

fn current_effective_network_requirements(
    current: &crate::sandbox::types::LocalSandboxState,
) -> (bool, NetworkRequirements) {
    if current.effective_default_network_access() == LocalSandboxNetworkAccess::Host {
        return (true, NetworkRequirements::default());
    }
    let profile_name = current.effective_default_permission_profile_name();
    if !profile_name.starts_with(':') {
        if let Ok(resolved) = current.resolve_permission_profile(profile_name.as_str(), Vec::new())
        {
            if let NetworkPermissionPolicy::Restricted { requirements } =
                resolved.effective_permissions.network
            {
                return (false, requirements);
            }
        }
    }
    (false, current.effective_default_network_requirements())
}

fn validate_network_requirements(network: &NetworkRequirements) -> Result<(), LocalApiError> {
    if network.allow_upstream_proxy == Some(true) {
        return Err(LocalApiError::bad_request(
            "upstream proxy chaining is not supported by the native sandbox yet",
        ));
    }
    if network.enable_socks5_udp == Some(true) {
        return Err(LocalApiError::bad_request(
            "SOCKS5 UDP is not supported by the native sandbox yet",
        ));
    }
    if network.dangerously_allow_non_loopback_proxy == Some(true) {
        return Err(LocalApiError::bad_request(
            "the native sandbox proxy may only bind loopback addresses",
        ));
    }
    for host in network
        .domains
        .as_ref()
        .into_iter()
        .flat_map(|domains| domains.keys())
        .chain(network.allowed_domains.as_deref().unwrap_or_default())
        .chain(network.denied_domains.as_deref().unwrap_or_default())
    {
        if host.trim().is_empty() || host.contains('\0') || host.contains('/') || host.contains('@')
        {
            return Err(LocalApiError::bad_request(
                "network domain rules must contain host patterns only",
            ));
        }
    }
    Ok(())
}

fn network_risk_increases(current: &NetworkRequirements, previous: &NetworkRequirements) -> bool {
    if current.enabled == Some(true) && previous.enabled != Some(true) {
        return true;
    }
    if current.allow_local_binding == Some(true) && previous.allow_local_binding != Some(true) {
        return true;
    }
    let previous_allowed = allowed_network_domains(previous);
    allowed_network_domains(current)
        .iter()
        .any(|host| !previous_allowed.contains(host))
}

fn allowed_network_domains(network: &NetworkRequirements) -> std::collections::BTreeSet<String> {
    let mut allowed = network
        .domains
        .as_ref()
        .into_iter()
        .flat_map(|domains| domains.iter())
        .filter(|(_, permission)| **permission == NetworkDomainPermission::Allow)
        .map(|(host, _)| host.trim().to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    allowed.extend(
        network
            .allowed_domains
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|host| host.trim().to_ascii_lowercase()),
    );
    allowed
}

fn sandbox_policy_fields_changed(
    req: &UpdateSandboxSettingsRequest,
    current: &crate::sandbox::types::LocalSandboxState,
) -> bool {
    req.default_backend
        .is_some_and(|backend| backend != current.default_backend)
        || req
            .default_permission_profile_id
            .is_some_and(|profile| profile != current.default_permission_profile_id)
        || req
            .default_permission_profile_name
            .as_ref()
            .is_some_and(|name| current.default_permission_profile_name.as_ref() != Some(name))
        || req
            .permission_profiles
            .as_ref()
            .is_some_and(|profiles| profiles != &current.permission_profiles)
        || req
            .allowed_permission_profiles
            .as_ref()
            .is_some_and(|allowed| Some(allowed) != current.allowed_permission_profiles.as_ref())
        || req
            .default_approval_policy
            .is_some_and(|policy| policy != current.default_approval_policy)
        || req
            .default_approval_reviewer
            .is_some_and(|reviewer| reviewer != current.default_approval_reviewer)
        || req
            .default_network_access
            .is_some_and(|access| access != current.effective_default_network_access())
        || req
            .default_network_requirements
            .as_ref()
            .is_some_and(|network| network != &current.default_network_requirements)
}

pub(crate) async fn local_sandbox_leases(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    let leases = runtime
        .sandbox_runtime
        .leases
        .read()
        .await
        .values()
        .cloned()
        .collect::<Vec<_>>();
    Ok(Json(json!(leases)))
}

async fn ensure_local_sandbox_enabled(runtime: &LocalRuntime) -> Result<(), LocalApiError> {
    let state = runtime.state.read().await;
    if state.sandbox.enabled {
        Ok(())
    } else {
        Err(LocalApiError::bad_request("local sandbox is disabled"))
    }
}

fn sandbox_settings_payload(sandbox: &crate::sandbox::types::LocalSandboxState) -> Value {
    let effective_configuration_result = sandbox.effective_permission_profile_configuration();
    let permission_configuration_error = effective_configuration_result.as_ref().err().cloned();
    let effective_configuration = effective_configuration_result.ok();
    let effective_policy = sandbox.effective_policy_defaults();
    let effective_profile_name = sandbox.effective_default_permission_profile_name();
    let effective_permissions = sandbox.effective_permissions(
        Some(effective_profile_name.as_str()),
        &effective_policy,
        Vec::new(),
    );
    json!({
        "enabled": sandbox.enabled,
        "default_backend": sandbox.default_backend,
        "default_permission_profile_id": sandbox.effective_default_permission_profile(),
        "default_permission_profile_name": sandbox.effective_default_permission_profile_name(),
        "default_permission_profile_provenance": effective_configuration
            .as_ref()
            .map(|effective| effective.default_provenance),
        "permission_configuration_error": permission_configuration_error,
        "custom_permission_profiles": sandbox.permission_profiles,
        "effective_custom_permission_profiles": effective_configuration
            .as_ref()
            .map(|effective| &effective.configuration.profiles),
        "managed_permission_profiles": effective_configuration
            .as_ref()
            .map(|effective| &effective.managed_profile_names),
        "default_approval_policy": sandbox.default_approval_policy,
        "default_approval_reviewer": sandbox.default_approval_reviewer,
        "default_network_access": sandbox.effective_default_network_access(),
        "default_network_requirements": sandbox.default_network_requirements,
        "configured_allowed_permission_profiles": sandbox.allowed_permission_profiles,
        "allowed_permission_profiles": effective_configuration
            .as_ref()
            .and_then(|effective| effective.configuration.allowed_permission_profiles.as_ref()),
        "permission_profiles": sandbox.permission_profile_catalog(),
        "policy_revision": sandbox.effective_policy_revision(),
        "effective_policy": effective_policy,
        "effective_permissions": effective_permissions,
    })
}

async fn local_sandbox_backend_capabilities() -> Vec<SandboxBackendCapability> {
    let process_capability = local_connector_execution_capability();
    vec![process_capability]
}

async fn ensure_sandbox_backend_ready(backend: SandboxBackendKind) -> Result<(), LocalApiError> {
    if backend != SandboxBackendKind::LocalProcess {
        return Err(LocalApiError::bad_request(
            "the local client only supports the local_process sandbox backend",
        ));
    }
    let capability = local_connector_execution_capability();
    if capability.status == SandboxBackendReadinessStatus::Ready {
        Ok(())
    } else {
        Err(LocalApiError::conflict_code(
            "sandbox_backend_not_ready",
            capability.message,
        ))
    }
}

#[cfg(test)]
mod tests;
