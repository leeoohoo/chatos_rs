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
    record_user_service_internal_resource_access, require_project_service_internal_request,
    UserServiceInternalResourceAudit, HARNESS_ACCESS_READ_SCOPE, HARNESS_REPO_WRITE_SCOPE,
};
use super::{bad_request, forbidden, internal_error, ApiResult};

pub async fn create_project_repo(
    State(state): State<AppState>,
    Extension(principal): Extension<CurrentPrincipal>,
    headers: HeaderMap,
    Json(input): Json<HarnessProjectRepoCreateRequest>,
) -> ApiResult<HarnessProjectRepoResponse> {
    let identity = require_project_service_internal_request(
        &state.config,
        &headers,
        HARNESS_REPO_WRITE_SCOPE,
    )?;
    let represented_user_id = principal
        .user_id
        .as_deref()
        .or(principal.owner_user_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let project_id = input.project_id.trim().to_string();
    let project_name = input.project_name.trim().to_string();
    let audit_resource_id = if project_id.is_empty() {
        "unknown"
    } else {
        project_id.as_str()
    };
    let result = async {
        let owner_user_id = represented_user_id
            .ok_or_else(|| forbidden("human user or agent owner identity is required"))?;
        if project_id.is_empty() {
            return Err(bad_request("project_id is required"));
        }
        if project_name.is_empty() {
            return Err(bad_request("project_name is required"));
        }
        create_harness_project_repo(&state, owner_user_id, input)
            .await
            .map(Json)
            .map_err(internal_error)
    }
    .await;
    record_user_service_internal_resource_access(
        &identity,
        UserServiceInternalResourceAudit {
            represented_user_id,
            project_id: (!project_id.is_empty()).then_some(project_id.as_str()),
            resource_type: "harness_project_repository",
            resource_id: audit_resource_id,
            resource_name: None,
            action: "create",
            outcome: if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
        },
    );
    result
}

pub async fn create_project_repo_for_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Json(input): Json<HarnessProjectRepoCreateRequest>,
) -> ApiResult<HarnessProjectRepoResponse> {
    let identity = require_project_service_internal_request(
        &state.config,
        &headers,
        HARNESS_REPO_WRITE_SCOPE,
    )?;
    let owner_user_id = user_id.trim().to_string();
    let project_id = input.project_id.trim().to_string();
    let project_name = input.project_name.trim().to_string();
    let audit_resource_id = if project_id.is_empty() {
        "unknown"
    } else {
        project_id.as_str()
    };
    let result = async {
        if owner_user_id.is_empty() {
            return Err(bad_request("owner user id is required"));
        }
        if project_id.is_empty() {
            return Err(bad_request("project_id is required"));
        }
        if project_name.is_empty() {
            return Err(bad_request("project_name is required"));
        }
        create_harness_project_repo(&state, owner_user_id.as_str(), input)
            .await
            .map(Json)
            .map_err(internal_error)
    }
    .await;
    record_user_service_internal_resource_access(
        &identity,
        UserServiceInternalResourceAudit {
            represented_user_id: (!owner_user_id.is_empty()).then_some(owner_user_id.as_str()),
            project_id: (!project_id.is_empty()).then_some(project_id.as_str()),
            resource_type: "harness_project_repository",
            resource_id: audit_resource_id,
            resource_name: None,
            action: "create",
            outcome: if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
        },
    );
    result
}

pub async fn get_user_harness_access(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> ApiResult<HarnessApiAccessResponse> {
    let identity = require_project_service_internal_request(
        &state.config,
        &headers,
        HARNESS_ACCESS_READ_SCOPE,
    )?;
    let user_id = user_id.trim().to_string();
    let audit_resource_id = if user_id.is_empty() {
        "unknown"
    } else {
        user_id.as_str()
    };
    let result = get_harness_api_access_for_user(&state, user_id.as_str())
        .await
        .map(Json)
        .map_err(internal_error);
    record_user_service_internal_resource_access(
        &identity,
        UserServiceInternalResourceAudit {
            represented_user_id: (!user_id.is_empty()).then_some(user_id.as_str()),
            project_id: None,
            resource_type: "harness_api_access",
            resource_id: audit_resource_id,
            resource_name: None,
            action: "read",
            outcome: if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
        },
    );
    result
}
