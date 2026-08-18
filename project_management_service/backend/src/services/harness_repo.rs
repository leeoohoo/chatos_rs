// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use reqwest::Method;
use serde::Deserialize;

use crate::http_body::{read_response_text_limited_or_message, ERROR_BODY_PREVIEW_LIMIT_BYTES};
use crate::models::{now_rfc3339, ProjectRecord};
use crate::state::AppState;
use crate::trace_context::InternalTraceContextExt;
use chatos_service_runtime::http_body::{read_response_json_limited, JSON_BODY_LIMIT_BYTES};

use super::cloud_import::{
    create_harness_repo_for_project, create_harness_repo_for_project_owner,
    HarnessProjectRepoResponse,
};

pub const HARNESS_PROVISION_STATUS_PENDING: &str = "pending";
pub const HARNESS_PROVISION_STATUS_READY: &str = "ready";
pub const HARNESS_PROVISION_STATUS_FAILED: &str = "failed";

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HarnessApiAccess {
    pub(crate) access_token: String,
    pub(crate) harness_uid: String,
    pub(crate) space_identifier: String,
}

pub(crate) async fn fetch_harness_api_access(
    state: &AppState,
    owner_user_id: &str,
) -> Result<HarnessApiAccess, String> {
    let secret = state
        .config
        .user_service_internal_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "PROJECT_SERVICE_USER_SERVICE_INTERNAL_SECRET is not configured".to_string()
        })?;
    let endpoint = format!(
        "{}/api/internal/harness/users/{}/access",
        state
            .config
            .user_service_internal_base_url
            .trim()
            .trim_end_matches('/'),
        urlencoding::encode(owner_user_id.trim())
    );
    let response = crate::user_model_runtime_client::signed_user_service_request(
        state
            .config
            .user_service_internal_http_client
            .request(Method::GET, endpoint),
        secret,
        crate::user_model_runtime_client::HARNESS_ACCESS_READ_SCOPE,
    )?
    .with_internal_trace_context()
    .send()
    .await
    .map_err(|err| format!("user_service Harness access request failed: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let text =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(format!(
            "user_service Harness access request failed: {status} {text}"
        ));
    }
    read_response_json_limited::<HarnessApiAccess>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| format!("parse user_service Harness access response failed: {err}"))
}

pub async fn ensure_harness_repo_for_project(
    state: &AppState,
    access_token: &str,
    project: &mut ProjectRecord,
) -> Result<HarnessProjectRepoResponse, String> {
    project.harness_provision_status = Some(HARNESS_PROVISION_STATUS_PENDING.to_string());
    project.harness_provision_error = None;
    project.updated_at = now_rfc3339();
    state.store.save_project_record(project).await?;

    match create_harness_repo_for_project(&state.config, access_token, project).await {
        Ok(repo) => {
            apply_harness_repo_metadata(project, &repo);
            state.store.save_project_record(project).await?;
            Ok(repo)
        }
        Err(err) => {
            project.harness_provision_status = Some(HARNESS_PROVISION_STATUS_FAILED.to_string());
            project.harness_provision_error = Some(err.clone());
            project.updated_at = now_rfc3339();
            state.store.save_project_record(project).await?;
            Err(err)
        }
    }
}

pub async fn ensure_harness_repo_for_project_owner(
    state: &AppState,
    owner_user_id: &str,
    project: &mut ProjectRecord,
) -> Result<HarnessProjectRepoResponse, String> {
    if project_harness_metadata_ready(project) {
        return Ok(HarnessProjectRepoResponse {
            space_identifier: project.harness_space_identifier.clone().unwrap_or_default(),
            repo_identifier: project.harness_repo_identifier.clone().unwrap_or_default(),
            repo_path: project.harness_repo_path.clone().unwrap_or_default(),
            git_url: project.harness_git_url.clone().unwrap_or_default(),
            git_ssh_url: project.harness_git_ssh_url.clone(),
            default_branch: project
                .harness_default_branch
                .clone()
                .unwrap_or_else(|| "main".to_string()),
            push_username: String::new(),
            push_token: String::new(),
        });
    }
    project.harness_provision_status = Some(HARNESS_PROVISION_STATUS_PENDING.to_string());
    project.harness_provision_error = None;
    project.updated_at = now_rfc3339();
    state.store.save_project_record(project).await?;

    match create_harness_repo_for_project_owner(&state.config, owner_user_id, project).await {
        Ok(repo) => {
            apply_harness_repo_metadata(project, &repo);
            state.store.save_project_record(project).await?;
            Ok(repo)
        }
        Err(err) => {
            project.harness_provision_status = Some(HARNESS_PROVISION_STATUS_FAILED.to_string());
            project.harness_provision_error = Some(err.clone());
            project.updated_at = now_rfc3339();
            state.store.save_project_record(project).await?;
            Err(err)
        }
    }
}

pub fn project_harness_metadata_ready(project: &ProjectRecord) -> bool {
    [
        project.harness_repo_path.as_deref(),
        project.harness_git_url.as_deref(),
        project.harness_space_identifier.as_deref(),
    ]
    .into_iter()
    .all(|value| value.map(str::trim).is_some_and(|value| !value.is_empty()))
}

fn apply_harness_repo_metadata(project: &mut ProjectRecord, repo: &HarnessProjectRepoResponse) {
    let now = now_rfc3339();
    project.harness_space_identifier = Some(repo.space_identifier.clone());
    project.harness_repo_identifier = Some(repo.repo_identifier.clone());
    project.harness_repo_path = Some(repo.repo_path.clone());
    project.harness_git_url = Some(repo.git_url.clone());
    project.harness_git_ssh_url = repo.git_ssh_url.clone();
    project.harness_default_branch = Some(repo.default_branch.clone());
    project.harness_provision_status = Some(HARNESS_PROVISION_STATUS_READY.to_string());
    project.harness_provision_error = None;
    project.harness_provisioned_at = Some(now.clone());
    project.updated_at = now;
}
