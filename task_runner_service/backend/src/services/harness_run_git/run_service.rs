// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RunService {
    pub(in crate::services) async fn prepare_harness_run_for_sandbox(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
        effective_workspace_dir: &str,
    ) -> Result<Option<HarnessRunContext>, String> {
        let project_id = crate::models::normalize_project_id(Some(task.project_id.clone()));
        if project_id == crate::models::PUBLIC_PROJECT_ID
            || !project_management_api_client::project_service_enabled(&self.config)
        {
            return Ok(None);
        }
        let project = match project_management_api_client::sync_get_project(
            &self.config,
            project_id.as_str(),
        )
        .await
        {
            Ok(Some(project)) => project,
            Ok(None) => {
                return Err(format!(
                    "Harness project context is unavailable: {project_id}"
                ))
            }
            Err(err) => {
                self.append_harness_prepare_failure(run, err.clone()).await;
                return Err(err);
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
                Ok(Some(context))
            }
            Err(err) => {
                self.append_harness_prepare_failure(run, err.clone()).await;
                Err(err)
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
            let base_commit = if is_cloud {
                create_cloud_run_branch(
                    worktree.as_path(),
                    base_branch.as_str(),
                    run_branch.as_str(),
                    &secrets,
                )
                .await?
            } else {
                let commit_message = format!("Sync project snapshot before run {}", run.id);
                create_snapshot_commit_and_push(
                    workspace_dir.as_str(),
                    worktree.as_path(),
                    base_branch.as_str(),
                    run_branch.as_str(),
                    commit_message.as_str(),
                    &secrets,
                )
                .await?
            };
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
                    "准备 Harness 运行分支失败，任务不会降级到其他工作区: {error}"
                )),
                None,
            ))
            .await;
    }

    pub(in crate::services) async fn commit_harness_run_output(
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
                let status = if is_harness_concurrent_merge_conflict(err.as_str()) {
                    "merge_conflict"
                } else {
                    "failed"
                };
                let report = context.output_report(status, None, None, Some(err.clone()));
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
            let promoted_commit = if status == "committed" {
                Some(
                    promote_run_branch_to_base(
                        worktree.as_path(),
                        context.base_branch.as_str(),
                        context.base_commit.as_str(),
                        &secrets,
                    )
                    .await?,
                )
            } else {
                None
            };
            Ok(context.output_report(status.as_str(), Some(result_commit), promoted_commit, None))
        }
        .await;
        let _ = fs::remove_dir_all(&temp_root);
        result
    }

    pub(in crate::services) fn cleanup_harness_run_workspace(&self, context: &HarnessRunContext) {
        if let Some(root) = context.owned_workspace_root.as_deref() {
            let _ = fs::remove_dir_all(root);
        }
    }
}

pub(in crate::services) async fn create_cloud_run_branch(
    worktree: &Path,
    base_branch: &str,
    run_branch: &str,
    secrets: &[&str],
) -> Result<String, String> {
    let remote_ref = format!("refs/remotes/origin/{base_branch}");
    let base_commit = run_git_output(
        vec!["rev-parse".to_string(), "--verify".to_string(), remote_ref],
        Some(worktree),
        secrets,
    )
    .await
    .map_err(|error| format!("Harness base branch is unavailable: {base_branch}: {error}"))?
    .trim()
    .to_string();
    if base_commit.is_empty() {
        return Err(format!("Harness base branch is empty: {base_branch}"));
    }
    run_git(
        vec![
            "push".to_string(),
            "origin".to_string(),
            "--force".to_string(),
            format!("{base_commit}:refs/heads/{run_branch}"),
        ],
        Some(worktree),
        secrets,
    )
    .await?;
    Ok(base_commit)
}

pub(in crate::services) async fn create_snapshot_commit_and_push(
    workspace_dir: &str,
    worktree: &Path,
    base_branch: &str,
    run_branch: &str,
    commit_message: &str,
    secrets: &[&str],
) -> Result<String, String> {
    let expected_base_commit = run_git_output(
        vec![
            "rev-parse".to_string(),
            "--verify".to_string(),
            format!("refs/remotes/origin/{base_branch}"),
        ],
        Some(worktree),
        secrets,
    )
    .await
    .ok()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty());
    replace_git_worktree_with_workspace(workspace_dir, worktree)?;
    let snapshot_branch = format!("chatos-snapshot-{}", Uuid::new_v4().simple());
    let base_commit =
        create_orphan_commit(worktree, snapshot_branch.as_str(), commit_message, secrets).await?;
    let base_lease = expected_base_commit
        .map(|commit| format!("--force-with-lease=refs/heads/{base_branch}:{commit}"))
        .unwrap_or_else(|| format!("--force-with-lease=refs/heads/{base_branch}:"));
    run_git(
        vec![
            "push".to_string(),
            "origin".to_string(),
            base_lease,
            format!("HEAD:refs/heads/{base_branch}"),
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

async fn create_orphan_commit(
    worktree: &Path,
    branch: &str,
    commit_message: &str,
    secrets: &[&str],
) -> Result<String, String> {
    run_git(
        vec![
            "checkout".to_string(),
            "--orphan".to_string(),
            branch.to_string(),
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
    Ok(base_commit)
}

pub(in crate::services) async fn commit_workspace_to_run_branch(
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

const HARNESS_CONCURRENT_MERGE_CONFLICT: &str = "harness concurrent merge conflict";
const HARNESS_PROMOTION_MAX_ATTEMPTS: usize = 8;

fn is_harness_concurrent_merge_conflict(error: &str) -> bool {
    error.contains(HARNESS_CONCURRENT_MERGE_CONFLICT)
}

pub(in crate::services) async fn promote_run_branch_to_base(
    worktree: &Path,
    base_branch: &str,
    expected_base_commit: &str,
    secrets: &[&str],
) -> Result<String, String> {
    let remote_ref = format!("refs/remotes/origin/{base_branch}");
    let mut rebased_onto = expected_base_commit.to_string();
    let mut last_push_error = None;

    for _ in 0..HARNESS_PROMOTION_MAX_ATTEMPTS {
        run_git(
            vec![
                "fetch".to_string(),
                "origin".to_string(),
                format!("+refs/heads/{base_branch}:{remote_ref}"),
            ],
            Some(worktree),
            secrets,
        )
        .await?;
        let latest_base_commit = run_git_output(
            vec![
                "rev-parse".to_string(),
                "--verify".to_string(),
                remote_ref.clone(),
            ],
            Some(worktree),
            secrets,
        )
        .await?
        .trim()
        .to_string();

        if latest_base_commit != rebased_onto {
            let rebase_result = run_git(
                vec![
                    "-c".to_string(),
                    "user.name=Chatos Task Runner".to_string(),
                    "-c".to_string(),
                    "user.email=task-runner@chatos.local".to_string(),
                    "rebase".to_string(),
                    "--onto".to_string(),
                    latest_base_commit.clone(),
                    rebased_onto.clone(),
                ],
                Some(worktree),
                secrets,
            )
            .await;
            if let Err(error) = rebase_result {
                let _ = run_git(
                    vec!["rebase".to_string(), "--abort".to_string()],
                    Some(worktree),
                    secrets,
                )
                .await;
                return Err(format!("{HARNESS_CONCURRENT_MERGE_CONFLICT}: {error}"));
            }
            rebased_onto = latest_base_commit.clone();
        }

        let promoted_commit = run_git_output(
            vec!["rev-parse".to_string(), "HEAD".to_string()],
            Some(worktree),
            secrets,
        )
        .await?
        .trim()
        .to_string();
        match run_git(
            vec![
                "push".to_string(),
                "origin".to_string(),
                format!("--force-with-lease=refs/heads/{base_branch}:{latest_base_commit}"),
                format!("HEAD:refs/heads/{base_branch}"),
            ],
            Some(worktree),
            secrets,
        )
        .await
        {
            Ok(()) => return Ok(promoted_commit),
            Err(error) => {
                last_push_error = Some(error);
            }
        }
    }

    Err(format!(
        "Harness base branch changed during all {HARNESS_PROMOTION_MAX_ATTEMPTS} promotion attempts: {}",
        last_push_error.unwrap_or_else(|| "unknown git push failure".to_string())
    ))
}
