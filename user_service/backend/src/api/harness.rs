// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Extension, Json};

use crate::auth::CurrentPrincipal;
use crate::integrations::{
    create_harness_project_repo, HarnessProjectRepoCreateRequest, HarnessProjectRepoResponse,
};
use crate::state::AppState;

use super::{bad_request, forbidden, internal_error, ApiResult};

const INTERNAL_SECRET_HEADER: &str = "x-user-service-internal-secret";

pub async fn create_project_repo(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    headers: HeaderMap,
    Json(input): Json<HarnessProjectRepoCreateRequest>,
) -> ApiResult<HarnessProjectRepoResponse> {
    require_internal_secret(&state, &headers)?;
    let owner_user_id = principal
        .user_id
        .as_deref()
        .or(principal.owner_user_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| forbidden("human user or agent owner identity is required"))?;
    if input.project_id.trim().is_empty() {
        return Err(bad_request("project_id is required"));
    }
    if input.project_name.trim().is_empty() {
        return Err(bad_request("project_name is required"));
    }
    create_harness_project_repo(&state, owner_user_id, input)
        .await
        .map(Json)
        .map_err(internal_error)
}

fn require_internal_secret(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let Some(expected) = state
        .config
        .user_service_internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(forbidden(
            "USER_SERVICE_INTERNAL_API_SECRET is not configured",
        ));
    };
    let provided = headers
        .get(INTERNAL_SECRET_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .unwrap_or_default();
    if provided != expected {
        return Err(forbidden("invalid user service internal secret"));
    }
    Ok(())
}
