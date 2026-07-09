// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::{Extension, Json};

use crate::auth::CurrentPrincipal;
use crate::models::UpdateUserModelConfigRequest;
use crate::state::AppState;

use super::super::{bad_request, internal_error, not_found, ApiResult};
use super::access::ensure_model_access;

pub(in crate::api) async fn refresh_model_config_provider_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(principal): Extension<CurrentPrincipal>,
    Json(_input): Json<UpdateUserModelConfigRequest>,
) -> ApiResult<serde_json::Value> {
    let Some(existing_record) = state
        .store
        .find_user_model_config_by_id(id.as_str())
        .await
        .map_err(internal_error)?
    else {
        return Err(not_found("model config not found"));
    };
    ensure_model_access(&principal, &existing_record)?;
    Err(bad_request(
        "model provider refresh is managed by Local Connector client",
    ))
}
