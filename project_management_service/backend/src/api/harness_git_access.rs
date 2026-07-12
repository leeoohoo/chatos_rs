// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use reqwest::Method;
use serde::{Deserialize, Serialize};

use super::internal_auth::{
    require_project_internal_request, CHATOS_CALLER, PROJECT_HARNESS_SCOPE, TASK_RUNNER_CALLER,
};
use super::ApiError;
use crate::http_body::{read_response_text_limited_or_message, ERROR_BODY_PREVIEW_LIMIT_BYTES};
use crate::models::ProjectRecord;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct HarnessApiAccessResponse {
    access_token: String,
    harness_uid: String,
    space_identifier: String,
}

#[derive(Debug, Serialize)]
pub(in crate::api) struct ProjectHarnessGitAccessResponse {
    project_id: String,
    repo_path: String,
    git_url: String,
    git_ssh_url: Option<String>,
    default_branch: String,
    space_identifier: String,
    access_username: String,
    access_token: String,
}

pub(in crate::api) async fn sync_get_project_harness_git_access(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ProjectHarnessGitAccessResponse>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER, TASK_RUNNER_CALLER],
        PROJECT_HARNESS_SCOPE,
    )?;
    let project = state
        .store
        .get_project(project_id.as_str())
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    let repo_path = required_project_value(&project.harness_repo_path, "harness_repo_path")?;
    let git_url = required_project_value(&project.harness_git_url, "harness_git_url")?;
    let project_space = required_project_value(
        &project.harness_space_identifier,
        "harness_space_identifier",
    )?;
    let owner_user_id = project_owner_user_id(&project)?;
    let access = fetch_harness_api_access(&state, owner_user_id.as_str()).await?;
    if access.space_identifier.trim() != project_space {
        return Err(ApiError::forbidden(
            "Harness access token owner does not match project Harness space",
        ));
    }

    Ok(Json(ProjectHarnessGitAccessResponse {
        project_id: project.id,
        repo_path,
        git_url,
        git_ssh_url: project.harness_git_ssh_url,
        default_branch: project
            .harness_default_branch
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("main")
            .to_string(),
        space_identifier: project_space,
        access_username: access.harness_uid,
        access_token: access.access_token,
    }))
}

fn required_project_value(value: &Option<String>, field: &str) -> Result<String, ApiError> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ApiError::bad_request(format!("project is missing {field}")))
}

fn project_owner_user_id(project: &ProjectRecord) -> Result<String, ApiError> {
    project
        .owner_user_id
        .as_deref()
        .or(project.creator_user_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ApiError::bad_request("project owner user id is missing"))
}

async fn fetch_harness_api_access(
    state: &AppState,
    owner_user_id: &str,
) -> Result<HarnessApiAccessResponse, ApiError> {
    let secret = state
        .config
        .user_service_internal_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::bad_request("PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET is not configured")
        })?;
    let endpoint = format!(
        "{}/api/internal/harness/users/{}/access",
        state
            .config
            .user_service_base_url
            .trim()
            .trim_end_matches('/'),
        urlencoding::encode(owner_user_id.trim())
    );
    let client = reqwest::Client::builder()
        .timeout(state.config.user_service_request_timeout)
        .build()
        .map_err(|err| ApiError::bad_request(format!("build user_service client failed: {err}")))?;
    let response = crate::user_model_runtime_client::signed_user_service_request(
        client.request(Method::GET, endpoint),
        secret,
        crate::user_model_runtime_client::HARNESS_ACCESS_READ_SCOPE,
    )
    .map_err(ApiError::bad_request)?
    .send()
    .await
    .map_err(|err| {
        ApiError::bad_request(format!("user_service Harness access request failed: {err}"))
    })?;
    if !response.status().is_success() {
        let status = response.status();
        let text =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(ApiError::bad_request(format!(
            "user_service Harness access request failed: {status} {text}"
        )));
    }
    response
        .json::<HarnessApiAccessResponse>()
        .await
        .map_err(|err| {
            ApiError::bad_request(format!(
                "parse user_service Harness access response failed: {err}"
            ))
        })
}
