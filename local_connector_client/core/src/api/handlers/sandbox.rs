// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use chatos_sandbox_contract::{
    parse_codex_permission_profile_toml, ApprovalReviewer, NetworkDomainPermission,
    NetworkPermissionPolicy, NetworkRequirements, PermissionProfileId, SandboxBackendCapability,
    SandboxBackendKind, SandboxBackendReadinessStatus,
};
use chatos_sandbox_image_mcp::SandboxImageBackend;
use serde_json::{json, Value};

use crate::config::normalize_optional;
use crate::sandbox::docker::{docker_status, docker_status_struct, ensure_docker_running};
use crate::sandbox::images::{
    delete_local_sandbox_image, local_sandbox_image_catalog, reinitialize_local_sandbox_image,
    start_local_sandbox_image_job,
};
use crate::sandbox::process::native_process_sandbox_capability;
use crate::{local_now_rfc3339, LocalRuntime};

use super::super::types::{
    InitializeImageRequest, LocalApiError, ToggleSandboxRequest, UpdateSandboxSettingsRequest,
};
use super::status::status_payload;

pub(crate) async fn local_docker_status() -> Json<Value> {
    Json(docker_status().await)
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
    let effective_backend = req.default_backend.unwrap_or(current.default_backend);
    if !effective_profile_name.starts_with(':')
        && effective_backend != SandboxBackendKind::LocalProcess
    {
        return Err(LocalApiError::conflict_code(
            "sandbox_custom_profile_requires_native_backend",
            "custom permission profiles require the native local-process sandbox backend",
        ));
    }
    let profile_network = match &resolved_profile.effective_permissions.network {
        NetworkPermissionPolicy::Restricted { requirements }
            if !effective_profile_name.starts_with(':') =>
        {
            Some(requirements)
        }
        _ => None,
    };
    let effective_network = profile_network.unwrap_or_else(|| {
        req.default_network_requirements
            .as_ref()
            .unwrap_or(&current.default_network_requirements)
    });
    let (previous_network_unrestricted, previous_network) =
        current_effective_network_requirements(current);
    if !previous_network_unrestricted
        && network_risk_increases(effective_network, &previous_network)
        && !req.risk_acknowledged
    {
        return Err(LocalApiError::conflict_code(
            "sandbox_risk_ack_required",
            "enabling or widening sandbox network access requires explicit risk acknowledgement",
        ));
    }
    if effective_network.enabled == Some(true)
        && effective_backend != SandboxBackendKind::LocalProcess
    {
        return Err(LocalApiError::conflict_code(
            "sandbox_network_proxy_requires_native_backend",
            "restricted domain networking requires the native local-process sandbox backend",
        ));
    }
    if effective_network.enabled == Some(true)
        && resolved_profile.permission_profile_id == PermissionProfileId::FullAccess
    {
        return Err(LocalApiError::conflict_code(
            "sandbox_network_proxy_full_access_conflict",
            "full-access permission profiles have unrestricted networking; choose read-only or workspace-write before enabling restricted networking",
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
    prospective
}

fn current_effective_network_requirements(
    current: &crate::sandbox::types::LocalSandboxState,
) -> (bool, NetworkRequirements) {
    if current.effective_default_permission_profile() == PermissionProfileId::FullAccess {
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
    (false, current.default_network_requirements.clone())
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
            .default_network_requirements
            .as_ref()
            .is_some_and(|network| network != &current.default_network_requirements)
}

pub(crate) async fn local_sandbox_images(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    Ok(Json(local_sandbox_image_catalog(&runtime).await))
}

pub(crate) async fn local_sandbox_image_jobs(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    let jobs = runtime.sandbox_runtime.jobs.read().await.clone();
    Ok(Json(json!(jobs)))
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

pub(crate) async fn local_initialize_sandbox_image(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<InitializeImageRequest>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    ensure_docker_running()
        .await
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    let job = start_local_sandbox_image_job(
        &runtime,
        req.features,
        normalize_optional(req.custom_build_script.as_deref()),
        None,
        None,
    )
    .await
    .map_err(LocalApiError::bad_request)?;
    Ok(Json(json!(job)))
}

pub(crate) async fn local_delete_sandbox_image(
    State(runtime): State<LocalRuntime>,
    Path(image_id): Path<String>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    ensure_docker_running()
        .await
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    delete_local_sandbox_image(&runtime, image_id.as_str())
        .await
        .map(Json)
        .map_err(|err| {
            if err.contains("in use by an active lease") {
                LocalApiError::conflict(err)
            } else {
                LocalApiError::bad_request(err)
            }
        })
}

pub(crate) async fn local_reinitialize_sandbox_image(
    State(runtime): State<LocalRuntime>,
    Path(image_id): Path<String>,
) -> Result<Json<Value>, LocalApiError> {
    ensure_local_sandbox_enabled(&runtime).await?;
    ensure_docker_running()
        .await
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    reinitialize_local_sandbox_image(&runtime, image_id.as_str())
        .await
        .map(|job| Json(json!(job)))
        .map_err(LocalApiError::bad_request)
}

pub(crate) async fn local_sandbox_image_mcp(
    State(runtime): State<LocalRuntime>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    let backend = LocalSandboxImageMcpBackend { runtime };
    Json(chatos_sandbox_image_mcp::handle_jsonrpc(&backend, payload).await)
}

struct LocalSandboxImageMcpBackend {
    runtime: LocalRuntime,
}

#[async_trait::async_trait]
impl SandboxImageBackend for LocalSandboxImageMcpBackend {
    async fn image_catalog(&self) -> Result<Value, String> {
        ensure_local_sandbox_enabled(&self.runtime)
            .await
            .map_err(|err| err.message().to_string())?;
        Ok(local_sandbox_image_catalog(&self.runtime).await)
    }

    async fn image_jobs(&self) -> Result<Value, String> {
        ensure_local_sandbox_enabled(&self.runtime)
            .await
            .map_err(|err| err.message().to_string())?;
        let jobs = self.runtime.sandbox_runtime.jobs.read().await.clone();
        Ok(json!(jobs))
    }

    async fn initialize_image(
        &self,
        features: Vec<String>,
        custom_build_script: Option<String>,
    ) -> Result<Value, String> {
        ensure_local_sandbox_enabled(&self.runtime)
            .await
            .map_err(|err| err.message().to_string())?;
        ensure_docker_running()
            .await
            .map_err(|err| err.to_string())?;
        let job =
            start_local_sandbox_image_job(&self.runtime, features, custom_build_script, None, None)
                .await
                .map_err(|err| err.to_string())?;
        Ok(json!(job))
    }
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
        "default_network_requirements": sandbox.default_network_requirements,
        "configured_allowed_permission_profiles": sandbox.allowed_permission_profiles,
        "allowed_permission_profiles": effective_configuration
            .as_ref()
            .and_then(|effective| effective.configuration.allowed_permission_profiles.as_ref()),
        "permission_profiles": sandbox.permission_profile_catalog(),
        "policy_revision": sandbox.effective_policy_revision(),
        "selected_image_ref": sandbox.selected_image_ref.clone(),
        "effective_policy": effective_policy,
        "effective_permissions": effective_permissions,
    })
}

async fn local_sandbox_backend_capabilities() -> Vec<SandboxBackendCapability> {
    let docker = serde_json::to_value(docker_status_struct().await).unwrap_or_else(|_| json!({}));
    let docker_capability = docker_backend_capability_from_status(&docker);
    let process_capability = native_process_sandbox_capability().await;

    vec![docker_capability, process_capability]
}

async fn ensure_sandbox_backend_ready(backend: SandboxBackendKind) -> Result<(), LocalApiError> {
    match backend {
        SandboxBackendKind::Docker => ensure_docker_running()
            .await
            .map_err(|err| LocalApiError::bad_request(err.to_string())),
        SandboxBackendKind::LocalProcess => {
            let capability = native_process_sandbox_capability().await;
            if capability.status == SandboxBackendReadinessStatus::Ready {
                Ok(())
            } else {
                Err(LocalApiError::conflict_code(
                    "sandbox_backend_not_ready",
                    capability.message,
                ))
            }
        }
    }
}

fn docker_backend_capability_from_status(docker: &Value) -> SandboxBackendCapability {
    let docker_installed = docker
        .get("installed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let docker_running = docker
        .get("running")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let docker_detail = docker
        .get("error")
        .and_then(Value::as_str)
        .or_else(|| docker.get("version").and_then(Value::as_str))
        .unwrap_or("Docker is required for the current local sandbox backend");
    let docker_status = if docker_installed && docker_running {
        SandboxBackendReadinessStatus::Ready
    } else {
        SandboxBackendReadinessStatus::SetupRequired
    };
    let docker_message = if docker_installed && docker_running {
        "Docker is installed and running; bridge networking does not provide outbound network isolation".to_string()
    } else if docker_installed {
        format!("Docker is installed but not running: {docker_detail}")
    } else {
        docker_detail.to_string()
    };

    SandboxBackendCapability {
        backend: SandboxBackendKind::Docker,
        status: docker_status,
        selectable: true,
        filesystem_isolation: true,
        network_isolation: false,
        process_tree_control: true,
        message: docker_message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_sandbox_contract::{
        AdditionalFileSystemPermissions, ApprovalPolicy, CustomPermissionProfile,
        FileSystemAccessMode, FileSystemPath, FileSystemSandboxEntry, FileSystemSpecialPath,
        NetworkProxyMode,
    };

    fn update_request() -> UpdateSandboxSettingsRequest {
        UpdateSandboxSettingsRequest {
            enabled: None,
            default_backend: None,
            default_permission_profile_id: None,
            default_permission_profile_name: None,
            permission_profiles: None,
            permission_profiles_toml: None,
            default_approval_policy: None,
            default_approval_reviewer: None,
            default_network_requirements: None,
            allowed_permission_profiles: None,
            risk_acknowledged: false,
        }
    }

    #[test]
    fn sandbox_settings_rejects_elevated_permission_without_risk_ack() {
        let mut req = update_request();
        req.default_permission_profile_id = Some(PermissionProfileId::FullAccess);
        let state = crate::sandbox::types::LocalSandboxState::default();

        let err = validate_sandbox_settings_update(&req, &state)
            .expect_err("full access requires acknowledgement");

        assert_eq!(err.message(), "switching sandbox permission profile to full access requires explicit risk acknowledgement");
    }

    #[test]
    fn sandbox_settings_treats_never_as_fail_closed_but_auto_review_as_elevated() {
        let state = crate::sandbox::types::LocalSandboxState::default();

        let mut never = update_request();
        never.default_approval_policy = Some(ApprovalPolicy::Never);
        validate_sandbox_settings_update(&never, &state)
            .expect("never denies escalation instead of auto-approving it");

        let mut auto = update_request();
        auto.default_approval_reviewer = Some(ApprovalReviewer::AutoReview);
        assert!(validate_sandbox_settings_update(&auto, &state).is_err());
    }

    #[test]
    fn restricted_network_requires_native_backend_and_risk_acknowledgement() {
        let state = crate::sandbox::types::LocalSandboxState {
            default_backend: SandboxBackendKind::LocalProcess,
            ..Default::default()
        };
        let mut req = update_request();
        req.default_network_requirements = Some(NetworkRequirements {
            enabled: Some(true),
            domains: Some(std::collections::BTreeMap::from([(
                "api.openai.com".to_string(),
                NetworkDomainPermission::Allow,
            )])),
            ..Default::default()
        });
        assert!(validate_sandbox_settings_update(&req, &state).is_err());

        req.risk_acknowledged = true;
        validate_sandbox_settings_update(&req, &state)
            .expect("native proxy network with acknowledgement");

        req.default_backend = Some(SandboxBackendKind::Docker);
        let err = validate_sandbox_settings_update(&req, &state)
            .expect_err("Docker cannot enforce restricted egress");
        assert_eq!(
            err.message(),
            "restricted domain networking requires the native local-process sandbox backend"
        );
    }

    #[test]
    fn sandbox_settings_allows_elevated_update_with_risk_ack() {
        let mut req = update_request();
        req.default_permission_profile_id = Some(PermissionProfileId::FullAccess);
        req.default_approval_policy = Some(ApprovalPolicy::Never);
        req.default_approval_reviewer = Some(ApprovalReviewer::AutoReview);
        req.risk_acknowledged = true;
        let state = crate::sandbox::types::LocalSandboxState::default();

        validate_sandbox_settings_update(&req, &state).expect("risk acknowledged");
    }

    #[test]
    fn permission_profile_allowlist_is_complete_and_enforced() {
        let state = crate::sandbox::types::LocalSandboxState::default();
        let mut req = update_request();
        req.default_permission_profile_id = Some(PermissionProfileId::ReadOnly);
        req.allowed_permission_profiles = Some(std::collections::BTreeMap::from([
            (":read-only".to_string(), true),
            (":workspace".to_string(), false),
        ]));
        validate_sandbox_settings_update(&req, &state).expect("read-only allowed");

        req.default_permission_profile_id = Some(PermissionProfileId::WorkspaceWrite);
        let err = validate_sandbox_settings_update(&req, &state)
            .expect_err("workspace profile must be rejected");
        assert!(err.message().contains("not enabled"));

        req.allowed_permission_profiles = Some(std::collections::BTreeMap::from([(
            "custom-profile".to_string(),
            true,
        )]));
        assert!(validate_sandbox_settings_update(&req, &state).is_err());
    }

    #[test]
    fn managed_allowlist_cannot_be_bypassed_by_api_selection() {
        let mut state = crate::sandbox::types::LocalSandboxState::default();
        state.runtime_permission_profile_layers =
            crate::sandbox::permission_layers::RuntimePermissionProfileLayers::for_tests(
                None,
                None,
                Some(
                    parse_codex_permission_profile_toml(
                        r#"
default_permissions = ":read-only"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
"#,
                    )
                    .expect("parse managed requirements"),
                ),
            );
        let mut req = update_request();
        req.default_permission_profile_id = Some(PermissionProfileId::FullAccess);
        req.risk_acknowledged = true;

        let error = validate_sandbox_settings_update(&req, &state)
            .expect_err("managed allowlist must reject full access");

        assert!(error.message().contains("not enabled"));
    }

    #[test]
    fn api_cannot_shadow_a_managed_custom_profile() {
        let mut state = crate::sandbox::types::LocalSandboxState::default();
        state.runtime_permission_profile_layers =
            crate::sandbox::permission_layers::RuntimePermissionProfileLayers::for_tests(
                None,
                None,
                Some(
                    parse_codex_permission_profile_toml(
                        r#"
default_permissions = "acme-review"

[allowed_permission_profiles]
acme-review = true

[permissions.acme-review]
extends = ":read-only"
"#,
                    )
                    .expect("parse managed profile"),
                ),
            );
        let mut req = update_request();
        req.permission_profiles = Some(std::collections::BTreeMap::from([(
            "acme-review".to_string(),
            CustomPermissionProfile {
                extends: Some(":workspace".to_string()),
                ..Default::default()
            },
        )]));

        let error = validate_sandbox_settings_update(&req, &state)
            .expect_err("managed profile collision must fail closed");

        assert!(error.message().contains("conflicts"));
    }

    #[test]
    fn api_cannot_widen_a_parent_inherited_by_managed_profile() {
        let mut state = crate::sandbox::types::LocalSandboxState {
            default_backend: SandboxBackendKind::LocalProcess,
            permission_profiles: std::collections::BTreeMap::from([(
                "shared-base".to_string(),
                CustomPermissionProfile {
                    extends: Some(":read-only".to_string()),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        state.runtime_permission_profile_layers =
            crate::sandbox::permission_layers::RuntimePermissionProfileLayers::for_tests(
                None,
                None,
                Some(
                    parse_codex_permission_profile_toml(
                        r#"
default_permissions = "acme-managed"

[allowed_permission_profiles]
acme-managed = true

[permissions.acme-managed]
extends = "shared-base"
"#,
                    )
                    .expect("parse managed inherited profile"),
                ),
            );
        let mut req = update_request();
        req.permission_profiles = Some(std::collections::BTreeMap::from([(
            "shared-base".to_string(),
            CustomPermissionProfile {
                extends: Some(":workspace".to_string()),
                ..Default::default()
            },
        )]));

        let error = validate_sandbox_settings_update(&req, &state)
            .expect_err("managed profile ancestry must be immutable through API");

        assert!(error.message().contains("inherited by a managed profile"));
    }

    #[test]
    fn custom_permission_profile_requires_native_backend_and_can_be_selected() {
        let state = crate::sandbox::types::LocalSandboxState {
            default_backend: SandboxBackendKind::LocalProcess,
            ..Default::default()
        };
        let mut req = update_request();
        req.permission_profiles = Some(std::collections::BTreeMap::from([(
            "project-edit".to_string(),
            CustomPermissionProfile {
                description: Some("Project edit".to_string()),
                extends: None,
                file_system: Some(AdditionalFileSystemPermissions {
                    entries: Some(vec![
                        FileSystemSandboxEntry {
                            access: FileSystemAccessMode::Read,
                            path: FileSystemPath::Special {
                                value: FileSystemSpecialPath::Minimal,
                            },
                        },
                        FileSystemSandboxEntry {
                            access: FileSystemAccessMode::Write,
                            path: FileSystemPath::Special {
                                value: FileSystemSpecialPath::ProjectRoots { subpath: None },
                            },
                        },
                    ]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )]));
        req.default_permission_profile_name = Some("project-edit".to_string());
        validate_sandbox_settings_update(&req, &state).expect("valid native custom profile");

        req.default_backend = Some(SandboxBackendKind::Docker);
        let err = validate_sandbox_settings_update(&req, &state)
            .expect_err("Docker cannot execute custom filesystem profiles");
        assert!(err.message().contains("native local-process"));
    }

    #[test]
    fn codex_permission_toml_import_normalizes_into_persisted_profile_fields() {
        let state = crate::sandbox::types::LocalSandboxState {
            default_backend: SandboxBackendKind::LocalProcess,
            ..Default::default()
        };
        let mut req = update_request();
        req.permission_profiles_toml = Some(
            r#"
default_permissions = "minimal-project"

[allowed_permission_profiles]
"minimal-project" = true

[permissions.minimal-project.filesystem]
glob_scan_max_depth = 3
":minimal" = "read"

[permissions.minimal-project.filesystem.":workspace_roots"]
"." = "write"
"**/*.env" = "deny"
"#
            .to_string(),
        );
        let req = normalize_sandbox_settings_update(req).expect("normalize TOML import");
        assert_eq!(
            req.default_permission_profile_name.as_deref(),
            Some("minimal-project")
        );
        assert!(req
            .permission_profiles
            .as_ref()
            .is_some_and(|profiles| profiles.contains_key("minimal-project")));
        validate_sandbox_settings_update(&req, &state).expect("validate imported TOML profile");
    }

    #[test]
    fn codex_permission_toml_import_rejects_ambiguous_explicit_fields() {
        let mut req = update_request();
        req.permission_profiles_toml = Some("default_permissions = \":read-only\"".to_string());
        req.default_permission_profile_id = Some(PermissionProfileId::ReadOnly);
        let error = normalize_sandbox_settings_update(req)
            .expect_err("ambiguous profile sources must fail closed");
        assert!(error.message().contains("cannot be combined"));
    }

    #[test]
    fn custom_permission_profile_cycles_fail_closed_at_the_api_boundary() {
        let state = crate::sandbox::types::LocalSandboxState::default();
        let mut req = update_request();
        req.permission_profiles = Some(std::collections::BTreeMap::from([
            (
                "a".to_string(),
                CustomPermissionProfile {
                    extends: Some("b".to_string()),
                    ..Default::default()
                },
            ),
            (
                "b".to_string(),
                CustomPermissionProfile {
                    extends: Some("a".to_string()),
                    ..Default::default()
                },
            ),
        ]));

        let err = validate_sandbox_settings_update(&req, &state)
            .expect_err("inheritance cycle must be rejected");
        assert!(err.message().contains("cycle"));
    }

    #[test]
    fn custom_limited_network_profile_requires_risk_acknowledgement() {
        let state = crate::sandbox::types::LocalSandboxState {
            default_backend: SandboxBackendKind::LocalProcess,
            ..Default::default()
        };
        let mut req = update_request();
        req.permission_profiles = Some(std::collections::BTreeMap::from([(
            "api-read".to_string(),
            CustomPermissionProfile {
                extends: Some(":read-only".to_string()),
                network: Some(NetworkRequirements {
                    enabled: Some(true),
                    mode: Some(NetworkProxyMode::Limited),
                    domains: Some(std::collections::BTreeMap::from([(
                        "api.openai.com".to_string(),
                        NetworkDomainPermission::Allow,
                    )])),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )]));
        req.default_permission_profile_name = Some("api-read".to_string());

        let err = validate_sandbox_settings_update(&req, &state)
            .expect_err("enabling custom profile network requires acknowledgement");
        assert!(err.message().contains("network access"));

        req.risk_acknowledged = true;
        validate_sandbox_settings_update(&req, &state)
            .expect("acknowledged limited custom network profile");
    }

    #[test]
    fn sandbox_policy_revision_changes_only_for_policy_fields() {
        let state = crate::sandbox::types::LocalSandboxState::default();

        let mut enabled_only = update_request();
        enabled_only.enabled = Some(true);
        assert!(!sandbox_policy_fields_changed(&enabled_only, &state));

        let mut same_backend = update_request();
        same_backend.default_backend = Some(state.default_backend);
        assert!(!sandbox_policy_fields_changed(&same_backend, &state));

        let mut profile = update_request();
        profile.default_permission_profile_id = Some(PermissionProfileId::ReadOnly);
        assert!(sandbox_policy_fields_changed(&profile, &state));

        let mut reviewer = update_request();
        reviewer.default_approval_reviewer = Some(ApprovalReviewer::AutoReview);
        assert!(sandbox_policy_fields_changed(&reviewer, &state));
    }

    #[test]
    fn docker_backend_capability_does_not_claim_network_isolation_for_bridge_mode() {
        let capability = docker_backend_capability_from_status(&json!({
            "installed": true,
            "running": true,
            "version": "Docker 27"
        }));

        assert_eq!(capability.backend, SandboxBackendKind::Docker);
        assert_eq!(capability.status, SandboxBackendReadinessStatus::Ready);
        assert!(capability.selectable);
        assert!(capability.filesystem_isolation);
        assert!(!capability.network_isolation);
        assert!(capability.process_tree_control);
        assert!(capability.message.contains("bridge networking"));
    }
}
