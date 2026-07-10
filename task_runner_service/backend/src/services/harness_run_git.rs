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

impl RunService {
    pub(super) async fn prepare_harness_run_for_sandbox(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
        effective_workspace_dir: &str,
    ) -> Option<HarnessRunContext> {
        let project_id = crate::models::normalize_project_id(Some(task.project_id.clone()));
        if project_id == crate::models::PUBLIC_PROJECT_ID
            || !project_management_api_client::project_service_enabled(&self.config)
        {
            return None;
        }
        let project = match project_management_api_client::sync_get_project(
            &self.config,
            project_id.as_str(),
        )
        .await
        {
            Ok(Some(project)) => project,
            Ok(None) => return None,
            Err(err) => {
                self.append_harness_prepare_failure(run, err).await;
                return None;
            }
        };
        match self
            .prepare_harness_run_inner(&project, run, effective_workspace_dir)
            .await
        {
            Ok(context) => {
                if let Some(object) = run.input_snapshot.as_object_mut() {
                    object.insert("harness".to_string(), context.to_metadata());
                    object.insert(
                        "effective_workspace_dir".to_string(),
                        serde_json::Value::String(context.effective_workspace_dir.clone()),
                    );
                }
                run.updated_at = crate::models::now_rfc3339();
                if let Err(err) = self.store.save_run(run.clone()).await {
                    warn!(
                        run_id = run.id.as_str(),
                        error = err.as_str(),
                        "persist Harness run context failed"
                    );
                }
                if let Err(err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "harness_run_prepared",
                        Some(format!("Harness 运行分支已准备: {}", context.run_branch)),
                        Some(context.to_metadata()),
                    ))
                    .await
                {
                    warn!(
                        run_id = run.id.as_str(),
                        error = err.as_str(),
                        "append Harness prepared event failed"
                    );
                }
                Some(context)
            }
            Err(err) => {
                self.append_harness_prepare_failure(run, err).await;
                None
            }
        }
    }

    async fn prepare_harness_run_inner(
        &self,
        project: &TaskProjectRecord,
        run: &TaskRunRecord,
        effective_workspace_dir: &str,
    ) -> Result<HarnessRunContext, String> {
        let access = project_management_api_client::get_project_harness_git_access(
            &self.config,
            project.id.as_str(),
        )
        .await?;
        validate_git_access(project, &access)?;
        let temp_root = harness_temp_dir(run.id.as_str(), "prepare");
        let worktree = temp_root.join("repo");
        fs::create_dir_all(&temp_root).map_err(|err| err.to_string())?;
        let authenticated_url = authenticated_git_url(&access)?;
        let secrets = [access.access_token.as_str()];
        let result = async {
            run_git(
                vec![
                    "clone".to_string(),
                    "--no-checkout".to_string(),
                    authenticated_url,
                    worktree.to_string_lossy().to_string(),
                ],
                None,
                &secrets,
            )
            .await?;

            let is_cloud = project
                .source_type
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| value.eq_ignore_ascii_case("cloud"));
            let (workspace_dir, owned_workspace_root) = if is_cloud {
                let snapshot_root = harness_temp_dir(run.id.as_str(), "cloud-workspace");
                let snapshot_workspace = snapshot_root.join("workspace");
                fs::create_dir_all(&snapshot_workspace).map_err(|err| err.to_string())?;
                hydrate_cloud_workspace(
                    worktree.as_path(),
                    snapshot_workspace.as_path(),
                    access.default_branch.as_str(),
                    &secrets,
                )
                .await?;
                (
                    snapshot_workspace.to_string_lossy().to_string(),
                    Some(snapshot_root.to_string_lossy().to_string()),
                )
            } else {
                (effective_workspace_dir.to_string(), None)
            };

            let base_branch = if is_cloud {
                normalize_branch_name(access.default_branch.as_str(), "main")
            } else {
                resolve_workspace_branch(workspace_dir.as_str(), access.default_branch.as_str())
                    .await
            };
            let run_branch = format!("chatos/runs/{}", normalize_run_branch_component(&run.id));
            let commit_message = format!("Sync project snapshot before run {}", run.id);
            let base_commit = create_snapshot_commit_and_push(
                workspace_dir.as_str(),
                worktree.as_path(),
                base_branch.as_str(),
                run_branch.as_str(),
                commit_message.as_str(),
                &secrets,
            )
            .await?;
            Ok(HarnessRunContext {
                project_id: project.id.clone(),
                repo_path: access.repo_path.clone(),
                git_url: access.git_url.clone(),
                base_branch,
                run_branch,
                base_commit,
                effective_workspace_dir: workspace_dir,
                owned_workspace_root,
            })
        }
        .await;
        let _ = fs::remove_dir_all(&temp_root);
        result
    }

    async fn append_harness_prepare_failure(&self, run: &TaskRunRecord, error: String) {
        warn!(
            run_id = run.id.as_str(),
            error = error.as_str(),
            "prepare Harness run branch failed"
        );
        let _ = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "harness_run_prepare_failed",
                Some(format!(
                    "准备 Harness 运行分支失败，继续使用沙箱 manifest: {error}"
                )),
                None,
            ))
            .await;
    }

    pub(super) async fn commit_harness_run_output(
        &self,
        run: &TaskRunRecord,
        context: &HarnessRunContext,
        output_workspace: Option<&str>,
    ) -> HarnessRunOutputReport {
        let result = match output_workspace
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(workspace) => {
                self.commit_harness_run_output_inner(run, context, workspace)
                    .await
            }
            None => Err("sandbox output workspace is unavailable".to_string()),
        };
        match result {
            Ok(report) => {
                let event_type = if report.status == "no_changes" {
                    "harness_output_no_changes"
                } else {
                    "harness_output_committed"
                };
                let _ = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        event_type,
                        Some(format!("Harness 运行分支已更新: {}", context.run_branch)),
                        serde_json::to_value(&report).ok(),
                    ))
                    .await;
                report
            }
            Err(err) => {
                warn!(
                    run_id = run.id.as_str(),
                    error = err.as_str(),
                    "commit sandbox output to Harness failed"
                );
                let report = context.output_report("failed", None, Some(err.clone()));
                let _ = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "harness_output_commit_failed",
                        Some(format!("提交沙箱输出到 Harness 失败: {err}")),
                        serde_json::to_value(&report).ok(),
                    ))
                    .await;
                report
            }
        }
    }

    async fn commit_harness_run_output_inner(
        &self,
        run: &TaskRunRecord,
        context: &HarnessRunContext,
        output_workspace: &str,
    ) -> Result<HarnessRunOutputReport, String> {
        let access = project_management_api_client::get_project_harness_git_access(
            &self.config,
            context.project_id.as_str(),
        )
        .await?;
        let temp_root = harness_temp_dir(run.id.as_str(), "commit");
        let worktree = temp_root.join("repo");
        fs::create_dir_all(&temp_root).map_err(|err| err.to_string())?;
        let authenticated_url = authenticated_git_url(&access)?;
        let secrets = [access.access_token.as_str()];
        let result = async {
            let commit_message = format!("Apply sandbox output for run {}", run.id);
            let (status, result_commit) = commit_workspace_to_run_branch(
                authenticated_url,
                worktree.as_path(),
                context.run_branch.as_str(),
                output_workspace,
                commit_message.as_str(),
                &secrets,
            )
            .await?;
            Ok(context.output_report(status.as_str(), Some(result_commit), None))
        }
        .await;
        let _ = fs::remove_dir_all(&temp_root);
        result
    }

    pub(super) fn cleanup_harness_run_workspace(&self, context: &HarnessRunContext) {
        if let Some(root) = context.owned_workspace_root.as_deref() {
            let _ = fs::remove_dir_all(root);
        }
    }
}

pub(super) async fn create_snapshot_commit_and_push(
    workspace_dir: &str,
    worktree: &Path,
    base_branch: &str,
    run_branch: &str,
    commit_message: &str,
    secrets: &[&str],
) -> Result<String, String> {
    replace_git_worktree_with_workspace(workspace_dir, worktree)?;
    let snapshot_branch = format!("chatos-snapshot-{}", Uuid::new_v4().simple());
    run_git(
        vec![
            "checkout".to_string(),
            "--orphan".to_string(),
            snapshot_branch,
        ],
        Some(worktree),
        secrets,
    )
    .await?;
    let _ = run_git(
        vec![
            "rm".to_string(),
            "-r".to_string(),
            "--cached".to_string(),
            "--ignore-unmatch".to_string(),
            ".".to_string(),
        ],
        Some(worktree),
        secrets,
    )
    .await;
    run_git(
        vec!["add".to_string(), "-A".to_string()],
        Some(worktree),
        secrets,
    )
    .await?;
    run_git(
        vec![
            "-c".to_string(),
            "user.name=Chatos Task Runner".to_string(),
            "-c".to_string(),
            "user.email=task-runner@chatos.local".to_string(),
            "commit".to_string(),
            "--allow-empty".to_string(),
            "-m".to_string(),
            commit_message.to_string(),
        ],
        Some(worktree),
        secrets,
    )
    .await?;
    let base_commit = run_git_output(
        vec!["rev-parse".to_string(), "HEAD".to_string()],
        Some(worktree),
        secrets,
    )
    .await?
    .trim()
    .to_string();
    run_git(
        vec![
            "push".to_string(),
            "origin".to_string(),
            format!("HEAD:refs/heads/{base_branch}"),
            "--force".to_string(),
        ],
        Some(worktree),
        secrets,
    )
    .await?;
    run_git(
        vec![
            "push".to_string(),
            "origin".to_string(),
            format!("HEAD:refs/heads/{run_branch}"),
            "--force".to_string(),
        ],
        Some(worktree),
        secrets,
    )
    .await?;
    Ok(base_commit)
}

pub(super) async fn commit_workspace_to_run_branch(
    authenticated_url: String,
    worktree: &Path,
    run_branch: &str,
    output_workspace: &str,
    commit_message: &str,
    secrets: &[&str],
) -> Result<(String, String), String> {
    run_git(
        vec![
            "clone".to_string(),
            "--branch".to_string(),
            run_branch.to_string(),
            "--single-branch".to_string(),
            authenticated_url,
            worktree.to_string_lossy().to_string(),
        ],
        None,
        secrets,
    )
    .await?;
    replace_git_worktree_with_workspace(output_workspace, worktree)?;
    run_git(
        vec!["add".to_string(), "-A".to_string()],
        Some(worktree),
        secrets,
    )
    .await?;
    let status = run_git_output(
        vec!["status".to_string(), "--porcelain".to_string()],
        Some(worktree),
        secrets,
    )
    .await?;
    if status.trim().is_empty() {
        let result_commit = run_git_output(
            vec!["rev-parse".to_string(), "HEAD".to_string()],
            Some(worktree),
            secrets,
        )
        .await?
        .trim()
        .to_string();
        return Ok(("no_changes".to_string(), result_commit));
    }
    run_git(
        vec![
            "-c".to_string(),
            "user.name=Chatos Task Runner".to_string(),
            "-c".to_string(),
            "user.email=task-runner@chatos.local".to_string(),
            "commit".to_string(),
            "-m".to_string(),
            commit_message.to_string(),
        ],
        Some(worktree),
        secrets,
    )
    .await?;
    let result_commit = run_git_output(
        vec!["rev-parse".to_string(), "HEAD".to_string()],
        Some(worktree),
        secrets,
    )
    .await?
    .trim()
    .to_string();
    run_git(
        vec![
            "push".to_string(),
            "origin".to_string(),
            format!("HEAD:refs/heads/{run_branch}"),
        ],
        Some(worktree),
        secrets,
    )
    .await?;
    Ok(("committed".to_string(), result_commit))
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
