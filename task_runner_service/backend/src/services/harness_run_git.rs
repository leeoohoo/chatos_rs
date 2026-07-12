// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::warn;
use uuid::Uuid;

use crate::models::{TaskProjectRecord, TaskRecord, TaskRunEventRecord, TaskRunRecord};

use super::project_management_api_client::{self, ProjectHarnessGitAccess};
use super::workspace_snapshot::{copy_workspace_snapshot, replace_git_worktree_with_workspace};
use super::RunService;

#[path = "harness_run_git/run_service.rs"]
mod run_service;
#[cfg(test)]
pub(super) use run_service::{commit_workspace_to_run_branch, create_snapshot_commit_and_push};

const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct HarnessRunContext {
    pub project_id: String,
    pub repo_path: String,
    pub git_url: String,
    pub base_branch: String,
    pub run_branch: String,
    pub base_commit: String,
    pub effective_workspace_dir: String,
    #[serde(default, skip_serializing)]
    pub owned_workspace_root: Option<String>,
}

impl HarnessRunContext {
    pub(super) fn to_metadata(&self) -> serde_json::Value {
        json!({
            "enabled": true,
            "project_id": self.project_id,
            "repo_path": self.repo_path,
            "git_url": self.git_url,
            "base_branch": self.base_branch,
            "run_branch": self.run_branch,
            "base_commit": self.base_commit,
            "status": "prepared",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct HarnessRunOutputReport {
    pub enabled: bool,
    pub project_id: String,
    pub repo_path: String,
    pub git_url: String,
    pub base_branch: String,
    pub run_branch: String,
    pub base_commit: String,
    #[serde(default)]
    pub result_commit: Option<String>,
    pub status: String,
    #[serde(default)]
    pub message: Option<String>,
}

impl HarnessRunContext {
    fn output_report(
        &self,
        status: &str,
        result_commit: Option<String>,
        message: Option<String>,
    ) -> HarnessRunOutputReport {
        HarnessRunOutputReport {
            enabled: true,
            project_id: self.project_id.clone(),
            repo_path: self.repo_path.clone(),
            git_url: self.git_url.clone(),
            base_branch: self.base_branch.clone(),
            run_branch: self.run_branch.clone(),
            base_commit: self.base_commit.clone(),
            result_commit,
            status: status.to_string(),
            message,
        }
    }
}

async fn hydrate_cloud_workspace(
    worktree: &Path,
    destination: &Path,
    default_branch: &str,
    secrets: &[&str],
) -> Result<(), String> {
    let branch = normalize_branch_name(default_branch, "main");
    let remote_ref = format!("refs/remotes/origin/{branch}");
    if run_git_output(
        vec![
            "rev-parse".to_string(),
            "--verify".to_string(),
            remote_ref.clone(),
        ],
        Some(worktree),
        secrets,
    )
    .await
    .is_ok()
    {
        run_git(
            vec![
                "checkout".to_string(),
                "-f".to_string(),
                "-B".to_string(),
                "chatos-cloud-snapshot".to_string(),
                remote_ref,
            ],
            Some(worktree),
            secrets,
        )
        .await?;
    }
    copy_workspace_snapshot(
        worktree.to_string_lossy().as_ref(),
        destination.to_string_lossy().as_ref(),
    )
}

async fn resolve_workspace_branch(workspace_dir: &str, fallback: &str) -> String {
    let symbolic_branch = run_git_output(
        vec![
            "symbolic-ref".to_string(),
            "--quiet".to_string(),
            "--short".to_string(),
            "HEAD".to_string(),
        ],
        Some(Path::new(workspace_dir)),
        &[],
    )
    .await
    .ok();
    let output = match symbolic_branch {
        Some(value) => Some(value),
        None => run_git_output(
            vec![
                "rev-parse".to_string(),
                "--abbrev-ref".to_string(),
                "HEAD".to_string(),
            ],
            Some(Path::new(workspace_dir)),
            &[],
        )
        .await
        .ok(),
    };
    let candidate = output.as_deref().map(str::trim).unwrap_or_default();
    if candidate.is_empty() || candidate == "HEAD" {
        normalize_branch_name(fallback, "main")
    } else {
        normalize_branch_name(candidate, fallback)
    }
}

fn normalize_branch_name(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if is_valid_branch_name(value) {
        return value.to_string();
    }
    let fallback = fallback.trim();
    if is_valid_branch_name(fallback) {
        fallback.to_string()
    } else {
        "main".to_string()
    }
}

fn is_valid_branch_name(value: &str) -> bool {
    !value.is_empty()
        && value != "HEAD"
        && !value.starts_with(['.', '/'])
        && !value.ends_with(['.', '/'])
        && !value.contains("..")
        && !value.contains("@{")
        && !value.contains("//")
        && !value
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace() || "~^:?*[\\]".contains(ch))
}

fn normalize_run_branch_component(value: &str) -> String {
    let normalized = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    if normalized.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        normalized
    }
}

pub(super) fn harness_temp_dir(run_id: &str, phase: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "chatos-harness-run-{}-{}-{}",
        normalize_run_branch_component(run_id),
        phase,
        Uuid::new_v4()
    ))
}

pub(super) fn authenticated_git_url(access: &ProjectHarnessGitAccess) -> Result<String, String> {
    let mut url = Url::parse(access.git_url.trim())
        .map_err(|err| format!("invalid Harness git url: {err}"))?;
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(format!("unsupported Harness git url scheme: {other}")),
    }
    url.set_username(access.access_username.trim())
        .map_err(|_| "invalid Harness git username".to_string())?;
    url.set_password(Some(access.access_token.trim()))
        .map_err(|_| "invalid Harness git access token".to_string())?;
    Ok(url.to_string())
}

async fn run_git(args: Vec<String>, cwd: Option<&Path>, secrets: &[&str]) -> Result<(), String> {
    run_git_output(args, cwd, secrets).await.map(|_| ())
}

pub(super) async fn run_git_output(
    args: Vec<String>,
    cwd: Option<&Path>,
    secrets: &[&str],
) -> Result<String, String> {
    let mut command = Command::new("git");
    command
        .args(args.iter())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = timeout(GIT_COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| "git command timed out".to_string())?
        .map_err(|err| format!("start git command failed: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(scrub_secrets(
            format!("git command failed: {detail}"),
            secrets,
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn validate_git_access(
    project: &TaskProjectRecord,
    access: &ProjectHarnessGitAccess,
) -> Result<(), String> {
    if access.project_id.trim() != project.id.trim() {
        return Err("Harness git access project id mismatch".to_string());
    }
    if let Some(project_space) = project
        .harness_space_identifier
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if access.space_identifier.trim() != project_space {
            return Err("Harness git access space does not match project".to_string());
        }
    }
    Ok(())
}

fn scrub_secrets(mut value: String, secrets: &[&str]) -> String {
    for secret in secrets {
        let secret = secret.trim();
        if !secret.is_empty() {
            value = value.replace(secret, "***");
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::{is_valid_branch_name, normalize_run_branch_component};

    #[test]
    fn run_branch_component_removes_git_ref_punctuation() {
        assert_eq!(normalize_run_branch_component("run/1:2"), "run-1-2");
    }

    #[test]
    fn branch_validation_rejects_unsafe_refs() {
        assert!(is_valid_branch_name("feature/task-1"));
        assert!(!is_valid_branch_name("../main"));
        assert!(!is_valid_branch_name("feature bad"));
        assert!(!is_valid_branch_name("HEAD"));
    }
}
