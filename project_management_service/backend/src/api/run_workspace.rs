// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use axum::extract::{Path as AxumPath, State};
use axum::http::HeaderMap;
use axum::Json;
use chatos_mcp_management_sdk::{SandboxExecutionTarget, SandboxProviderKind};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::internal_auth::{
    require_project_internal_request, PROJECT_HARNESS_SCOPE, TASK_RUNNER_CALLER,
};
use super::ApiError;
use crate::http_body::{
    read_response_json_limited, read_response_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use crate::models::{
    ProjectImportStatus, ProjectRuntimeEnvironmentStatus, ProjectStatus,
    RuntimeEnvironmentProvider, RuntimeServiceRole,
};
use crate::services::cloud_import::git::{authenticated_git_url, run_git, run_git_output};
use crate::services::harness_repo::fetch_harness_api_access;
use crate::state::AppState;
use crate::trace_context::InternalTraceContextExt;

const SANDBOX_MANAGER_CALLER: &str = "project-service";
const SANDBOX_MANAGER_AUDIENCE: &str = "sandbox-manager";
const SANDBOX_MANAGER_SCOPE: &str = "sandbox.service";
const RUN_CHANGES_PATCH_LIMIT_BYTES: usize = 256 * 1024;
const GIT_INTEGRATION_LEASE_SECONDS: i64 = 10 * 60;

#[derive(Debug, Deserialize)]
pub(in crate::api) struct PrepareRunWorkspaceRequest {
    owner_user_id: String,
    tenant_id: String,
    create_run_branch: bool,
    create_cloud_sandbox: bool,
    execution_group_id: Option<String>,
    expected_execution_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::api) struct PreparedRunBranch {
    pub branch_id: String,
    pub branch_ref: String,
    pub base_branch: String,
    pub base_commit: String,
}

#[derive(Debug, Serialize)]
pub(in crate::api) struct PrepareRunWorkspaceResponse {
    project_id: String,
    run_id: String,
    default_branch: String,
    branch: Option<PreparedRunBranch>,
    sandbox_target: Option<SandboxExecutionTarget>,
    execution_branch_ref: Option<String>,
    execution_base_commit: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(in crate::api) struct FinalizeRunWorkspaceRequest {
    owner_user_id: String,
    branch: Option<PreparedRunBranch>,
    sandbox_target: Option<SandboxExecutionTarget>,
}

#[derive(Debug, Serialize)]
pub(in crate::api) struct FinalizeRunWorkspaceResponse {
    project_id: String,
    run_id: String,
    result_commit: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(in crate::api) struct GetRunWorkspaceChangesRequest {
    owner_user_id: String,
    branch: PreparedRunBranch,
}

#[derive(Debug, Serialize)]
pub(in crate::api) struct RunWorkspaceChangedFile {
    status: String,
    path: String,
    old_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub(in crate::api) struct GetRunWorkspaceChangesResponse {
    project_id: String,
    run_id: String,
    branch_ref: String,
    base_commit: String,
    result_commit: String,
    files: Vec<RunWorkspaceChangedFile>,
    patch: String,
    patch_truncated: bool,
}

#[derive(Debug, Deserialize)]
pub(in crate::api) struct IntegrateRunWorkspaceRequest {
    owner_user_id: String,
    execution_group_id: String,
    execution_branch_ref: String,
    integration_ready_at: String,
    branch: PreparedRunBranch,
    result_commit: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::api) enum RunWorkspaceIntegrationResultStatus {
    Integrated,
    Conflict,
    RetryableError,
}

#[derive(Debug, Serialize)]
pub(in crate::api) struct IntegrateRunWorkspaceResponse {
    project_id: String,
    run_id: String,
    status: RunWorkspaceIntegrationResultStatus,
    result_commit: String,
    integration_base_commit: Option<String>,
    integrated_commit: Option<String>,
    execution_head_commit: Option<String>,
    conflict_files: Vec<String>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(in crate::api) struct PromoteExecutionWorkspaceRequest {
    owner_user_id: String,
    execution_group_id: String,
    execution_branch_ref: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::api) enum PromoteExecutionWorkspaceStatus {
    Promoted,
    Conflict,
    RetryableError,
}

#[derive(Debug, Serialize)]
pub(in crate::api) struct PromoteExecutionWorkspaceResponse {
    project_id: String,
    execution_group_id: String,
    status: PromoteExecutionWorkspaceStatus,
    promoted_commit: Option<String>,
    conflict_files: Vec<String>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SandboxLeaseResponse {
    lease_id: String,
    sandbox_id: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseSandboxResponse {
    output_workspace: Option<String>,
    output_error: Option<String>,
}

pub(in crate::api) async fn prepare_run_workspace(
    AxumPath((project_id, run_id)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PrepareRunWorkspaceRequest>,
) -> Result<Json<PrepareRunWorkspaceResponse>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[TASK_RUNNER_CALLER],
        PROJECT_HARNESS_SCOPE,
    )?;
    let project_id = validate_project_id(project_id.as_str())?;
    let run_id = validate_run_id(run_id.as_str())?;
    let project = state
        .store
        .get_project(project_id.as_str())
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    if project.status == ProjectStatus::Archived
        || project.import_status != ProjectImportStatus::Ready
    {
        return Err(ApiError::conflict("project source workspace is not ready"));
    }
    let project_owner = project
        .owner_user_id
        .as_deref()
        .or(project.creator_user_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::conflict("project owner user id is missing"))?;
    if project_owner != request.owner_user_id.trim() {
        return Err(ApiError::forbidden(
            "represented user does not own the requested project",
        ));
    }
    if request.create_cloud_sandbox && !request.create_run_branch {
        return Err(ApiError::bad_request(
            "cloud sandbox preparation requires a run branch",
        ));
    }
    let git_url = required(&project.harness_git_url, "harness_git_url")?;
    let default_branch = project
        .harness_default_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("main")
        .to_string();
    let access = fetch_harness_api_access(&state, project_owner)
        .await
        .map_err(ApiError::bad_gateway)?;
    let authenticated_url = authenticated_git_url(
        git_url.as_str(),
        access.harness_uid.as_str(),
        access.access_token.as_str(),
    )
    .map_err(ApiError::bad_request)?;
    let scrub = [access.access_token.as_str(), authenticated_url.as_str()];
    let base_commit = remote_branch_sha(
        &state,
        authenticated_url.as_str(),
        default_branch.as_str(),
        &scrub,
    )
    .await?
    .ok_or_else(|| ApiError::conflict("Harness default branch does not exist"))?;
    let (execution_branch_ref, execution_base_commit) = if request.create_run_branch {
        let execution_group_id = validate_execution_group_id(
            request
                .execution_group_id
                .as_deref()
                .unwrap_or(run_id.as_str()),
        )?;
        let execution_branch_ref = format!("chatos/executions/{execution_group_id}");
        let execution_base_commit = ensure_execution_branch(
            &state,
            project_id.as_str(),
            run_id.as_str(),
            git_url.as_str(),
            authenticated_url.as_str(),
            default_branch.as_str(),
            base_commit.as_str(),
            execution_branch_ref.as_str(),
            &scrub,
        )
        .await?;
        let integration = state
            .store
            .ensure_execution_integration(
                project_id.as_str(),
                execution_group_id.as_str(),
                default_branch.as_str(),
                execution_branch_ref.as_str(),
                base_commit.as_str(),
                execution_base_commit.as_str(),
            )
            .await
            .map_err(ApiError::conflict)?;
        if integration.current_head_commit != execution_base_commit {
            return Err(ApiError::conflict(
                "execution branch HEAD does not match its integration record",
            ));
        }
        if let Some(expected) = request
            .expected_execution_commit
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if expected != execution_base_commit {
                return Err(ApiError::conflict(format!(
                    "execution branch advanced: expected {expected}, current {execution_base_commit}"
                )));
            }
        }
        (Some(execution_branch_ref), Some(execution_base_commit))
    } else {
        (None, None)
    };
    let branch = if request.create_run_branch {
        Some(
            ensure_run_branch(
                &state,
                project_id.as_str(),
                run_id.as_str(),
                git_url.as_str(),
                authenticated_url.as_str(),
                execution_branch_ref
                    .as_deref()
                    .expect("execution branch prepared for run branch"),
                execution_base_commit
                    .as_deref()
                    .expect("execution base prepared for run branch"),
                &scrub,
            )
            .await?,
        )
    } else {
        None
    };
    let sandbox_target = if request.create_cloud_sandbox {
        let workspace_image_id =
            resolve_cloud_workspace_image_id(&state, project_id.as_str()).await?;
        let branch = branch.as_ref().ok_or_else(|| {
            ApiError::conflict("cloud sandbox preparation lost its required run branch")
        })?;
        let workspace = prepare_shared_workspace(
            &state,
            project_id.as_str(),
            run_id.as_str(),
            git_url.as_str(),
            authenticated_url.as_str(),
            branch.branch_ref.as_str(),
            &scrub,
        )
        .await?;
        Some(
            create_cloud_sandbox(
                &state,
                project_id.as_str(),
                run_id.as_str(),
                request.tenant_id.trim(),
                project_owner,
                workspace.as_path(),
                workspace_image_id.as_str(),
            )
            .await?,
        )
    } else {
        None
    };

    Ok(Json(PrepareRunWorkspaceResponse {
        project_id,
        run_id,
        default_branch,
        branch,
        sandbox_target,
        execution_branch_ref,
        execution_base_commit,
    }))
}

pub(in crate::api) async fn finalize_run_workspace(
    AxumPath((project_id, run_id)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FinalizeRunWorkspaceRequest>,
) -> Result<Json<FinalizeRunWorkspaceResponse>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[TASK_RUNNER_CALLER],
        PROJECT_HARNESS_SCOPE,
    )?;
    let project_id = validate_project_id(project_id.as_str())?;
    let run_id = validate_run_id(run_id.as_str())?;
    let project = state
        .store
        .get_project(project_id.as_str())
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    let project_owner = project
        .owner_user_id
        .as_deref()
        .or(project.creator_user_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::conflict("project owner user id is missing"))?;
    if project_owner != request.owner_user_id.trim() {
        return Err(ApiError::forbidden(
            "represented user does not own the requested project",
        ));
    }
    let Some(branch) = request.branch else {
        return Ok(Json(FinalizeRunWorkspaceResponse {
            project_id,
            run_id,
            result_commit: None,
        }));
    };
    let git_url = required(&project.harness_git_url, "harness_git_url")?;
    let access = fetch_harness_api_access(&state, project_owner)
        .await
        .map_err(ApiError::bad_gateway)?;
    let authenticated_url = authenticated_git_url(
        git_url.as_str(),
        access.harness_uid.as_str(),
        access.access_token.as_str(),
    )
    .map_err(ApiError::bad_request)?;
    let scrub = [access.access_token.as_str(), authenticated_url.as_str()];
    let released_output = match request.sandbox_target.as_ref() {
        Some(target) => Some(release_cloud_sandbox(&state, target).await?),
        None => None,
    };
    let workspace = prepare_shared_workspace(
        &state,
        project_id.as_str(),
        run_id.as_str(),
        git_url.as_str(),
        authenticated_url.as_str(),
        branch.branch_ref.as_str(),
        &scrub,
    )
    .await?;
    if let Some(output_workspace) = released_output
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        replace_worktree_from_sandbox_output(workspace.as_path(), Path::new(output_workspace))?;
    }
    let result_commit = commit_and_push_run_branch(
        &state,
        workspace.as_path(),
        authenticated_url.as_str(),
        branch.branch_ref.as_str(),
        run_id.as_str(),
        &scrub,
    )
    .await?;
    let shared_workspace = run_workspace_path(project_id.as_str(), run_id.as_str());
    if shared_workspace.starts_with(run_workspace_root()) {
        let _ = std::fs::remove_dir_all(shared_workspace);
    }
    Ok(Json(FinalizeRunWorkspaceResponse {
        project_id,
        run_id,
        result_commit: Some(result_commit),
    }))
}

pub(in crate::api) async fn get_run_workspace_changes(
    AxumPath((project_id, run_id)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<GetRunWorkspaceChangesRequest>,
) -> Result<Json<GetRunWorkspaceChangesResponse>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[TASK_RUNNER_CALLER],
        PROJECT_HARNESS_SCOPE,
    )?;
    let project_id = validate_project_id(project_id.as_str())?;
    let run_id = validate_run_id(run_id.as_str())?;
    if request.branch.branch_ref != format!("chatos/runs/{run_id}") {
        return Err(ApiError::bad_request(
            "run branch does not match the requested Run",
        ));
    }
    let base_commit = validate_commit_sha(request.branch.base_commit.as_str())?;
    let project = state
        .store
        .get_project(project_id.as_str())
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    let project_owner = project
        .owner_user_id
        .as_deref()
        .or(project.creator_user_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::conflict("project owner user id is missing"))?;
    if project_owner != request.owner_user_id.trim() {
        return Err(ApiError::forbidden(
            "represented user does not own the requested project",
        ));
    }
    let git_url = required(&project.harness_git_url, "harness_git_url")?;
    let access = fetch_harness_api_access(&state, project_owner)
        .await
        .map_err(ApiError::bad_gateway)?;
    let authenticated_url = authenticated_git_url(
        git_url.as_str(),
        access.harness_uid.as_str(),
        access.access_token.as_str(),
    )
    .map_err(ApiError::bad_request)?;
    let scrub = [access.access_token.as_str(), authenticated_url.as_str()];
    let workspace_id = format!("changes-{run_id}");
    let workspace = prepare_shared_workspace(
        &state,
        project_id.as_str(),
        workspace_id.as_str(),
        git_url.as_str(),
        authenticated_url.as_str(),
        request.branch.branch_ref.as_str(),
        &scrub,
    )
    .await?;
    run_git(
        vec![
            "cat-file".to_string(),
            "-e".to_string(),
            format!("{base_commit}^{{commit}}"),
        ],
        Some(workspace.as_path()),
        &state.config,
        &scrub,
    )
    .await
    .map_err(|error| ApiError::conflict(format!("run base commit is unavailable: {error}")))?;
    let result_commit = run_git_output(
        vec!["rev-parse".to_string(), "HEAD".to_string()],
        Some(workspace.as_path()),
        &state.config,
        &scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?
    .trim()
    .to_string();
    let name_status = run_git_output(
        vec![
            "diff".to_string(),
            "--name-status".to_string(),
            "-z".to_string(),
            base_commit.clone(),
            result_commit.clone(),
            "--".to_string(),
        ],
        Some(workspace.as_path()),
        &state.config,
        &scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    let files = parse_name_status_z(name_status.as_str())?;
    let patch = run_git_output(
        vec![
            "diff".to_string(),
            "--no-ext-diff".to_string(),
            "--unified=3".to_string(),
            base_commit.clone(),
            result_commit.clone(),
            "--".to_string(),
        ],
        Some(workspace.as_path()),
        &state.config,
        &scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    let (patch, patch_truncated) = truncate_utf8(patch, RUN_CHANGES_PATCH_LIMIT_BYTES);
    Ok(Json(GetRunWorkspaceChangesResponse {
        project_id,
        run_id,
        branch_ref: request.branch.branch_ref,
        base_commit,
        result_commit,
        files,
        patch,
        patch_truncated,
    }))
}

pub(in crate::api) async fn integrate_run_workspace(
    AxumPath((project_id, run_id)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<IntegrateRunWorkspaceRequest>,
) -> Result<Json<IntegrateRunWorkspaceResponse>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[TASK_RUNNER_CALLER],
        PROJECT_HARNESS_SCOPE,
    )?;
    let project_id = validate_project_id(project_id.as_str())?;
    let run_id = validate_run_id(run_id.as_str())?;
    let execution_group_id = validate_execution_group_id(request.execution_group_id.as_str())?;
    let expected_execution_branch_ref = format!("chatos/executions/{execution_group_id}");
    if request.execution_branch_ref.trim() != expected_execution_branch_ref {
        return Err(ApiError::bad_request(
            "execution branch does not match the execution group",
        ));
    }
    if request.branch.branch_ref != format!("chatos/runs/{run_id}")
        || request.branch.base_branch != expected_execution_branch_ref
    {
        return Err(ApiError::bad_request(
            "run branch does not belong to the execution branch",
        ));
    }
    let result_commit = validate_commit_sha(request.result_commit.as_str())?;
    if request.integration_ready_at.trim().is_empty() {
        return Err(ApiError::bad_request("integration_ready_at is required"));
    }
    let project = state
        .store
        .get_project(project_id.as_str())
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    let project_owner = project
        .owner_user_id
        .as_deref()
        .or(project.creator_user_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::conflict("project owner user id is missing"))?;
    if project_owner != request.owner_user_id.trim() {
        return Err(ApiError::forbidden(
            "represented user does not own the requested project",
        ));
    }
    let git_url = required(&project.harness_git_url, "harness_git_url")?;
    let access = fetch_harness_api_access(&state, project_owner)
        .await
        .map_err(ApiError::bad_gateway)?;
    let authenticated_url = authenticated_git_url(
        git_url.as_str(),
        access.harness_uid.as_str(),
        access.access_token.as_str(),
    )
    .map_err(ApiError::bad_request)?;
    let scrub = [access.access_token.as_str(), authenticated_url.as_str()];
    let execution_head = remote_branch_sha(
        &state,
        authenticated_url.as_str(),
        expected_execution_branch_ref.as_str(),
        &scrub,
    )
    .await?
    .ok_or_else(|| ApiError::conflict("execution branch does not exist"))?;
    let lease_token = uuid::Uuid::new_v4().to_string();
    let worker_id = format!("project-service:{}", std::process::id());
    let Some(integration_record) = state
        .store
        .acquire_execution_integration_lease(
            project_id.as_str(),
            execution_group_id.as_str(),
            worker_id.as_str(),
            lease_token.as_str(),
            GIT_INTEGRATION_LEASE_SECONDS,
        )
        .await
        .map_err(ApiError::bad_gateway)?
    else {
        return Ok(Json(IntegrateRunWorkspaceResponse {
            project_id,
            run_id,
            status: RunWorkspaceIntegrationResultStatus::RetryableError,
            result_commit,
            integration_base_commit: Some(execution_head.clone()),
            integrated_commit: None,
            execution_head_commit: Some(execution_head),
            conflict_files: Vec::new(),
            message: Some("execution group integration lease is busy".to_string()),
        }));
    };
    if integration_record.execution_branch_ref != expected_execution_branch_ref
        || integration_record.current_head_commit != execution_head
    {
        let _ = state
            .store
            .release_execution_integration_lease(
                project_id.as_str(),
                execution_group_id.as_str(),
                lease_token.as_str(),
            )
            .await;
        return Err(ApiError::conflict(
            "execution branch HEAD does not match the leased integration record",
        ));
    }
    macro_rules! try_execution_lease {
        ($result:expr) => {
            match $result {
                Ok(value) => value,
                Err(error) => {
                    let _ = state
                        .store
                        .release_execution_integration_lease(
                            project_id.as_str(),
                            execution_group_id.as_str(),
                            lease_token.as_str(),
                        )
                        .await;
                    return Err(error);
                }
            }
        };
    }
    let remote_run_head = try_execution_lease!(
        async {
            remote_branch_sha(
                &state,
                authenticated_url.as_str(),
                request.branch.branch_ref.as_str(),
                &scrub,
            )
            .await?
            .ok_or_else(|| ApiError::conflict("run result branch does not exist"))
        }
        .await
    );
    let workspace = try_execution_lease!(
        prepare_shared_workspace(
            &state,
            project_id.as_str(),
            run_id.as_str(),
            git_url.as_str(),
            authenticated_url.as_str(),
            request.branch.branch_ref.as_str(),
            &scrub,
        )
        .await
    );
    try_execution_lease!(
        fetch_remote_branch(
            &state,
            workspace.as_path(),
            authenticated_url.as_str(),
            expected_execution_branch_ref.as_str(),
            &scrub,
        )
        .await
    );

    if remote_run_head != result_commit {
        if git_is_ancestor(
            &state,
            workspace.as_path(),
            "HEAD",
            format!("origin/{expected_execution_branch_ref}").as_str(),
            &scrub,
        )
        .await
        {
            let _ = state
                .store
                .release_execution_integration_lease(
                    project_id.as_str(),
                    execution_group_id.as_str(),
                    lease_token.as_str(),
                )
                .await;
            return Ok(Json(IntegrateRunWorkspaceResponse {
                project_id,
                run_id,
                status: RunWorkspaceIntegrationResultStatus::Integrated,
                result_commit,
                integration_base_commit: None,
                integrated_commit: Some(remote_run_head),
                execution_head_commit: Some(execution_head),
                conflict_files: Vec::new(),
                message: None,
            }));
        }
        if git_is_ancestor(
            &state,
            workspace.as_path(),
            format!("origin/{expected_execution_branch_ref}").as_str(),
            "HEAD",
            &scrub,
        )
        .await
        {
            try_execution_lease!(
                push_with_lease(
                    &state,
                    workspace.as_path(),
                    authenticated_url.as_str(),
                    expected_execution_branch_ref.as_str(),
                    execution_head.as_str(),
                    &scrub,
                )
                .await
            );
            try_execution_lease!(state
                .store
                .update_execution_integration_head(
                    project_id.as_str(),
                    execution_group_id.as_str(),
                    lease_token.as_str(),
                    execution_head.as_str(),
                    remote_run_head.as_str(),
                )
                .await
                .map_err(ApiError::conflict));
            state
                .store
                .release_execution_integration_lease(
                    project_id.as_str(),
                    execution_group_id.as_str(),
                    lease_token.as_str(),
                )
                .await
                .map_err(ApiError::bad_gateway)?;
            return Ok(Json(IntegrateRunWorkspaceResponse {
                project_id,
                run_id,
                status: RunWorkspaceIntegrationResultStatus::Integrated,
                result_commit,
                integration_base_commit: Some(execution_head),
                integrated_commit: Some(remote_run_head.clone()),
                execution_head_commit: Some(remote_run_head),
                conflict_files: Vec::new(),
                message: None,
            }));
        }
        let _ = state
            .store
            .release_execution_integration_lease(
                project_id.as_str(),
                execution_group_id.as_str(),
                lease_token.as_str(),
            )
            .await;
        return Err(ApiError::conflict(
            "run branch changed outside the integration protocol",
        ));
    }

    if let Err(error) = run_git(
        vec![
            "rebase".to_string(),
            format!("origin/{expected_execution_branch_ref}"),
        ],
        Some(workspace.as_path()),
        &state.config,
        &scrub,
    )
    .await
    {
        let conflict_files = run_git_output(
            vec![
                "diff".to_string(),
                "--name-only".to_string(),
                "--diff-filter=U".to_string(),
            ],
            Some(workspace.as_path()),
            &state.config,
            &scrub,
        )
        .await
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
        let _ = run_git(
            vec!["rebase".to_string(), "--abort".to_string()],
            Some(workspace.as_path()),
            &state.config,
            &scrub,
        )
        .await;
        let _ = state
            .store
            .mark_execution_integration_blocked(
                project_id.as_str(),
                execution_group_id.as_str(),
                lease_token.as_str(),
            )
            .await;
        let _ = state
            .store
            .release_execution_integration_lease(
                project_id.as_str(),
                execution_group_id.as_str(),
                lease_token.as_str(),
            )
            .await;
        return Ok(Json(IntegrateRunWorkspaceResponse {
            project_id,
            run_id,
            status: RunWorkspaceIntegrationResultStatus::Conflict,
            result_commit,
            integration_base_commit: Some(execution_head.clone()),
            integrated_commit: None,
            execution_head_commit: Some(execution_head),
            conflict_files,
            message: Some(format!("Task Run integration conflict: {error}")),
        }));
    }
    let integrated_commit = try_execution_lease!(run_git_output(
        vec!["rev-parse".to_string(), "HEAD".to_string()],
        Some(workspace.as_path()),
        &state.config,
        &scrub,
    )
    .await
    .map_err(ApiError::bad_gateway))
    .trim()
    .to_string();
    try_execution_lease!(
        push_run_branch_with_lease(
            &state,
            workspace.as_path(),
            authenticated_url.as_str(),
            request.branch.branch_ref.as_str(),
            result_commit.as_str(),
            &scrub,
        )
        .await
    );
    try_execution_lease!(
        push_with_lease(
            &state,
            workspace.as_path(),
            authenticated_url.as_str(),
            expected_execution_branch_ref.as_str(),
            execution_head.as_str(),
            &scrub,
        )
        .await
    );
    try_execution_lease!(state
        .store
        .update_execution_integration_head(
            project_id.as_str(),
            execution_group_id.as_str(),
            lease_token.as_str(),
            execution_head.as_str(),
            integrated_commit.as_str(),
        )
        .await
        .map_err(ApiError::conflict));
    state
        .store
        .release_execution_integration_lease(
            project_id.as_str(),
            execution_group_id.as_str(),
            lease_token.as_str(),
        )
        .await
        .map_err(ApiError::bad_gateway)?;
    Ok(Json(IntegrateRunWorkspaceResponse {
        project_id,
        run_id,
        status: RunWorkspaceIntegrationResultStatus::Integrated,
        result_commit,
        integration_base_commit: Some(execution_head),
        integrated_commit: Some(integrated_commit.clone()),
        execution_head_commit: Some(integrated_commit),
        conflict_files: Vec::new(),
        message: None,
    }))
}

pub(in crate::api) async fn promote_execution_workspace(
    AxumPath((project_id, execution_group_id)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PromoteExecutionWorkspaceRequest>,
) -> Result<Json<PromoteExecutionWorkspaceResponse>, ApiError> {
    require_project_internal_request(
        &state.config,
        &headers,
        &[TASK_RUNNER_CALLER],
        PROJECT_HARNESS_SCOPE,
    )?;
    let project_id = validate_project_id(project_id.as_str())?;
    let execution_group_id = validate_execution_group_id(execution_group_id.as_str())?;
    if request.execution_group_id.trim() != execution_group_id {
        return Err(ApiError::bad_request("execution group identity mismatch"));
    }
    let expected_execution_branch_ref = format!("chatos/executions/{execution_group_id}");
    if request.execution_branch_ref.trim() != expected_execution_branch_ref {
        return Err(ApiError::bad_request(
            "execution branch does not match the execution group",
        ));
    }
    let project = state
        .store
        .get_project(project_id.as_str())
        .await
        .map_err(ApiError::bad_request)?
        .ok_or_else(|| ApiError::not_found(format!("项目不存在: {project_id}")))?;
    let project_owner = project
        .owner_user_id
        .as_deref()
        .or(project.creator_user_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::conflict("project owner user id is missing"))?;
    if project_owner != request.owner_user_id.trim() {
        return Err(ApiError::forbidden(
            "represented user does not own the requested project",
        ));
    }
    let record = state
        .store
        .get_execution_integration(project_id.as_str(), execution_group_id.as_str())
        .await
        .map_err(ApiError::bad_gateway)?
        .ok_or_else(|| ApiError::not_found("execution integration record not found"))?;
    if record.execution_branch_ref != expected_execution_branch_ref {
        return Err(ApiError::conflict(
            "execution integration record has a different branch",
        ));
    }
    if record.status == crate::models::ProjectExecutionIntegrationStatus::Promoted {
        return Ok(Json(PromoteExecutionWorkspaceResponse {
            project_id,
            execution_group_id,
            status: PromoteExecutionWorkspaceStatus::Promoted,
            promoted_commit: record.promoted_commit,
            conflict_files: Vec::new(),
            message: None,
        }));
    }
    let git_url = required(&project.harness_git_url, "harness_git_url")?;
    let access = fetch_harness_api_access(&state, project_owner)
        .await
        .map_err(ApiError::bad_gateway)?;
    let authenticated_url = authenticated_git_url(
        git_url.as_str(),
        access.harness_uid.as_str(),
        access.access_token.as_str(),
    )
    .map_err(ApiError::bad_request)?;
    let scrub = [access.access_token.as_str(), authenticated_url.as_str()];
    let worker_id = format!("project-service:{}", std::process::id());
    let execution_lease_token = uuid::Uuid::new_v4().to_string();
    let Some(leased_record) = state
        .store
        .acquire_execution_integration_lease(
            project_id.as_str(),
            execution_group_id.as_str(),
            worker_id.as_str(),
            execution_lease_token.as_str(),
            GIT_INTEGRATION_LEASE_SECONDS,
        )
        .await
        .map_err(ApiError::bad_gateway)?
    else {
        return Ok(Json(PromoteExecutionWorkspaceResponse {
            project_id,
            execution_group_id,
            status: PromoteExecutionWorkspaceStatus::RetryableError,
            promoted_commit: None,
            conflict_files: Vec::new(),
            message: Some("execution integration lease is busy".to_string()),
        }));
    };
    let promotion_lease_token = uuid::Uuid::new_v4().to_string();
    let promotion_lease_acquired = match state
        .store
        .acquire_branch_promotion_lease(
            project_id.as_str(),
            leased_record.target_branch.as_str(),
            worker_id.as_str(),
            promotion_lease_token.as_str(),
            GIT_INTEGRATION_LEASE_SECONDS,
        )
        .await
    {
        Ok(acquired) => acquired,
        Err(error) => {
            let _ = state
                .store
                .release_execution_integration_lease(
                    project_id.as_str(),
                    execution_group_id.as_str(),
                    execution_lease_token.as_str(),
                )
                .await;
            return Err(ApiError::bad_gateway(error));
        }
    };
    if !promotion_lease_acquired {
        let _ = state
            .store
            .release_execution_integration_lease(
                project_id.as_str(),
                execution_group_id.as_str(),
                execution_lease_token.as_str(),
            )
            .await;
        return Ok(Json(PromoteExecutionWorkspaceResponse {
            project_id,
            execution_group_id,
            status: PromoteExecutionWorkspaceStatus::RetryableError,
            promoted_commit: None,
            conflict_files: Vec::new(),
            message: Some("target branch promotion lease is busy".to_string()),
        }));
    }
    macro_rules! try_promotion_leases {
        ($result:expr) => {
            match $result {
                Ok(value) => value,
                Err(error) => {
                    release_promotion_leases(
                        &state,
                        project_id.as_str(),
                        execution_group_id.as_str(),
                        leased_record.target_branch.as_str(),
                        execution_lease_token.as_str(),
                        promotion_lease_token.as_str(),
                    )
                    .await;
                    return Err(error);
                }
            }
        };
    }
    let execution_head = try_promotion_leases!(
        async {
            remote_branch_sha(
                &state,
                authenticated_url.as_str(),
                expected_execution_branch_ref.as_str(),
                &scrub,
            )
            .await?
            .ok_or_else(|| ApiError::conflict("execution branch does not exist"))
        }
        .await
    );
    let target_head = try_promotion_leases!(
        async {
            remote_branch_sha(
                &state,
                authenticated_url.as_str(),
                leased_record.target_branch.as_str(),
                &scrub,
            )
            .await?
            .ok_or_else(|| ApiError::conflict("target branch does not exist"))
        }
        .await
    );
    if execution_head != leased_record.current_head_commit {
        release_promotion_leases(
            &state,
            project_id.as_str(),
            execution_group_id.as_str(),
            leased_record.target_branch.as_str(),
            execution_lease_token.as_str(),
            promotion_lease_token.as_str(),
        )
        .await;
        return Err(ApiError::conflict(
            "execution branch HEAD changed outside the integration protocol",
        ));
    }
    let workspace_id = format!("promotion-{execution_group_id}");
    let workspace = try_promotion_leases!(
        prepare_shared_workspace(
            &state,
            project_id.as_str(),
            workspace_id.as_str(),
            git_url.as_str(),
            authenticated_url.as_str(),
            expected_execution_branch_ref.as_str(),
            &scrub,
        )
        .await
    );
    try_promotion_leases!(
        fetch_remote_branch(
            &state,
            workspace.as_path(),
            authenticated_url.as_str(),
            leased_record.target_branch.as_str(),
            &scrub,
        )
        .await
    );
    if git_is_ancestor(
        &state,
        workspace.as_path(),
        "HEAD",
        format!("origin/{}", leased_record.target_branch).as_str(),
        &scrub,
    )
    .await
    {
        try_promotion_leases!(state
            .store
            .mark_execution_integration_promoted(
                project_id.as_str(),
                execution_group_id.as_str(),
                execution_lease_token.as_str(),
                target_head.as_str(),
            )
            .await
            .map_err(ApiError::bad_gateway));
        release_promotion_leases(
            &state,
            project_id.as_str(),
            execution_group_id.as_str(),
            leased_record.target_branch.as_str(),
            execution_lease_token.as_str(),
            promotion_lease_token.as_str(),
        )
        .await;
        return Ok(Json(PromoteExecutionWorkspaceResponse {
            project_id,
            execution_group_id,
            status: PromoteExecutionWorkspaceStatus::Promoted,
            promoted_commit: Some(target_head),
            conflict_files: Vec::new(),
            message: None,
        }));
    }
    let mut promoted_commit = execution_head.clone();
    if target_head != leased_record.initial_base_commit {
        if let Err(error) = run_git(
            vec![
                "rebase".to_string(),
                format!("origin/{}", leased_record.target_branch),
            ],
            Some(workspace.as_path()),
            &state.config,
            &scrub,
        )
        .await
        {
            let conflict_files = run_git_output(
                vec![
                    "diff".to_string(),
                    "--name-only".to_string(),
                    "--diff-filter=U".to_string(),
                ],
                Some(workspace.as_path()),
                &state.config,
                &scrub,
            )
            .await
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
            let _ = run_git(
                vec!["rebase".to_string(), "--abort".to_string()],
                Some(workspace.as_path()),
                &state.config,
                &scrub,
            )
            .await;
            let _ = state
                .store
                .mark_execution_integration_blocked(
                    project_id.as_str(),
                    execution_group_id.as_str(),
                    execution_lease_token.as_str(),
                )
                .await;
            release_promotion_leases(
                &state,
                project_id.as_str(),
                execution_group_id.as_str(),
                leased_record.target_branch.as_str(),
                execution_lease_token.as_str(),
                promotion_lease_token.as_str(),
            )
            .await;
            return Ok(Json(PromoteExecutionWorkspaceResponse {
                project_id,
                execution_group_id,
                status: PromoteExecutionWorkspaceStatus::Conflict,
                promoted_commit: None,
                conflict_files,
                message: Some(format!("execution promotion conflict: {error}")),
            }));
        }
        promoted_commit = try_promotion_leases!(run_git_output(
            vec!["rev-parse".to_string(), "HEAD".to_string()],
            Some(workspace.as_path()),
            &state.config,
            &scrub,
        )
        .await
        .map_err(ApiError::bad_gateway))
        .trim()
        .to_string();
        try_promotion_leases!(
            push_with_lease(
                &state,
                workspace.as_path(),
                authenticated_url.as_str(),
                expected_execution_branch_ref.as_str(),
                execution_head.as_str(),
                &scrub,
            )
            .await
        );
        try_promotion_leases!(state
            .store
            .update_execution_integration_head(
                project_id.as_str(),
                execution_group_id.as_str(),
                execution_lease_token.as_str(),
                execution_head.as_str(),
                promoted_commit.as_str(),
            )
            .await
            .map_err(ApiError::conflict));
    }
    try_promotion_leases!(
        push_with_lease(
            &state,
            workspace.as_path(),
            authenticated_url.as_str(),
            leased_record.target_branch.as_str(),
            target_head.as_str(),
            &scrub,
        )
        .await
    );
    try_promotion_leases!(state
        .store
        .mark_execution_integration_promoted(
            project_id.as_str(),
            execution_group_id.as_str(),
            execution_lease_token.as_str(),
            promoted_commit.as_str(),
        )
        .await
        .map_err(ApiError::bad_gateway));
    release_promotion_leases(
        &state,
        project_id.as_str(),
        execution_group_id.as_str(),
        leased_record.target_branch.as_str(),
        execution_lease_token.as_str(),
        promotion_lease_token.as_str(),
    )
    .await;
    Ok(Json(PromoteExecutionWorkspaceResponse {
        project_id,
        execution_group_id,
        status: PromoteExecutionWorkspaceStatus::Promoted,
        promoted_commit: Some(promoted_commit),
        conflict_files: Vec::new(),
        message: None,
    }))
}

async fn release_cloud_sandbox(
    state: &AppState,
    target: &SandboxExecutionTarget,
) -> Result<String, ApiError> {
    if target.provider != SandboxProviderKind::Cloud {
        return Err(ApiError::bad_request(
            "run workspace contains a non-cloud sandbox target",
        ));
    }
    let token = sandbox_manager_token(state)?;
    let url = sandbox_manager_internal_url(
        state.config.sandbox_manager_base_url.as_str(),
        format!(
            "sandboxes/{}/release",
            urlencoding::encode(target.sandbox_id.trim())
        )
        .as_str(),
    );
    let response = state
        .config
        .sandbox_manager_http_client
        .post(url)
        .header("x-sandbox-caller", SANDBOX_MANAGER_CALLER)
        .header("x-sandbox-internal-token", token)
        .json(&json!({
            "lease_id": target.lease_id,
            "export_result": true,
            "destroy": true
        }))
        .with_internal_trace_context()
        .send()
        .await
        .map_err(|error| ApiError::bad_gateway(format!("release cloud sandbox failed: {error}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let body =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(ApiError::bad_gateway(format!(
            "release cloud sandbox failed: {status} {body}"
        )));
    }
    let released =
        read_response_json_limited::<ReleaseSandboxResponse>(response, JSON_BODY_LIMIT_BYTES)
            .await
            .map_err(|error| {
                ApiError::bad_gateway(format!("decode sandbox release failed: {error}"))
            })?;
    if let Some(error) = released
        .output_error
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Err(ApiError::bad_gateway(format!(
            "sandbox output export failed: {error}"
        )));
    }
    released
        .output_workspace
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_gateway("sandbox release did not return an output workspace"))
}

async fn commit_and_push_run_branch(
    state: &AppState,
    workspace: &Path,
    authenticated_url: &str,
    branch_ref: &str,
    run_id: &str,
    scrub: &[&str],
) -> Result<String, ApiError> {
    run_git(
        vec![
            "remote".to_string(),
            "set-url".to_string(),
            "origin".to_string(),
            authenticated_url.to_string(),
        ],
        Some(workspace),
        &state.config,
        scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    let status = run_git_output(
        vec!["status".to_string(), "--porcelain".to_string()],
        Some(workspace),
        &state.config,
        scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    if !status.trim().is_empty() {
        run_git(
            vec!["add".to_string(), "-A".to_string()],
            Some(workspace),
            &state.config,
            scrub,
        )
        .await
        .map_err(ApiError::bad_gateway)?;
        run_git(
            vec![
                "-c".to_string(),
                "user.name=Chatos Task Runner".to_string(),
                "-c".to_string(),
                "user.email=task-runner@chatos.local".to_string(),
                "commit".to_string(),
                "-m".to_string(),
                format!("Apply Task Run {run_id}"),
            ],
            Some(workspace),
            &state.config,
            scrub,
        )
        .await
        .map_err(ApiError::bad_gateway)?;
    }
    run_git(
        vec![
            "push".to_string(),
            "origin".to_string(),
            format!("HEAD:refs/heads/{branch_ref}"),
        ],
        Some(workspace),
        &state.config,
        scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    run_git_output(
        vec!["rev-parse".to_string(), "HEAD".to_string()],
        Some(workspace),
        &state.config,
        scrub,
    )
    .await
    .map(|value| value.trim().to_string())
    .map_err(ApiError::bad_gateway)
}

async fn ensure_execution_branch(
    state: &AppState,
    project_id: &str,
    run_id: &str,
    raw_git_url: &str,
    authenticated_url: &str,
    default_branch: &str,
    default_commit: &str,
    execution_branch_ref: &str,
    scrub: &[&str],
) -> Result<String, ApiError> {
    if let Some(existing) =
        remote_branch_sha(state, authenticated_url, execution_branch_ref, scrub).await?
    {
        return Ok(existing);
    }
    let workspace = prepare_shared_workspace(
        state,
        project_id,
        run_id,
        raw_git_url,
        authenticated_url,
        default_branch,
        scrub,
    )
    .await?;
    run_git(
        vec![
            "push".to_string(),
            authenticated_url.to_string(),
            format!("HEAD:refs/heads/{execution_branch_ref}"),
        ],
        Some(workspace.as_path()),
        &state.config,
        scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    Ok(default_commit.to_string())
}

async fn release_promotion_leases(
    state: &AppState,
    project_id: &str,
    execution_group_id: &str,
    target_branch: &str,
    execution_lease_token: &str,
    promotion_lease_token: &str,
) {
    let _ = state
        .store
        .release_branch_promotion_lease(project_id, target_branch, promotion_lease_token)
        .await;
    let _ = state
        .store
        .release_execution_integration_lease(project_id, execution_group_id, execution_lease_token)
        .await;
}

async fn fetch_remote_branch(
    state: &AppState,
    workspace: &Path,
    authenticated_url: &str,
    branch_ref: &str,
    scrub: &[&str],
) -> Result<(), ApiError> {
    run_git(
        vec![
            "fetch".to_string(),
            authenticated_url.to_string(),
            format!("refs/heads/{branch_ref}:refs/remotes/origin/{branch_ref}"),
        ],
        Some(workspace),
        &state.config,
        scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)
}

async fn git_is_ancestor(
    state: &AppState,
    workspace: &Path,
    ancestor: &str,
    descendant: &str,
    scrub: &[&str],
) -> bool {
    run_git(
        vec![
            "merge-base".to_string(),
            "--is-ancestor".to_string(),
            ancestor.to_string(),
            descendant.to_string(),
        ],
        Some(workspace),
        &state.config,
        scrub,
    )
    .await
    .is_ok()
}

async fn push_run_branch_with_lease(
    state: &AppState,
    workspace: &Path,
    authenticated_url: &str,
    branch_ref: &str,
    expected_commit: &str,
    scrub: &[&str],
) -> Result<(), ApiError> {
    run_git(
        vec![
            "push".to_string(),
            authenticated_url.to_string(),
            format!("HEAD:refs/heads/{branch_ref}"),
            format!("--force-with-lease=refs/heads/{branch_ref}:{expected_commit}"),
        ],
        Some(workspace),
        &state.config,
        scrub,
    )
    .await
    .map_err(|error| ApiError::conflict(format!("run branch CAS push failed: {error}")))
}

async fn push_with_lease(
    state: &AppState,
    workspace: &Path,
    authenticated_url: &str,
    branch_ref: &str,
    expected_commit: &str,
    scrub: &[&str],
) -> Result<(), ApiError> {
    run_git(
        vec![
            "push".to_string(),
            authenticated_url.to_string(),
            format!("HEAD:refs/heads/{branch_ref}"),
            format!("--force-with-lease=refs/heads/{branch_ref}:{expected_commit}"),
        ],
        Some(workspace),
        &state.config,
        scrub,
    )
    .await
    .map_err(|error| ApiError::conflict(format!("execution branch CAS push failed: {error}")))
}

async fn ensure_run_branch(
    state: &AppState,
    project_id: &str,
    run_id: &str,
    raw_git_url: &str,
    authenticated_url: &str,
    default_branch: &str,
    base_commit: &str,
    scrub: &[&str],
) -> Result<PreparedRunBranch, ApiError> {
    let branch_ref = format!("chatos/runs/{run_id}");
    let existing_branch_commit =
        remote_branch_sha(state, authenticated_url, branch_ref.as_str(), scrub).await?;
    let branch_base_commit = if let Some(existing_branch_commit) = existing_branch_commit {
        existing_branch_commit
    } else {
        let workspace = initialize_run_branch_workspace(
            state,
            project_id,
            run_id,
            raw_git_url,
            authenticated_url,
            default_branch,
            scrub,
        )
        .await?;
        run_git(
            vec![
                "push".to_string(),
                authenticated_url.to_string(),
                format!("HEAD:refs/heads/{branch_ref}"),
            ],
            Some(workspace.as_path()),
            &state.config,
            scrub,
        )
        .await
        .map_err(ApiError::bad_gateway)?;
        base_commit.to_string()
    };
    Ok(PreparedRunBranch {
        branch_id: format!("{project_id}:{run_id}"),
        branch_ref,
        base_branch: default_branch.to_string(),
        base_commit: branch_base_commit,
    })
}

async fn initialize_run_branch_workspace(
    state: &AppState,
    project_id: &str,
    run_id: &str,
    raw_git_url: &str,
    authenticated_url: &str,
    default_branch: &str,
    scrub: &[&str],
) -> Result<PathBuf, ApiError> {
    let workspace = run_workspace_path(project_id, run_id);
    if !workspace.starts_with(run_workspace_root()) {
        return Err(ApiError::bad_gateway(
            "run branch workspace escaped the configured root",
        ));
    }
    if workspace.exists() {
        std::fs::remove_dir_all(workspace.as_path()).map_err(|error| {
            ApiError::bad_gateway(format!(
                "remove stale run branch workspace {} failed: {error}",
                workspace.display()
            ))
        })?;
    }
    let parent = workspace
        .parent()
        .ok_or_else(|| ApiError::bad_gateway("run branch workspace has no parent"))?;
    tokio::fs::create_dir_all(parent).await.map_err(|error| {
        ApiError::bad_gateway(format!(
            "create run branch workspace parent failed: {error}"
        ))
    })?;
    run_git(
        vec!["init".to_string(), workspace.to_string_lossy().to_string()],
        None,
        &state.config,
        scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    run_git(
        vec![
            "remote".to_string(),
            "add".to_string(),
            "origin".to_string(),
            raw_git_url.to_string(),
        ],
        Some(workspace.as_path()),
        &state.config,
        scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    run_git(
        vec![
            "fetch".to_string(),
            authenticated_url.to_string(),
            format!("refs/heads/{default_branch}"),
        ],
        Some(workspace.as_path()),
        &state.config,
        scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    run_git(
        vec![
            "checkout".to_string(),
            "-B".to_string(),
            format!("chatos/runs/{run_id}"),
            "FETCH_HEAD".to_string(),
        ],
        Some(workspace.as_path()),
        &state.config,
        scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    Ok(workspace)
}

async fn remote_branch_sha(
    state: &AppState,
    authenticated_url: &str,
    branch: &str,
    scrub: &[&str],
) -> Result<Option<String>, ApiError> {
    let output = run_git_output(
        vec![
            "ls-remote".to_string(),
            authenticated_url.to_string(),
            format!("refs/heads/{branch}"),
        ],
        None,
        &state.config,
        scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    Ok(output
        .lines()
        .find_map(|line| line.split_whitespace().next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned))
}

async fn prepare_shared_workspace(
    state: &AppState,
    project_id: &str,
    run_id: &str,
    raw_git_url: &str,
    authenticated_url: &str,
    branch_ref: &str,
    scrub: &[&str],
) -> Result<PathBuf, ApiError> {
    let workspace = run_workspace_path(project_id, run_id);
    let parent = workspace
        .parent()
        .ok_or_else(|| ApiError::bad_gateway("run workspace has no parent"))?;
    tokio::fs::create_dir_all(parent).await.map_err(|error| {
        ApiError::bad_gateway(format!("create run workspace parent failed: {error}"))
    })?;
    if workspace.join(".git").is_dir() {
        run_git(
            vec![
                "fetch".to_string(),
                authenticated_url.to_string(),
                branch_ref.to_string(),
            ],
            Some(workspace.as_path()),
            &state.config,
            scrub,
        )
        .await
        .map_err(ApiError::bad_gateway)?;
        run_git(
            vec![
                "reset".to_string(),
                "--hard".to_string(),
                "FETCH_HEAD".to_string(),
            ],
            Some(workspace.as_path()),
            &state.config,
            scrub,
        )
        .await
        .map_err(ApiError::bad_gateway)?;
        run_git(
            vec!["clean".to_string(), "-fdx".to_string()],
            Some(workspace.as_path()),
            &state.config,
            scrub,
        )
        .await
        .map_err(ApiError::bad_gateway)?;
    } else {
        if workspace.exists() {
            std::fs::remove_dir_all(workspace.as_path()).map_err(|error| {
                ApiError::bad_gateway(format!(
                    "remove incomplete run workspace {} failed: {error}",
                    workspace.display()
                ))
            })?;
        }
        run_git(
            vec![
                "clone".to_string(),
                "--single-branch".to_string(),
                "--branch".to_string(),
                branch_ref.to_string(),
                authenticated_url.to_string(),
                workspace.to_string_lossy().to_string(),
            ],
            None,
            &state.config,
            scrub,
        )
        .await
        .map_err(ApiError::bad_gateway)?;
    }
    run_git(
        vec![
            "remote".to_string(),
            "set-url".to_string(),
            "origin".to_string(),
            raw_git_url.to_string(),
        ],
        Some(workspace.as_path()),
        &state.config,
        scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    Ok(workspace)
}

async fn create_cloud_sandbox(
    state: &AppState,
    project_id: &str,
    run_id: &str,
    tenant_id: &str,
    owner_user_id: &str,
    workspace: &Path,
    image_id: &str,
) -> Result<SandboxExecutionTarget, ApiError> {
    let token = sandbox_manager_token(state)?;
    let url = sandbox_manager_internal_url(
        state.config.sandbox_manager_base_url.as_str(),
        "sandboxes/leases",
    );
    let response = state
        .config
        .sandbox_manager_http_client
        .post(url)
        .header("x-sandbox-caller", SANDBOX_MANAGER_CALLER)
        .header("x-sandbox-internal-token", token)
        .header("x-idempotency-key", format!("task-run:{run_id}:workspace"))
        .json(&json!({
            "tenant_id": tenant_id,
            "user_id": owner_user_id,
            "project_id": project_id,
            "run_id": run_id,
            "workspace_root": workspace.to_string_lossy(),
            "image_id": image_id,
            "tools": ["filesystem", "terminal"],
            "ttl_seconds": 7200,
            "resource_limits": null,
            "network": null,
            "permission_profile_id": "workspace_write",
            "approval_policy": "on_request",
            "approval_reviewer": "user"
        }))
        .with_internal_trace_context()
        .send()
        .await
        .map_err(|error| ApiError::bad_gateway(format!("create cloud sandbox failed: {error}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let body =
            read_response_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES).await;
        return Err(ApiError::bad_gateway(format!(
            "create cloud sandbox failed: {status} {body}"
        )));
    }
    let lease = read_response_json_limited::<SandboxLeaseResponse>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|error| {
            ApiError::bad_gateway(format!("decode cloud sandbox lease failed: {error}"))
        })?;
    if lease.status.trim() != "ready" {
        return Err(ApiError::conflict(format!(
            "cloud sandbox is not ready: {}",
            lease.status.trim()
        )));
    }
    Ok(SandboxExecutionTarget {
        provider: SandboxProviderKind::Cloud,
        pairing_id: None,
        sandbox_id: lease.sandbox_id,
        lease_id: lease.lease_id,
        is_environment: false,
        service_id: None,
    })
}

async fn resolve_cloud_workspace_image_id(
    state: &AppState,
    project_id: &str,
) -> Result<String, ApiError> {
    let environment = state
        .store
        .get_project_runtime_environment(project_id)
        .await
        .map_err(ApiError::bad_gateway)?
        .ok_or_else(|| ApiError::conflict("project runtime environment is not initialized"))?;
    if environment.status != ProjectRuntimeEnvironmentStatus::Ready
        || !environment.sandbox_enabled
        || environment.sandbox_provider != RuntimeEnvironmentProvider::CloudSandboxManager
    {
        return Err(ApiError::conflict(
            "project cloud sandbox runtime environment is not ready",
        ));
    }
    let execution_service_id = environment
        .execution_service_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::conflict("project workspace execution service is missing"))?;
    let images = state
        .store
        .list_project_runtime_environment_images(project_id)
        .await
        .map_err(ApiError::bad_gateway)?;
    let workspace_image = images
        .iter()
        .find(|image| {
            image.service_role == RuntimeServiceRole::Workspace
                && image.service_id.trim() == execution_service_id
        })
        .ok_or_else(|| ApiError::conflict("project workspace execution image is missing"))?;
    if workspace_image.image_provider != RuntimeEnvironmentProvider::CloudSandboxManager
        || workspace_image.status.trim() != "ready"
    {
        return Err(ApiError::conflict(
            "project workspace execution image is not ready",
        ));
    }
    workspace_image
        .image_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ApiError::conflict("project workspace execution image id is missing"))
}

fn replace_worktree_from_sandbox_output(
    worktree: &Path,
    sandbox_output: &Path,
) -> Result<(), ApiError> {
    if !worktree.join(".git").is_dir() {
        return Err(ApiError::bad_gateway(
            "run branch workspace is not a Git worktree",
        ));
    }
    if !sandbox_output.is_dir() {
        return Err(ApiError::bad_gateway(format!(
            "sandbox output workspace is not a directory: {}",
            sandbox_output.display()
        )));
    }
    for entry in std::fs::read_dir(worktree).map_err(|error| {
        ApiError::bad_gateway(format!("read run branch worktree failed: {error}"))
    })? {
        let entry = entry.map_err(|error| {
            ApiError::bad_gateway(format!("read worktree entry failed: {error}"))
        })?;
        if entry.file_name() == ".git" {
            continue;
        }
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            ApiError::bad_gateway(format!("read worktree entry type failed: {error}"))
        })?;
        if file_type.is_dir() {
            std::fs::remove_dir_all(path.as_path()).map_err(|error| {
                ApiError::bad_gateway(format!(
                    "remove stale worktree directory {} failed: {error}",
                    path.display()
                ))
            })?;
        } else {
            std::fs::remove_file(path.as_path()).map_err(|error| {
                ApiError::bad_gateway(format!(
                    "remove stale worktree file {} failed: {error}",
                    path.display()
                ))
            })?;
        }
    }
    copy_workspace_contents(sandbox_output, worktree)
}

fn copy_workspace_contents(source: &Path, destination: &Path) -> Result<(), ApiError> {
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| {
            ApiError::bad_gateway(format!("scan run workspace failed: {error}"))
        })?;
        let relative = entry.path().strip_prefix(source).map_err(|error| {
            ApiError::bad_gateway(format!("resolve run workspace path failed: {error}"))
        })?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        if relative.components().any(
            |component| matches!(component, std::path::Component::Normal(name) if name == ".git"),
        ) {
            continue;
        }
        let target = destination.join(relative);
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            return Err(ApiError::bad_gateway(format!(
                "run workspace contains an unsupported symlink: {}",
                relative.display()
            )));
        }
        if file_type.is_dir() {
            std::fs::create_dir_all(target.as_path()).map_err(|error| {
                ApiError::bad_gateway(format!(
                    "create sandbox workspace directory failed: {error}"
                ))
            })?;
        } else if file_type.is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    ApiError::bad_gateway(format!(
                        "create sandbox workspace parent failed: {error}"
                    ))
                })?;
            }
            std::fs::copy(entry.path(), target.as_path()).map_err(|error| {
                ApiError::bad_gateway(format!("copy sandbox workspace file failed: {error}"))
            })?;
        }
    }
    Ok(())
}

fn sandbox_manager_token(state: &AppState) -> Result<String, ApiError> {
    let secret = state
        .config
        .sandbox_manager_client_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::conflict("sandbox manager client secret is not configured"))?;
    chatos_service_runtime::issue_internal_service_token(
        secret,
        SANDBOX_MANAGER_CALLER,
        SANDBOX_MANAGER_AUDIENCE,
        SANDBOX_MANAGER_SCOPE,
        60,
    )
    .map_err(ApiError::bad_gateway)
}

fn run_workspace_root() -> PathBuf {
    std::env::var("CHATOS_RUN_WORKSPACE_ROOT")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/data/chatos"))
}

fn run_workspace_path(project_id: &str, run_id: &str) -> PathBuf {
    run_workspace_root().join(project_id).join(run_id)
}

fn sandbox_manager_internal_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/api/internal/{}",
        base_url.trim().trim_end_matches('/'),
        path.trim().trim_start_matches('/')
    )
}

fn parse_name_status_z(value: &str) -> Result<Vec<RunWorkspaceChangedFile>, ApiError> {
    let mut fields = value.split('\0').filter(|field| !field.is_empty());
    let mut files = Vec::new();
    while let Some(status) = fields.next() {
        let first_path = fields
            .next()
            .ok_or_else(|| ApiError::bad_gateway("Git diff returned an incomplete file entry"))?;
        let is_rename_or_copy = status.starts_with('R') || status.starts_with('C');
        let (old_path, path) = if is_rename_or_copy {
            let new_path = fields.next().ok_or_else(|| {
                ApiError::bad_gateway("Git diff returned an incomplete rename entry")
            })?;
            (Some(first_path.to_string()), new_path.to_string())
        } else {
            (None, first_path.to_string())
        };
        files.push(RunWorkspaceChangedFile {
            status: status.to_string(),
            path,
            old_path,
        });
    }
    Ok(files)
}

fn truncate_utf8(value: String, limit_bytes: usize) -> (String, bool) {
    if value.len() <= limit_bytes {
        return (value, false);
    }
    let boundary = value
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= limit_bytes)
        .last()
        .unwrap_or(0);
    (value[..boundary].to_string(), true)
}

fn validate_run_id(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(ApiError::bad_request("invalid Task Runner run id"));
    }
    Ok(value.to_string())
}

fn validate_execution_group_id(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(ApiError::bad_request("invalid execution group id"));
    }
    Ok(value.to_string())
}

fn validate_commit_sha(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if !(value.len() == 40 || value.len() == 64)
        || !value.chars().all(|character| character.is_ascii_hexdigit())
    {
        return Err(ApiError::bad_request("invalid Git commit SHA"));
    }
    Ok(value.to_ascii_lowercase())
}

fn validate_project_id(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(ApiError::bad_request("invalid project id"));
    }
    Ok(value.to_string())
}

fn required(value: &Option<String>, field: &str) -> Result<String, ApiError> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ApiError::conflict(format!("project is missing {field}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_id_validation_rejects_path_segments_and_accepts_uuid_shape() {
        assert!(validate_run_id("run-123").is_ok());
        assert!(validate_run_id("../run-123").is_err());
        assert!(validate_run_id("run_123").is_err());
        assert!(validate_run_id("").is_err());
    }

    #[test]
    fn project_id_validation_rejects_workspace_traversal() {
        assert!(validate_project_id("project-123").is_ok());
        assert!(validate_project_id("project_123").is_ok());
        assert!(validate_project_id("../project-123").is_err());
    }

    #[test]
    fn sandbox_manager_calls_use_the_internal_mtls_router_prefix() {
        assert_eq!(
            sandbox_manager_internal_url("https://sandbox-manager:8097/", "/sandboxes/leases"),
            "https://sandbox-manager:8097/api/internal/sandboxes/leases"
        );
    }

    #[test]
    fn sandbox_output_replaces_worktree_but_preserves_git_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let worktree = temp.path().join("worktree");
        let output = temp.path().join("output");
        std::fs::create_dir_all(worktree.join(".git")).unwrap();
        std::fs::create_dir_all(worktree.join("stale")).unwrap();
        std::fs::create_dir_all(output.join("src")).unwrap();
        std::fs::write(worktree.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(worktree.join("stale/file.txt"), "stale\n").unwrap();
        std::fs::write(output.join("src/main.rs"), "fn main() {}\n").unwrap();

        replace_worktree_from_sandbox_output(worktree.as_path(), output.as_path()).unwrap();

        assert!(worktree.join(".git/HEAD").is_file());
        assert!(!worktree.join("stale").exists());
        assert_eq!(
            std::fs::read_to_string(worktree.join("src/main.rs")).unwrap(),
            "fn main() {}\n"
        );
    }

    #[test]
    fn name_status_parser_preserves_rename_sources() {
        let files = parse_name_status_z("M\0src/main.rs\0R100\0old.rs\0new.rs\0")
            .expect("parse name status");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].status, "M");
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[1].status, "R100");
        assert_eq!(files[1].old_path.as_deref(), Some("old.rs"));
        assert_eq!(files[1].path, "new.rs");
    }

    #[test]
    fn patch_truncation_keeps_utf8_valid() {
        let (value, truncated) = truncate_utf8("甲乙丙".to_string(), 7);
        assert_eq!(value, "甲乙");
        assert!(truncated);
    }
}
