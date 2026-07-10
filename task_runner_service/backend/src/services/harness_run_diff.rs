// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::models::{
    RunOutputChangesResponse, RunOutputDiffResponse, RunOutputFileChange,
    RunOutputFileChangeCounts, TaskRunRecord,
};

use super::harness_run_git::{
    authenticated_git_url, harness_temp_dir, run_git_output, HarnessRunOutputReport,
};
use super::project_management_api_client;
use super::RunService;

const MAX_PATCH_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
struct HarnessNameStatus {
    path: String,
    status: String,
}

impl RunService {
    pub(super) async fn get_harness_run_output_changes(
        &self,
        run: &TaskRunRecord,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Option<RunOutputChangesResponse>, String> {
        let Some(report) = harness_output_report_from_run(run)? else {
            return Ok(None);
        };
        if report.status == "no_changes" {
            return Ok(Some(empty_changes_response(run, limit, offset)));
        }
        if report.status != "committed" {
            return Ok(None);
        }
        let access = project_management_api_client::get_project_harness_git_access(
            &self.config,
            report.project_id.as_str(),
        )
        .await?;
        let temp_root = harness_temp_dir(run.id.as_str(), "diff-list");
        let worktree = temp_root.join("repo");
        fs::create_dir_all(&temp_root).map_err(|err| err.to_string())?;
        let authenticated_url = authenticated_git_url(&access)?;
        let secrets = [access.access_token.as_str()];
        let result = async {
            clone_run_branch(
                worktree.as_path(),
                authenticated_url,
                report.run_branch.as_str(),
                &secrets,
            )
            .await?;
            let range = harness_diff_range(&report);
            let name_status = run_git_output(
                vec![
                    "diff".to_string(),
                    "--name-status".to_string(),
                    "--find-renames".to_string(),
                    range.clone(),
                ],
                Some(worktree.as_path()),
                &secrets,
            )
            .await?;
            let numstat = run_git_output(
                vec![
                    "diff".to_string(),
                    "--numstat".to_string(),
                    "--find-renames".to_string(),
                    range,
                ],
                Some(worktree.as_path()),
                &secrets,
            )
            .await?;
            let statuses = parse_name_status(name_status.as_str());
            let stats = parse_numstat(numstat.as_str(), statuses.as_slice());
            let files = statuses
                .into_iter()
                .map(|item| {
                    let (added_lines, deleted_lines, binary) = stats
                        .get(item.path.as_str())
                        .copied()
                        .unwrap_or((0, 0, false));
                    RunOutputFileChange {
                        path: item.path,
                        status: item.status,
                        old_size: None,
                        new_size: None,
                        old_sha256: None,
                        new_sha256: None,
                        added_lines,
                        deleted_lines,
                        binary,
                        diff_available: !binary,
                        diff_truncated: false,
                        diff_ref: None,
                    }
                })
                .collect::<Vec<_>>();
            let counts = count_file_changes(files.as_slice());
            let total = files.len();
            let limit = limit.unwrap_or(100).clamp(1, 500);
            let offset = offset.unwrap_or(0);
            let files = files
                .into_iter()
                .skip(offset)
                .take(limit)
                .collect::<Vec<_>>();
            Ok(RunOutputChangesResponse {
                run_id: run.id.clone(),
                counts,
                files,
                total,
                limit,
                offset,
                has_more: offset.saturating_add(limit) < total,
            })
        }
        .await;
        let _ = fs::remove_dir_all(&temp_root);
        result.map(Some)
    }

    pub(super) async fn get_harness_run_output_diff(
        &self,
        run: &TaskRunRecord,
        path: &str,
    ) -> Result<Option<RunOutputDiffResponse>, String> {
        let Some(report) = harness_output_report_from_run(run)? else {
            return Ok(None);
        };
        if report.status != "committed" && report.status != "no_changes" {
            return Ok(None);
        }
        let normalized_path = normalize_relative_path(path)?;
        if report.status == "no_changes" {
            return Err("文件不在本次运行变更清单中".to_string());
        }
        let access = project_management_api_client::get_project_harness_git_access(
            &self.config,
            report.project_id.as_str(),
        )
        .await?;
        let temp_root = harness_temp_dir(run.id.as_str(), "diff-file");
        let worktree = temp_root.join("repo");
        fs::create_dir_all(&temp_root).map_err(|err| err.to_string())?;
        let authenticated_url = authenticated_git_url(&access)?;
        let secrets = [access.access_token.as_str()];
        let result = async {
            clone_run_branch(
                worktree.as_path(),
                authenticated_url,
                report.run_branch.as_str(),
                &secrets,
            )
            .await?;
            let range = harness_diff_range(&report);
            let name_status = run_git_output(
                vec![
                    "diff".to_string(),
                    "--name-status".to_string(),
                    "--find-renames".to_string(),
                    range.clone(),
                ],
                Some(worktree.as_path()),
                &secrets,
            )
            .await?;
            let statuses = parse_name_status(name_status.as_str());
            let change = statuses
                .iter()
                .find(|item| item.path == normalized_path)
                .ok_or_else(|| "文件不在本次运行变更清单中".to_string())?;
            let numstat = run_git_output(
                vec![
                    "diff".to_string(),
                    "--numstat".to_string(),
                    range.clone(),
                    "--".to_string(),
                    normalized_path.clone(),
                ],
                Some(worktree.as_path()),
                &secrets,
            )
            .await?;
            let stats = parse_numstat(numstat.as_str(), std::slice::from_ref(change));
            let binary = stats
                .get(normalized_path.as_str())
                .map(|value| value.2)
                .unwrap_or(false);
            if binary {
                return Ok(RunOutputDiffResponse {
                    run_id: run.id.clone(),
                    path: normalized_path,
                    status: change.status.clone(),
                    patch: None,
                    binary: true,
                    diff_available: false,
                    diff_truncated: false,
                    message: Some("二进制文件不提供文本 diff。".to_string()),
                });
            }
            let patch = run_git_output(
                vec![
                    "diff".to_string(),
                    "--binary".to_string(),
                    "--find-renames".to_string(),
                    range,
                    "--".to_string(),
                    normalized_path.clone(),
                ],
                Some(worktree.as_path()),
                &secrets,
            )
            .await?;
            let (patch, diff_truncated) = truncate_patch(patch);
            Ok(RunOutputDiffResponse {
                run_id: run.id.clone(),
                path: normalized_path,
                status: change.status.clone(),
                patch: Some(patch),
                binary: false,
                diff_available: true,
                diff_truncated,
                message: diff_truncated.then(|| "diff 内容过大，已截断。".to_string()),
            })
        }
        .await;
        let _ = fs::remove_dir_all(&temp_root);
        result.map(Some)
    }
}

async fn clone_run_branch(
    worktree: &Path,
    authenticated_url: String,
    run_branch: &str,
    secrets: &[&str],
) -> Result<(), String> {
    run_git_output(
        vec![
            "clone".to_string(),
            "--branch".to_string(),
            run_branch.to_string(),
            "--single-branch".to_string(),
            "--no-checkout".to_string(),
            authenticated_url,
            worktree.to_string_lossy().to_string(),
        ],
        None,
        secrets,
    )
    .await
    .map(|_| ())
}

fn harness_output_report_from_run(
    run: &TaskRunRecord,
) -> Result<Option<HarnessRunOutputReport>, String> {
    let Some(value) = run
        .report
        .as_ref()
        .and_then(|report| report.pointer("/output/harness"))
    else {
        return Ok(None);
    };
    serde_json::from_value::<HarnessRunOutputReport>(value.clone())
        .map(Some)
        .map_err(|err| format!("解析 Harness 输出摘要失败: {err}"))
}

fn harness_diff_range(report: &HarnessRunOutputReport) -> String {
    format!("{}..HEAD", report.base_commit)
}

fn parse_name_status(output: &str) -> Vec<HarnessNameStatus> {
    output
        .lines()
        .filter_map(|line| {
            let parts = line.split('\t').collect::<Vec<_>>();
            let code = parts.first()?.trim();
            let path = if code.starts_with('R') || code.starts_with('C') {
                parts.get(2).copied().or_else(|| parts.get(1).copied())?
            } else {
                parts.get(1).copied()?
            };
            let status = match code.chars().next().unwrap_or('M') {
                'A' => "added",
                'D' => "deleted",
                _ => "modified",
            };
            Some(HarnessNameStatus {
                path: path.to_string(),
                status: status.to_string(),
            })
        })
        .collect()
}

fn parse_numstat(
    output: &str,
    changes: &[HarnessNameStatus],
) -> HashMap<String, (usize, usize, bool)> {
    let mut result = HashMap::new();
    for line in output.lines() {
        let mut parts = line.splitn(3, '\t');
        let added = parts.next().unwrap_or_default();
        let deleted = parts.next().unwrap_or_default();
        let raw_path = parts.next().unwrap_or_default();
        let binary = added == "-" || deleted == "-";
        let added_lines = added.parse::<usize>().unwrap_or(0);
        let deleted_lines = deleted.parse::<usize>().unwrap_or(0);
        let path = changes
            .iter()
            .find(|change| raw_path == change.path || raw_path.contains(change.path.as_str()))
            .map(|change| change.path.clone())
            .unwrap_or_else(|| raw_path.to_string());
        result.insert(path, (added_lines, deleted_lines, binary));
    }
    result
}

fn count_file_changes(files: &[RunOutputFileChange]) -> RunOutputFileChangeCounts {
    let mut counts = RunOutputFileChangeCounts::default();
    for file in files {
        match file.status.as_str() {
            "added" => counts.added += 1,
            "deleted" => counts.deleted += 1,
            _ => counts.modified += 1,
        }
        if file.binary {
            counts.binary += 1;
        }
        if file.diff_available {
            counts.diff_available += 1;
        }
    }
    counts.total = files.len();
    counts
}

fn empty_changes_response(
    run: &TaskRunRecord,
    limit: Option<usize>,
    offset: Option<usize>,
) -> RunOutputChangesResponse {
    RunOutputChangesResponse {
        run_id: run.id.clone(),
        counts: RunOutputFileChangeCounts::default(),
        files: Vec::new(),
        total: 0,
        limit: limit.unwrap_or(100).clamp(1, 500),
        offset: offset.unwrap_or(0),
        has_more: false,
    }
}

fn normalize_relative_path(path: &str) -> Result<String, String> {
    let path = path.trim().replace('\\', "/");
    if path.is_empty() || Path::new(path.as_str()).is_absolute() {
        return Err("文件路径无效".to_string());
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err("文件路径不能包含 ..".to_string()),
            value => parts.push(value),
        }
    }
    if parts.is_empty() {
        return Err("文件路径不能为空".to_string());
    }
    Ok(parts.join("/"))
}

fn truncate_patch(mut patch: String) -> (String, bool) {
    if patch.len() <= MAX_PATCH_BYTES {
        return (patch, false);
    }
    let mut boundary = MAX_PATCH_BYTES;
    while boundary > 0 && !patch.is_char_boundary(boundary) {
        boundary -= 1;
    }
    patch.truncate(boundary);
    (patch, true)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use uuid::Uuid;

    use super::{clone_run_branch, count_file_changes, parse_name_status, parse_numstat};
    use crate::models::RunOutputFileChange;
    use crate::services::harness_run_git::{
        commit_workspace_to_run_branch, create_snapshot_commit_and_push, run_git_output,
    };
    use crate::services::workspace_snapshot::copy_workspace_snapshot;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "chatos-harness-diff-test-{label}-{}",
                Uuid::new_v4()
            ));
            fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            self.0.as_path()
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn parses_git_name_status_and_numstat() {
        let changes = parse_name_status("A\tsrc/new.rs\nM\tsrc/main.rs\nD\told.txt\n");
        let stats = parse_numstat(
            "5\t0\tsrc/new.rs\n2\t1\tsrc/main.rs\n0\t4\told.txt\n",
            &changes,
        );
        assert_eq!(changes.len(), 3);
        assert_eq!(stats.get("src/main.rs"), Some(&(2, 1, false)));
    }

    #[test]
    fn counts_binary_and_diff_available_files() {
        let files = vec![RunOutputFileChange {
            path: "asset.bin".to_string(),
            status: "added".to_string(),
            old_size: None,
            new_size: None,
            old_sha256: None,
            new_sha256: None,
            added_lines: 0,
            deleted_lines: 0,
            binary: true,
            diff_available: false,
            diff_truncated: false,
            diff_ref: None,
        }];
        let counts = count_file_changes(&files);
        assert_eq!(counts.added, 1);
        assert_eq!(counts.binary, 1);
        assert_eq!(counts.diff_available, 0);
    }

    #[tokio::test]
    async fn bare_repo_round_trip_commits_and_parses_sandbox_changes() {
        let root = TestDirectory::new("round-trip");
        let source = root.path().join("local-project");
        let output = root.path().join("sandbox-output");
        let bare_repo = root.path().join("harness.git");
        let prepare_worktree = root.path().join("prepare");
        let commit_worktree = root.path().join("commit");
        let no_change_worktree = root.path().join("no-change");
        let diff_worktree = root.path().join("diff");
        fs::create_dir_all(&source).expect("create local project");
        fs::write(source.join("modified.txt"), "before\n").expect("write modified baseline");
        fs::write(source.join("deleted.txt"), "delete me\n").expect("write deleted baseline");
        fs::write(source.join("unchanged.txt"), "stable\n").expect("write unchanged baseline");

        run_git_output(
            vec!["init".to_string(), source.to_string_lossy().to_string()],
            None,
            &[],
        )
        .await
        .expect("initialize user's local git repository");
        run_git_output(
            vec![
                "checkout".to_string(),
                "-b".to_string(),
                "feature/local-work".to_string(),
            ],
            Some(source.as_path()),
            &[],
        )
        .await
        .expect("create user's local branch");
        run_git_output(
            vec![
                "remote".to_string(),
                "add".to_string(),
                "origin".to_string(),
                "https://example.invalid/user/project.git".to_string(),
            ],
            Some(source.as_path()),
            &[],
        )
        .await
        .expect("configure user's own remote");
        let original_git_config =
            fs::read_to_string(source.join(".git/config")).expect("read original git config");

        run_git_output(
            vec![
                "init".to_string(),
                "--bare".to_string(),
                bare_repo.to_string_lossy().to_string(),
            ],
            None,
            &[],
        )
        .await
        .expect("initialize temporary Harness repository");
        run_git_output(
            vec![
                "clone".to_string(),
                "--no-checkout".to_string(),
                bare_repo.to_string_lossy().to_string(),
                prepare_worktree.to_string_lossy().to_string(),
            ],
            None,
            &[],
        )
        .await
        .expect("clone temporary Harness repository");

        let run_branch = "chatos/runs/test-run";
        let base_commit = create_snapshot_commit_and_push(
            source.to_string_lossy().as_ref(),
            prepare_worktree.as_path(),
            "feature/local-work",
            run_branch,
            "snapshot before test run",
            &[],
        )
        .await
        .expect("push execution snapshot");

        copy_workspace_snapshot(
            source.to_string_lossy().as_ref(),
            output.to_string_lossy().as_ref(),
        )
        .expect("copy sandbox input");
        fs::write(output.join("modified.txt"), "after\nmore\n").expect("modify text file");
        fs::write(output.join("added.txt"), "added\n").expect("add text file");
        fs::remove_file(output.join("deleted.txt")).expect("delete text file");
        fs::write(output.join("asset.bin"), [0_u8, 159, 146, 150, 255]).expect("add binary file");

        let (status, result_commit) = commit_workspace_to_run_branch(
            bare_repo.to_string_lossy().to_string(),
            commit_worktree.as_path(),
            run_branch,
            output.to_string_lossy().as_ref(),
            "apply sandbox output",
            &[],
        )
        .await
        .expect("commit sandbox output");
        assert_eq!(status, "committed");
        assert_ne!(result_commit, base_commit);

        let (no_change_status, no_change_commit) = commit_workspace_to_run_branch(
            bare_repo.to_string_lossy().to_string(),
            no_change_worktree.as_path(),
            run_branch,
            output.to_string_lossy().as_ref(),
            "should not create an empty commit",
            &[],
        )
        .await
        .expect("detect unchanged sandbox output");
        assert_eq!(no_change_status, "no_changes");
        assert_eq!(no_change_commit, result_commit);

        clone_run_branch(
            diff_worktree.as_path(),
            bare_repo.to_string_lossy().to_string(),
            run_branch,
            &[],
        )
        .await
        .expect("clone run branch for diff");
        let range = format!("{base_commit}..HEAD");
        let name_status = run_git_output(
            vec![
                "diff".to_string(),
                "--name-status".to_string(),
                range.clone(),
            ],
            Some(diff_worktree.as_path()),
            &[],
        )
        .await
        .expect("read name status");
        let numstat = run_git_output(
            vec!["diff".to_string(), "--numstat".to_string(), range],
            Some(diff_worktree.as_path()),
            &[],
        )
        .await
        .expect("read numstat");
        let changes = parse_name_status(name_status.as_str());
        let stats = parse_numstat(numstat.as_str(), changes.as_slice());
        let statuses = changes
            .iter()
            .map(|change| (change.path.as_str(), change.status.as_str()))
            .collect::<HashMap<_, _>>();

        assert_eq!(statuses.get("added.txt"), Some(&"added"));
        assert_eq!(statuses.get("modified.txt"), Some(&"modified"));
        assert_eq!(statuses.get("deleted.txt"), Some(&"deleted"));
        assert_eq!(statuses.get("asset.bin"), Some(&"added"));
        assert_eq!(stats.get("modified.txt"), Some(&(2, 1, false)));
        assert_eq!(stats.get("deleted.txt"), Some(&(0, 1, false)));
        assert_eq!(stats.get("asset.bin"), Some(&(0, 0, true)));

        let base_branch_commit = run_git_output(
            vec![
                "--git-dir".to_string(),
                bare_repo.to_string_lossy().to_string(),
                "rev-parse".to_string(),
                "refs/heads/feature/local-work".to_string(),
            ],
            None,
            &[],
        )
        .await
        .expect("read base branch commit");
        assert_eq!(base_branch_commit.trim(), base_commit);
        assert_eq!(
            fs::read_to_string(source.join(".git/config")).expect("read local git config"),
            original_git_config
        );
        let local_branch = run_git_output(
            vec![
                "symbolic-ref".to_string(),
                "--short".to_string(),
                "HEAD".to_string(),
            ],
            Some(source.as_path()),
            &[],
        )
        .await
        .expect("read user's local branch");
        assert_eq!(local_branch.trim(), "feature/local-work");
    }
}
