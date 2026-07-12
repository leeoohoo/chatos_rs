// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::{Extension, Json};

use crate::auth::CurrentPrincipal;
use crate::integrations::{
    create_harness_project_repo, get_harness_api_access_for_user, HarnessApiAccessResponse,
    HarnessProjectRepoCreateRequest, HarnessProjectRepoResponse,
};
use crate::state::AppState;

use super::internal_auth::{
    require_project_service_internal_request, HARNESS_ACCESS_READ_SCOPE, HARNESS_REPO_WRITE_SCOPE,
};
use super::{bad_request, forbidden, internal_error, ApiResult};

pub async fn create_project_repo(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    headers: HeaderMap,
    Json(input): Json<HarnessProjectRepoCreateRequest>,
) -> ApiResult<HarnessProjectRepoResponse> {
    require_project_service_internal_request(&state.config, &headers, HARNESS_REPO_WRITE_SCOPE)?;
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

pub async fn get_user_harness_access(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> ApiResult<HarnessApiAccessResponse> {
    require_project_service_internal_request(&state.config, &headers, HARNESS_ACCESS_READ_SCOPE)?;
    get_harness_api_access_for_user(&state, user_id.as_str())
        .await
        .map(Json)
        .map_err(internal_error)
}
