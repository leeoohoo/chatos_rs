// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{
    normalize_approval_policy, normalize_approval_reviewer, normalize_optional_text,
    normalize_permission_profile_id, normalize_sandbox_mode, normalize_sandbox_readiness,
    CurrentUser, LocalConnectorSandboxPairing, SANDBOX_READINESS_READY,
};
use crate::state::AppState;

use super::{required_text, validate_device_workspace, ApiError};

#[derive(Debug, Deserialize)]
pub(super) struct SandboxPairingQuery {
    device_id: Option<String>,
    workspace_id: Option<String>,
    #[serde(default)]
    active_only: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateSandboxPairingRequest {
    device_id: Option<String>,
    workspace_id: Option<String>,
    sandbox_mode: Option<String>,
    sandbox_readiness: Option<String>,
    permission_profile_id: Option<String>,
    approval_policy: Option<String>,
    approval_reviewer: Option<String>,
    policy_revision: Option<String>,
    enabled: Option<bool>,
    access_client_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdateSandboxPairingRequest {
    workspace_id: Option<String>,
    sandbox_mode: Option<String>,
    sandbox_readiness: Option<String>,
    permission_profile_id: Option<String>,
    approval_policy: Option<String>,
    approval_reviewer: Option<String>,
    policy_revision: Option<String>,
    enabled: Option<bool>,
    access_client_id: Option<String>,
}

pub(super) async fn list_sandbox_pairings(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<SandboxPairingQuery>,
) -> Result<Json<Vec<LocalConnectorSandboxPairing>>, ApiError> {
    let owner_user_id = user.effective_owner_user_id();
    let requested_device_id = normalize_optional_text(query.device_id);
    let device_id = if query.active_only {
        let Some(session) = state
            .store
            .active_session(owner_user_id)
            .await
            .map_err(ApiError::internal)?
        else {
            return Ok(Json(Vec::new()));
        };
        if requested_device_id
            .as_deref()
            .is_some_and(|device_id| device_id != session.device_id)
        {
            return Ok(Json(Vec::new()));
        }
        Some(session.device_id)
    } else {
        requested_device_id
    };
    let mut pairings = state
        .store
        .list_sandbox_pairings(
            owner_user_id,
            device_id,
            normalize_optional_text(query.workspace_id),
        )
        .await
        .map_err(ApiError::internal)?;
    if query.active_only {
        pairings.retain(active_sandbox_pairing);
    }
    Ok(Json(pairings))
}

pub(super) async fn create_sandbox_pairing(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(req): Json<CreateSandboxPairingRequest>,
) -> Result<(StatusCode, Json<LocalConnectorSandboxPairing>), ApiError> {
    let device_id = required_text(req.device_id, "device_id")?;
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let mut pairing = LocalConnectorSandboxPairing::new(
        user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        req.enabled.unwrap_or(false),
        normalize_sandbox_mode(req.sandbox_mode),
        Some(normalize_sandbox_readiness(req.sandbox_readiness)),
        Some(normalize_permission_profile_id(req.permission_profile_id)),
        Some(normalize_approval_policy(req.approval_policy)),
        Some(normalize_approval_reviewer(req.approval_reviewer)),
        normalize_optional_text(req.policy_revision),
        None,
        normalize_optional_text(req.access_client_id),
    );
    pairing.facade_base_url = Some(state.config.sandbox_facade_base_url(pairing.id.as_str()));
    let saved = state
        .store
        .upsert_sandbox_pairing(&pairing)
        .await
        .map_err(ApiError::internal)?;
    Ok((StatusCode::CREATED, Json(saved)))
}

pub(super) async fn update_sandbox_pairing(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSandboxPairingRequest>,
) -> Result<Json<LocalConnectorSandboxPairing>, ApiError> {
    let mut pairing = load_owned_sandbox_pairing(&state, &user, id.as_str()).await?;
    if let Some(workspace_id) = normalize_optional_text(req.workspace_id) {
        pairing.workspace_id = workspace_id;
    }
    validate_device_workspace(
        &state,
        &user,
        pairing.device_id.as_str(),
        pairing.workspace_id.as_str(),
    )
    .await?;
    if let Some(mode) = normalize_optional_text(req.sandbox_mode) {
        pairing.sandbox_mode = normalize_sandbox_mode(Some(mode));
    }
    if let Some(readiness) = normalize_optional_text(req.sandbox_readiness) {
        pairing.sandbox_readiness = normalize_sandbox_readiness(Some(readiness));
    }
    if let Some(profile) = normalize_optional_text(req.permission_profile_id) {
        pairing.permission_profile_id = normalize_permission_profile_id(Some(profile));
    }
    if let Some(policy) = normalize_optional_text(req.approval_policy) {
        pairing.approval_policy = normalize_approval_policy(Some(policy));
    }
    if let Some(reviewer) = normalize_optional_text(req.approval_reviewer) {
        pairing.approval_reviewer = normalize_approval_reviewer(Some(reviewer));
    }
    if let Some(policy_revision) = normalize_optional_text(req.policy_revision) {
        pairing.policy_revision = Some(policy_revision);
    }
    if let Some(enabled) = req.enabled {
        pairing.enabled = enabled;
    }
    if let Some(access_client_id) = normalize_optional_text(req.access_client_id) {
        pairing.access_client_id = Some(access_client_id);
    }
    if pairing.facade_base_url.is_none() {
        pairing.facade_base_url = Some(state.config.sandbox_facade_base_url(pairing.id.as_str()));
    }
    state
        .store
        .update_sandbox_pairing(&pairing)
        .await
        .map_err(ApiError::internal)?;
    load_owned_sandbox_pairing(&state, &user, id.as_str())
        .await
        .map(Json)
}

pub(super) async fn delete_sandbox_pairing(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    load_owned_sandbox_pairing(&state, &user, id.as_str()).await?;
    state
        .store
        .delete_sandbox_pairing(user.effective_owner_user_id(), id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(json!({ "success": true })))
}

pub(super) async fn load_owned_sandbox_pairing(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<LocalConnectorSandboxPairing, ApiError> {
    let pairing = state
        .store
        .get_sandbox_pairing(id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Local Connector sandbox pairing not found"))?;
    if pairing.owner_user_id != user.effective_owner_user_id() {
        return Err(ApiError::forbidden(
            "Local Connector sandbox pairing does not belong to current user",
        ));
    }
    Ok(pairing)
}

fn active_sandbox_pairing(pairing: &LocalConnectorSandboxPairing) -> bool {
    pairing.enabled
        && pairing
            .sandbox_readiness
            .trim()
            .eq_ignore_ascii_case(SANDBOX_READINESS_READY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairing(enabled: bool, readiness: Option<&str>) -> LocalConnectorSandboxPairing {
        LocalConnectorSandboxPairing::new(
            "owner-1".to_string(),
            "device-1".to_string(),
            "workspace-1".to_string(),
            enabled,
            "docker".to_string(),
            readiness.map(ToOwned::to_owned),
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    #[test]
    fn active_sandbox_pairing_requires_enabled_and_ready() {
        assert!(active_sandbox_pairing(&pairing(true, Some("ready"))));
        assert!(active_sandbox_pairing(&pairing(true, Some(" READY "))));
        assert!(!active_sandbox_pairing(&pairing(false, Some("ready"))));
        assert!(!active_sandbox_pairing(&pairing(
            true,
            Some("setup_required")
        )));
        assert!(!active_sandbox_pairing(&pairing(
            true,
            Some("under_development")
        )));
        assert!(!active_sandbox_pairing(&pairing(true, Some("unsupported"))));
    }
}
