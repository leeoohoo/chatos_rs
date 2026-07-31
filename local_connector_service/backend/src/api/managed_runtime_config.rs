// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::{Extension, Json};
use chatos_agent::ManagedRuntimeConfigBundle;

use crate::models::CurrentUser;
use crate::state::AppState;

use super::ApiError;

pub(super) async fn get_managed_runtime_config(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<ManagedRuntimeConfigBundle>, ApiError> {
    if user.principal_type != "human_user" {
        return Err(ApiError::forbidden(
            "Managed runtime config sync requires a human user",
        ));
    }
    Ok(Json(state.managed_runtime_config_bundle().await))
}
