use std::path::Path;

use super::contracts::{GitActionResult, GitSummary};
use super::parsing::{compact_output, summary_from_status};
use super::process::{
    git_output, GitCommandOutput, GitCommandStatusOutput, DEFAULT_GIT_TIMEOUT,
};
use super::validation::ensure_safe_ref;

pub(super) async fn read_repo_summary(repo_root: &Path) -> Result<GitSummary, String> {
    let status = git_output(
        repo_root,
        [
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
        ],
        DEFAULT_GIT_TIMEOUT,
    )
    .await?;
    Ok(summary_from_status(repo_root.to_path_buf(), &status.stdout))
}

pub(super) async fn comparison_range(
    repo_root: &Path,
    target: &str,
) -> Result<(String, String, String), String> {
    ensure_safe_ref(target, "target")?;
    let current = current_branch_or_head(repo_root).await?;
    let range = format!("{}...{}", current, target);
    Ok((current, target.to_string(), range))
}

pub(super) async fn current_branch_or_head(repo_root: &Path) -> Result<String, String> {
    let current = read_repo_summary(repo_root)
        .await?
        .current_branch
        .unwrap_or_else(|| "HEAD".to_string());
    ensure_safe_ref(current.as_str(), "current")?;
    Ok(current)
}

pub(super) async fn require_current_branch(
    repo_root: &Path,
    action: &str,
) -> Result<String, String> {
    read_repo_summary(repo_root)
        .await?
        .current_branch
        .ok_or_else(|| format!("当前不是分支状态，无法 {}", action))
}

pub(super) async fn stage_paths(
    repo_root: &Path,
    paths: &[String],
) -> Result<GitCommandOutput, String> {
    path_command_output(repo_root, &["add"], paths).await
}

pub(super) async fn unstage_paths(
    repo_root: &Path,
    paths: &[String],
) -> Result<GitCommandOutput, String> {
    path_command_output(repo_root, &["restore", "--staged"], paths).await
}

pub(super) async fn action_result(
    repo_root: &Path,
    output: GitCommandOutput,
) -> Result<GitActionResult, String> {
    Ok(GitActionResult {
        success: true,
        summary: read_repo_summary(repo_root).await?,
        stdout: compact_output(output.stdout.as_str()),
        stderr: compact_output(output.stderr.as_str()),
    })
}

pub(super) async fn action_result_with_status(
    repo_root: &Path,
    output: GitCommandStatusOutput,
) -> Result<GitActionResult, String> {
    Ok(GitActionResult {
        success: output.success,
        summary: read_repo_summary(repo_root).await?,
        stdout: compact_output(output.stdout.as_str()),
        stderr: compact_output(output.stderr.as_str()),
    })
}

async fn path_command_output(
    repo_root: &Path,
    prefix: &[&str],
    paths: &[String],
) -> Result<GitCommandOutput, String> {
    let mut args = prefix.iter().map(|value| value.to_string()).collect::<Vec<_>>();
    args.push("--".to_string());
    args.extend(paths.iter().cloned());
    git_output(repo_root, args, DEFAULT_GIT_TIMEOUT).await
}
