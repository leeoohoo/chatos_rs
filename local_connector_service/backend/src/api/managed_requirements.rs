// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::{Extension, Json};
use chatos_sandbox_contract::{ManagedRequirementsBundle, ManagedRequirementsBundleLayer};
use chrono::Utc;

use crate::models::CurrentUser;
use crate::state::AppState;

use super::devices::load_owned_device;
use super::ApiError;

pub(super) async fn get_managed_requirements(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
) -> Result<Json<ManagedRequirementsBundle>, ApiError> {
    let device = load_owned_device(&state, &user, device_id.as_str(), true).await?;
    let signer = state.managed_requirements_signer.as_ref().ok_or_else(|| {
        ApiError::not_found("managed requirements are not configured for this service")
    })?;
    let layers = state
        .store
        .applicable_managed_requirements_layers(device.owner_user_id.as_str(), user.role.as_str())
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .map(|layer| ManagedRequirementsBundleLayer {
            policy_id: layer.policy.id,
            policy_version: layer.policy.version,
            assignment_id: layer.assignment.id,
            assignment_scope: layer.assignment.scope,
            requirements_toml: layer.policy.requirements_toml,
            requirements_sha256: layer.policy.content_sha256,
        })
        .collect();
    signer
        .bundle_for_device(
            device.owner_user_id.as_str(),
            device.id.as_str(),
            device.public_key.as_str(),
            layers,
            Utc::now(),
        )
        .map(Json)
        .map_err(ApiError::internal)
}
