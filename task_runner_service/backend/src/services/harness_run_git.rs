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
pub(super) use run_service::{
    commit_workspace_to_run_branch, create_cloud_run_branch, create_snapshot_commit_and_push,
    promote_run_branch_to_base,
};

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
    pub(super) fn from_run(run: &TaskRunRecord) -> Result<Option<Self>, String> {
        let Some(value) = run.input_snapshot.get("harness") else {
            return Ok(None);
        };
        let required = |key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("persisted Harness context is missing {key}"))
        };
        Ok(Some(Self {
            project_id: required("project_id")?,
            repo_path: required("repo_path")?,
            git_url: required("git_url")?,
            base_branch: required("base_branch")?,
            run_branch: required("run_branch")?,
            base_commit: required("base_commit")?,
            effective_workspace_dir: run
                .input_snapshot
                .get("effective_workspace_dir")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    "persisted Harness context is missing effective_workspace_dir".to_string()
                })?,
            owned_workspace_root: None,
        }))
    }

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

impl RunService {
    pub(crate) async fn cleanup_harness_artifacts_for_run(
        &self,
        run: &TaskRunRecord,
    ) -> Result<Vec<String>, String> {
        let mut cleaned = cleanup_managed_harness_temp_dirs(run.id.as_str())?;
        let Some(harness) = run.input_snapshot.get("harness") else {
            return Ok(cleaned);
        };
        let Some(project_id) = harness
            .get("project_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(cleaned);
        };
        let Some(run_branch) = harness
            .get("run_branch")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(cleaned);
        };
        let expected_branch = format!(
            "chatos/runs/{}",
            normalize_run_branch_component(run.id.as_str())
        );
        if run_branch != expected_branch {
            return Err(format!(
                "refusing to delete unexpected run branch: {run_branch}"
            ));
        }
        let access =
            project_management_api_client::get_project_harness_git_access(&self.config, project_id)
                .await?;
        let authenticated_url = authenticated_git_url(&access)?;
        let secrets = [access.access_token.as_str()];
        let branch_ref = format!("refs/heads/{run_branch}");
        let existing = run_git_output(
            vec![
                "ls-remote".to_string(),
                "--heads".to_string(),
                authenticated_url.clone(),
                branch_ref,
            ],
            None,
            &secrets,
        )
        .await?;
        if !existing.trim().is_empty() {
            run_git(
                vec![
                    "push".to_string(),
                    authenticated_url,
                    "--delete".to_string(),
                    run_branch.to_string(),
                ],
                None,
                &secrets,
            )
            .await?;
            cleaned.push(format!("branch:{run_branch}"));
        }
        Ok(cleaned)
    }
}

fn cleanup_managed_harness_temp_dirs(run_id: &str) -> Result<Vec<String>, String> {
    let prefix = format!(
        "chatos-harness-run-{}-",
        normalize_run_branch_component(run_id)
    );
    let mut cleaned = Vec::new();
    let entries = std::fs::read_dir(std::env::temp_dir()).map_err(|error| error.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(prefix.as_str()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(path.as_path()).map_err(|error| {
                format!("remove managed Harness temp directory failed: {error}")
            })?;
            cleaned.push(format!("directory:{}", path.display()));
        }
    }
    Ok(cleaned)
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
    #[serde(default)]
    pub promoted_commit: Option<String>,
    pub status: String,
    #[serde(default)]
    pub message: Option<String>,
}

impl HarnessRunContext {
    fn output_report(
        &self,
        status: &str,
        result_commit: Option<String>,
        promoted_commit: Option<String>,
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
            promoted_commit,
            status: status.to_string(),
            message,
        }
    }
}

async fn hydrate_cloud_workspace(
    worktree: &Path,
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
    Ok(())
}

async fn materialize_harness_workspace(
    worktree: &Path,
    destination: &Path,
    run_branch: &str,
) -> Result<(), String> {
    copy_workspace_snapshot(
        worktree.to_string_lossy().as_ref(),
        destination.to_string_lossy().as_ref(),
    )?;

    run_git(
        vec![
            "-c".to_string(),
            "init.templateDir=".to_string(),
            "init".to_string(),
        ],
        Some(destination),
        &[],
    )
    .await?;
    run_git(
        vec![
            "symbolic-ref".to_string(),
            "HEAD".to_string(),
            format!("refs/heads/{run_branch}"),
        ],
        Some(destination),
        &[],
    )
    .await?;
    for (key, value) in [
        ("user.name", "ChatOS Task Runner"),
        ("user.email", "task-runner@chatos.local"),
    ] {
        run_git(
            vec!["config".to_string(), key.to_string(), value.to_string()],
            Some(destination),
            &[],
        )
        .await?;
    }
    run_git(
        vec!["add".to_string(), "--all".to_string()],
        Some(destination),
        &[],
    )
    .await?;
    run_git(
        vec![
            "commit".to_string(),
            "--allow-empty".to_string(),
            "--no-gpg-sign".to_string(),
            "-m".to_string(),
            "Initialize platform-managed task workspace".to_string(),
        ],
        Some(destination),
        &[],
    )
    .await
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
    use std::fs;

    use super::{
        harness_temp_dir, is_valid_branch_name, materialize_harness_workspace,
        normalize_run_branch_component, run_git_output,
    };

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

    #[tokio::test]
    async fn materialized_workspace_has_independent_clean_git_metadata() {
        let root = harness_temp_dir("materialize-test", "workspace");
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(source.join(".git")).expect("create source git metadata");
        fs::write(source.join(".git/config"), "secret-source-metadata")
            .expect("write source git metadata");
        fs::write(source.join("tracked.txt"), "baseline\n").expect("write tracked file");

        materialize_harness_workspace(&source, &destination, "chatos/runs/test")
            .await
            .expect("materialize managed workspace");

        assert!(destination.join(".git").is_dir());
        assert!(!fs::read_to_string(destination.join(".git/config"))
            .expect("read managed git config")
            .contains("secret-source-metadata"));
        assert_eq!(
            run_git_output(
                vec!["branch".to_string(), "--show-current".to_string()],
                Some(&destination),
                &[],
            )
            .await
            .expect("read managed branch")
            .trim(),
            "chatos/runs/test"
        );
        assert!(
            run_git_output(vec!["remote".to_string()], Some(&destination), &[])
                .await
                .expect("read managed remotes")
                .trim()
                .is_empty()
        );
        assert!(run_git_output(
            vec!["status".to_string(), "--porcelain".to_string()],
            Some(&destination),
            &[],
        )
        .await
        .expect("read managed status")
        .trim()
        .is_empty());

        fs::remove_file(destination.join("tracked.txt")).expect("delete tracked file");
        assert_eq!(
            run_git_output(
                vec!["diff".to_string(), "--name-status".to_string()],
                Some(&destination),
                &[],
            )
            .await
            .expect("read managed diff")
            .trim(),
            "D\ttracked.txt"
        );

        let _ = fs::remove_dir_all(root);
    }
}
