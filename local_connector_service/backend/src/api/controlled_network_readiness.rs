// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::{Extension, Json};
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
    let resolution =
        resolve_controlled_network_policy_source(&state, &user, device_id.as_str()).await?;
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

async fn resolve_controlled_network_policy_source(
    state: &AppState,
    user: &CurrentUser,
    device_id: &str,
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
    let request = ControlledNetworkPolicyRequest::default();
    let permission_profile = document.default_permissions.clone();
    let allowed_hosts = match allowed_hosts_from_managed_requirements(&document, &request) {
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
