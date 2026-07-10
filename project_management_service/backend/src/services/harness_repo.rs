// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{now_rfc3339, ProjectRecord};
use crate::state::AppState;

use super::cloud_import::{create_harness_repo_for_project, HarnessProjectRepoResponse};

pub const HARNESS_PROVISION_STATUS_PENDING: &str = "pending";
pub const HARNESS_PROVISION_STATUS_READY: &str = "ready";
pub const HARNESS_PROVISION_STATUS_FAILED: &str = "failed";

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
