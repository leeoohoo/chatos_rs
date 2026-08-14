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

#[derive(Debug, Deserialize)]
pub(in crate::api) struct PrepareRunWorkspaceRequest {
    owner_user_id: String,
    tenant_id: String,
    create_run_branch: bool,
    create_cloud_sandbox: bool,
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
}

#[derive(Debug, Deserialize)]
pub(in crate::api) struct FinalizeRunWorkspaceRequest {
    owner_user_id: String,
    promote_changes: bool,
    branch: Option<PreparedRunBranch>,
    sandbox_target: Option<SandboxExecutionTarget>,
}

#[derive(Debug, Serialize)]
pub(in crate::api) struct FinalizeRunWorkspaceResponse {
    project_id: String,
    run_id: String,
    promoted: bool,
    result_commit: Option<String>,
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
    let branch = if request.create_run_branch {
        Some(
            ensure_run_branch(
                &state,
                project_id.as_str(),
                run_id.as_str(),
                git_url.as_str(),
                authenticated_url.as_str(),
                default_branch.as_str(),
                base_commit.as_str(),
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
            promoted: false,
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
    let promoted = if request.promote_changes {
        promote_run_branch(
            &state,
            workspace.as_path(),
            authenticated_url.as_str(),
            branch.base_branch.as_str(),
            run_id.as_str(),
            &scrub,
        )
        .await?;
        true
    } else {
        false
    };
    let shared_workspace = run_workspace_path(project_id.as_str(), run_id.as_str());
    if shared_workspace.starts_with(run_workspace_root()) {
        let _ = std::fs::remove_dir_all(shared_workspace);
    }
    Ok(Json(FinalizeRunWorkspaceResponse {
        project_id,
        run_id,
        promoted,
        result_commit: Some(result_commit),
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

async fn promote_run_branch(
    state: &AppState,
    workspace: &Path,
    authenticated_url: &str,
    base_branch: &str,
    run_id: &str,
    scrub: &[&str],
) -> Result<(), ApiError> {
    run_git(
        vec![
            "fetch".to_string(),
            authenticated_url.to_string(),
            format!("refs/heads/{base_branch}:refs/remotes/origin/{base_branch}"),
        ],
        Some(workspace),
        &state.config,
        scrub,
    )
    .await
    .map_err(ApiError::bad_gateway)?;
    if let Err(error) = run_git(
        vec!["rebase".to_string(), format!("origin/{base_branch}")],
        Some(workspace),
        &state.config,
        scrub,
    )
    .await
    {
        let _ = run_git(
            vec!["rebase".to_string(), "--abort".to_string()],
            Some(workspace),
            &state.config,
            scrub,
        )
        .await;
        return Err(ApiError::conflict(format!(
            "Task Run {run_id} conflicts with the latest {base_branch}: {error}"
        )));
    }
    run_git(
        vec![
            "push".to_string(),
            authenticated_url.to_string(),
            format!("HEAD:refs/heads/{base_branch}"),
        ],
        Some(workspace),
        &state.config,
        scrub,
    )
    .await
    .map_err(|error| {
        ApiError::conflict(format!(
            "Task Run {run_id} could not promote to {base_branch}: {error}"
        ))
    })
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
}
