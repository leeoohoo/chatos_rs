// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use chatos_sandbox_contract::{
    ApprovalPolicy, ApprovalReviewer, PermissionProfileId, SandboxBackendCapability,
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
    if backend == SandboxBackendKind::LocalProcess {
        return Err(LocalApiError::conflict_code(
            "sandbox_backend_not_ready",
            "local process sandbox is not ready on this device",
        ));
    }
    ensure_docker_running()
        .await
        .map_err(|err| LocalApiError::bad_request(err.to_string()))
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
    {
        let state = runtime.state.read().await;
        validate_sandbox_settings_update(&req, &state.sandbox)?;
    }
    let next_backend = {
        let state = runtime.state.read().await;
        req.default_backend.unwrap_or(state.sandbox.default_backend)
    };
    if next_backend == SandboxBackendKind::LocalProcess {
        return Err(LocalApiError::conflict_code(
            "sandbox_backend_not_ready",
            "local process sandbox is not ready on this device",
        ));
    }
    if req.enabled == Some(true) {
        ensure_docker_running()
            .await
            .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
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
        if let Some(profile) = req.default_permission_profile_id {
            state.sandbox.default_permission_profile_id = profile;
        }
        if let Some(policy) = req.default_approval_policy {
            state.sandbox.default_approval_policy = policy;
        }
        if let Some(reviewer) = req.default_approval_reviewer {
            state.sandbox.default_approval_reviewer = reviewer;
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

fn validate_sandbox_settings_update(
    req: &UpdateSandboxSettingsRequest,
    current: &crate::sandbox::types::LocalSandboxState,
) -> Result<(), LocalApiError> {
    if req
        .default_permission_profile_id
        .is_some_and(|profile| profile == PermissionProfileId::FullAccess)
        && current.default_permission_profile_id != PermissionProfileId::FullAccess
        && !req.risk_acknowledged
    {
        return Err(LocalApiError::conflict_code(
            "sandbox_risk_ack_required",
            "switching sandbox permission profile to full access requires explicit risk acknowledgement",
        ));
    }
    if req
        .default_approval_policy
        .is_some_and(|policy| policy == ApprovalPolicy::Never)
        && current.default_approval_policy != ApprovalPolicy::Never
        && !req.risk_acknowledged
    {
        return Err(LocalApiError::conflict_code(
            "sandbox_risk_ack_required",
            "switching sandbox approval policy to never requires explicit risk acknowledgement",
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
    Ok(())
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
            .default_approval_policy
            .is_some_and(|policy| policy != current.default_approval_policy)
        || req
            .default_approval_reviewer
            .is_some_and(|reviewer| reviewer != current.default_approval_reviewer)
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
    json!({
        "enabled": sandbox.enabled,
        "default_backend": sandbox.default_backend,
        "default_permission_profile_id": sandbox.default_permission_profile_id,
        "default_approval_policy": sandbox.default_approval_policy,
        "default_approval_reviewer": sandbox.default_approval_reviewer,
        "policy_revision": sandbox.policy_revision.clone(),
        "selected_image_ref": sandbox.selected_image_ref.clone(),
        "effective_policy": sandbox.effective_policy_defaults(),
    })
}

async fn local_sandbox_backend_capabilities() -> Vec<SandboxBackendCapability> {
    let docker = serde_json::to_value(docker_status_struct().await).unwrap_or_else(|_| json!({}));
    let docker_capability = docker_backend_capability_from_status(&docker);

    vec![
        docker_capability,
        SandboxBackendCapability {
            backend: SandboxBackendKind::LocalProcess,
            status: SandboxBackendReadinessStatus::UnderDevelopment,
            selectable: false,
            filesystem_isolation: false,
            network_isolation: false,
            process_tree_control: false,
            message: "Local process sandbox isolation is still under development".to_string(),
        },
    ]
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

    fn update_request() -> UpdateSandboxSettingsRequest {
        UpdateSandboxSettingsRequest {
            enabled: None,
            default_backend: None,
            default_permission_profile_id: None,
            default_approval_policy: None,
            default_approval_reviewer: None,
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
    fn sandbox_settings_rejects_elevated_approval_without_risk_ack() {
        let state = crate::sandbox::types::LocalSandboxState::default();

        let mut never = update_request();
        never.default_approval_policy = Some(ApprovalPolicy::Never);
        assert!(validate_sandbox_settings_update(&never, &state).is_err());

        let mut auto = update_request();
        auto.default_approval_reviewer = Some(ApprovalReviewer::AutoReview);
        assert!(validate_sandbox_settings_update(&auto, &state).is_err());
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
    fn sandbox_policy_revision_changes_only_for_policy_fields() {
        let state = crate::sandbox::types::LocalSandboxState::default();

        let mut enabled_only = update_request();
        enabled_only.enabled = Some(true);
        assert!(!sandbox_policy_fields_changed(&enabled_only, &state));

        let mut same_backend = update_request();
        same_backend.default_backend = Some(SandboxBackendKind::Docker);
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
