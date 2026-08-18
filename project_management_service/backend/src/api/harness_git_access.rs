// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Serialize;
use std::collections::BTreeMap;

use super::internal_auth::{
    require_project_internal_request, CHATOS_CALLER, PROJECT_HARNESS_SCOPE, TASK_RUNNER_CALLER,
};
use super::ApiError;
use crate::auth::verify_token_via_user_service;
use crate::models::ProjectRecord;
use crate::services::cloud_import::git::{authenticated_git_url, run_git_output};
use crate::services::harness_repo::{
    ensure_harness_repo_for_project, fetch_harness_api_access, project_harness_metadata_ready,
};
use crate::state::AppState;

const CHATOS_OWNER_USER_ID_HEADER: &str = "x-chatos-owner-user-id";

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProjectHarnessGitBranch {
    name: String,
    sha: String,
    is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::api) struct ProjectHarnessGitBranchesResponse {
    project_id: String,
    current: Option<String>,
    branches: Vec<ProjectHarnessGitBranch>,
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
    let owner_user_id = project_owner_user_id(&project)?;
    let project =
        ensure_project_harness_metadata(&state, &headers, project, owner_user_id.as_str()).await?;
    let repo_path = required_project_value(&project.harness_repo_path, "harness_repo_path")?;
    let git_url = required_project_value(&project.harness_git_url, "harness_git_url")?;
    let project_space = required_project_value(
        &project.harness_space_identifier,
        "harness_space_identifier",
    )?;
    let access = fetch_harness_api_access(&state, owner_user_id.as_str())
        .await
        .map_err(ApiError::bad_request)?;
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

pub(in crate::api) async fn sync_get_project_harness_git_branches(
    Path(project_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ProjectHarnessGitBranchesResponse>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[CHATOS_CALLER],
        PROJECT_HARNESS_SCOPE,
    )?;
    let represented_owner_user_id = required_header(&headers, CHATOS_OWNER_USER_ID_HEADER)?;
    let project = state
        .store
        .get_project(project_id.as_str())
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    let owner_user_id = project_owner_user_id(&project)?;
    require_matching_owner(represented_owner_user_id, owner_user_id.as_str())?;
    let project =
        ensure_project_harness_metadata(&state, &headers, project, owner_user_id.as_str()).await?;

    let git_url = required_project_value(&project.harness_git_url, "harness_git_url")?;
    let project_space = required_project_value(
        &project.harness_space_identifier,
        "harness_space_identifier",
    )?;
    let default_branch = project
        .harness_default_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("main");
    let access = fetch_harness_api_access(&state, owner_user_id.as_str())
        .await
        .map_err(ApiError::bad_request)?;
    if access.space_identifier.trim() != project_space {
        return Err(ApiError::forbidden(
            "Harness access token owner does not match project Harness space",
        ));
    }

    let authenticated_url = authenticated_git_url(
        git_url.as_str(),
        access.harness_uid.as_str(),
        access.access_token.as_str(),
    )
    .map_err(ApiError::bad_request)?;
    let output = run_git_output(
        vec![
            "ls-remote".to_string(),
            "--symref".to_string(),
            authenticated_url.clone(),
            "HEAD".to_string(),
            "refs/heads/*".to_string(),
        ],
        None,
        &state.config,
        &[access.access_token.as_str(), authenticated_url.as_str()],
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    let (current, branches) = parse_git_ls_remote(output.as_str(), default_branch);

    Ok(Json(ProjectHarnessGitBranchesResponse {
        project_id: project.id,
        current,
        branches,
    }))
}

fn required_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, ApiError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request(format!("missing required header: {name}")))
}

fn require_matching_owner(represented_owner: &str, project_owner: &str) -> Result<(), ApiError> {
    if represented_owner.trim() == project_owner.trim() {
        return Ok(());
    }
    Err(ApiError::forbidden(
        "represented user does not own the requested project",
    ))
}

async fn ensure_project_harness_metadata(
    state: &AppState,
    headers: &HeaderMap,
    mut project: ProjectRecord,
    owner_user_id: &str,
) -> Result<ProjectRecord, ApiError> {
    if project_harness_metadata_ready(&project) {
        return Ok(project);
    }
    let user_access_token = user_access_token_from_headers(headers)?
        .ok_or_else(|| ApiError::bad_request("project Harness repository is not ready"))?;
    let user = verify_token_via_user_service(&state.config, user_access_token.as_str())
        .await
        .map_err(ApiError::forbidden)?;
    if !user.can_access_owned_resource(Some(owner_user_id)) {
        return Err(ApiError::forbidden(
            "user token owner does not match project owner",
        ));
    }
    ensure_harness_repo_for_project(state, user_access_token.as_str(), &mut project)
        .await
        .map_err(ApiError::bad_gateway)?;
    Ok(project)
}

fn user_access_token_from_headers(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    for key in [
        "x-chatos-user-authorization",
        "x-user-service-authorization",
        "x-chatos-user-token",
    ] {
        let Some(value) = headers.get(key) else {
            continue;
        };
        let value = value
            .to_str()
            .map_err(|_| ApiError::bad_request(format!("{key} header is invalid UTF-8")))?
            .trim();
        let token = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
            .map(str::trim)
            .unwrap_or(value);
        if !token.is_empty() {
            return Ok(Some(token.to_string()));
        }
    }
    Ok(None)
}

fn parse_git_ls_remote(
    output: &str,
    configured_default_branch: &str,
) -> (Option<String>, Vec<ProjectHarnessGitBranch>) {
    let mut symbolic_head = None;
    let mut branch_shas = BTreeMap::new();
    for line in output.lines() {
        let Some((value, reference)) = line.split_once('\t') else {
            continue;
        };
        let value = value.trim();
        let reference = reference.trim();
        if reference == "HEAD" {
            if let Some(branch) = value.strip_prefix("ref: refs/heads/") {
                let branch = branch.trim();
                if !branch.is_empty() {
                    symbolic_head = Some(branch.to_string());
                }
            }
            continue;
        }
        let Some(branch) = reference.strip_prefix("refs/heads/") else {
            continue;
        };
        let branch = branch.trim();
        if !branch.is_empty() && !value.is_empty() {
            branch_shas.insert(branch.to_string(), value.to_string());
        }
    }

    let configured_default = configured_default_branch.trim();
    let current = symbolic_head
        .or_else(|| (!configured_default.is_empty()).then(|| configured_default.to_string()))
        .or_else(|| branch_shas.keys().next().cloned());
    let branches = branch_shas
        .into_iter()
        .map(|(name, sha)| ProjectHarnessGitBranch {
            is_default: current.as_deref() == Some(name.as_str()),
            name,
            sha,
        })
        .collect();
    (current, branches)
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

#[cfg(test)]
mod tests {
    use super::{parse_git_ls_remote, require_matching_owner};

    #[test]
    fn parses_symbolic_head_and_sorted_branches() {
        let output = concat!(
            "ref: refs/heads/main\tHEAD\n",
            "1111111111111111111111111111111111111111\tHEAD\n",
            "2222222222222222222222222222222222222222\trefs/heads/release\n",
            "1111111111111111111111111111111111111111\trefs/heads/main\n",
        );

        let (current, branches) = parse_git_ls_remote(output, "fallback");

        assert_eq!(current.as_deref(), Some("main"));
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[0].name, "main");
        assert!(branches[0].is_default);
        assert_eq!(branches[1].name, "release");
        assert!(!branches[1].is_default);
    }

    #[test]
    fn empty_repository_uses_configured_default_branch() {
        let (current, branches) = parse_git_ls_remote("", "develop");

        assert_eq!(current.as_deref(), Some("develop"));
        assert!(branches.is_empty());
    }

    #[test]
    fn represented_owner_must_match_project_owner() {
        assert!(require_matching_owner("user-1", "user-1").is_ok());
        assert!(require_matching_owner("user-2", "user-1").is_err());
    }
}
