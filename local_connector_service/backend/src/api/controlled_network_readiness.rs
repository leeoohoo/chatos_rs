// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;

use chatos_sandbox_contract::{
    merge_codex_permission_profile_document_layers, parse_managed_requirements_toml,
    CodexPermissionProfileDocument,
};

use crate::controlled_network::{
    allowed_hosts_from_managed_requirements, ControlledNetworkPolicyRequest,
};
use crate::models::CurrentUser;
use crate::state::AppState;

use super::ApiError;

#[derive(Debug, Serialize)]
pub(super) struct ControlledNetworkReadinessResponse {
    available: bool,
    state: &'static str,
    permission_profile: Option<String>,
    allowed_host_count: usize,
}

#[derive(Debug, Deserialize)]
pub(super) struct ControlledNetworkPolicyQuery {
    workspace_id: String,
    permission_profile: Option<String>,
}

struct ControlledNetworkPolicySource {
    permission_profile: String,
    allowed_hosts: Vec<String>,
}

struct ControlledNetworkPolicyResolution {
    state: &'static str,
    source: Option<ControlledNetworkPolicySource>,
}

pub(super) async fn controlled_network_readiness(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
) -> Result<Json<ControlledNetworkReadinessResponse>, ApiError> {
    let resolution = resolve_controlled_network_policy_source(
        &state,
        &user,
        device_id.as_str(),
        &ControlledNetworkPolicyRequest::default(),
    )
    .await?;
    let response = match resolution.source {
        Some(source) => ControlledNetworkReadinessResponse {
            available: true,
            state: "ready",
            permission_profile: Some(source.permission_profile),
            allowed_host_count: source.allowed_hosts.len(),
        },
        None => ControlledNetworkReadinessResponse {
            available: false,
            state: resolution.state,
            permission_profile: None,
            allowed_host_count: 0,
        },
    };
    Ok(Json(response))
}

pub(super) async fn issue_controlled_network_policy(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<ControlledNetworkPolicyQuery>,
) -> Result<Json<crate::controlled_network::ControlledNetworkPolicyEnvelope>, ApiError> {
    let device = super::load_owned_device(&state, &user, device_id.as_str(), true).await?;
    let windows_user_sid = device.windows_user_sid.ok_or_else(|| {
        ApiError::conflict(
            "controlled_network_not_ready",
            "Windows user SID is not registered for this device",
        )
    })?;
    let workspace = state
        .store
        .get_workspace(query.workspace_id.trim())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Local Connector workspace not found"))?;
    if workspace.owner_user_id != user.effective_owner_user_id() {
        return Err(ApiError::forbidden(
            "Local Connector workspace does not belong to current user",
        ));
    }
    if workspace.device_id != device_id {
        return Err(ApiError::bad_request(
            "Local Connector workspace is not attached to this device",
        ));
    }
    let request = ControlledNetworkPolicyRequest {
        permission_profile: query.permission_profile,
    };
    let resolution =
        resolve_controlled_network_policy_source(&state, &user, device_id.as_str(), &request)
            .await?;
    let source = resolution.source.ok_or_else(|| {
        ApiError::service_unavailable(format!(
            "Controlled Network policy is not ready: {}",
            resolution.state
        ))
    })?;
    let signer = state.controlled_network_signer.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Controlled Network signing is not configured")
    })?;
    let policy = signer
        .issue(
            user.effective_owner_user_id(),
            device_id.as_str(),
            query.workspace_id.trim(),
            windows_user_sid.as_str(),
            source.allowed_hosts,
            vec![80, 443],
            Utc::now(),
        )
        .map_err(ApiError::internal)?;
    Ok(Json(policy))
}

async fn resolve_controlled_network_policy_source(
    state: &AppState,
    user: &CurrentUser,
    device_id: &str,
    request: &ControlledNetworkPolicyRequest,
) -> Result<ControlledNetworkPolicyResolution, ApiError> {
    if state.controlled_network_signer.is_none() {
        return Ok(unavailable("signer_not_configured"));
    }
    let device = state
        .store
        .get_device(device_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Local Connector device not found"))?;
    if device.owner_user_id != user.effective_owner_user_id() {
        return Err(ApiError::forbidden(
            "Local Connector device does not belong to current user",
        ));
    }
    if device.windows_user_sid.is_none() {
        return Ok(unavailable("windows_sid_not_registered"));
    }
    let layers = state
        .store
        .applicable_managed_requirements_layers(user.effective_owner_user_id(), user.role.as_str())
        .await
        .map_err(ApiError::internal)?;
    let mut requirements = layers
        .into_iter()
        .map(|layer| layer.policy.requirements_toml)
        .collect::<Vec<_>>();
    if requirements.is_empty() {
        if let Some(fallback) = state
            .managed_requirements_signer
            .as_ref()
            .and_then(|value| value.fallback_requirements_toml())
        {
            requirements.push(fallback.to_string());
        }
    }
    if requirements.is_empty() {
        return Ok(unavailable("managed_policy_not_configured"));
    }
    let mut document = CodexPermissionProfileDocument::default();
    for requirements_toml in requirements {
        let layer = match parse_managed_requirements_toml(requirements_toml.as_str()) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(device_id, error = %error, "managed Controlled network policy is invalid");
                return Ok(unavailable("managed_policy_invalid"));
            }
        };
        document = merge_codex_permission_profile_document_layers(document, layer);
    }
    if let Err(error) = document.configuration.validate() {
        tracing::warn!(device_id, error = %error, "merged Controlled network policy is invalid");
        return Ok(unavailable("managed_policy_invalid"));
    }
    let permission_profile = document.default_permissions.clone();
    let allowed_hosts = match allowed_hosts_from_managed_requirements(&document, request) {
        Ok(Some(value)) => value,
        Ok(None) => return Ok(unavailable("managed_allowlist_not_configured")),
        Err(error) => {
            tracing::warn!(
                device_id,
                error = %error,
                "managed network policy cannot be compiled for Windows Controlled mode"
            );
            return Ok(unavailable("managed_policy_not_compilable"));
        }
    };
    Ok(ControlledNetworkPolicyResolution {
        state: "ready",
        source: Some(ControlledNetworkPolicySource {
            permission_profile: permission_profile.expect("allowed hosts require a profile"),
            allowed_hosts,
        }),
    })
}

fn unavailable(state: &'static str) -> ControlledNetworkPolicyResolution {
    ControlledNetworkPolicyResolution {
        state,
        source: None,
    }
}
