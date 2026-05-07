use std::collections::HashMap;

use super::contracts::*;
use super::inspection::{ahead_behind, is_tracked_path, untracked_file_patch};
use super::parsing::{
    non_empty, non_repo_summary, parse_compare_commits, parse_name_status_z, parse_status_files,
    split_remote_branch,
};
use super::process::{git_output, git_version, resolve_git_binary, DEFAULT_GIT_TIMEOUT};
use super::shared::{comparison_range, read_repo_summary};
use super::validation::{discover_repo_root, parse_root, require_repo_root, validate_relative_paths};

pub async fn client_info() -> GitClientInfo {
    let git_bin = resolve_git_binary();
    let version = git_version(&git_bin).await;
    let (available, version, error) = match version {
        Ok(value) => (true, Some(value), None),
        Err(message) => (false, None, Some(message)),
    };
    GitClientInfo {
        available,
        source: git_bin.source.as_str().to_string(),
        path: git_bin.path.to_string_lossy().to_string(),
        version,
        error,
        bundled_candidates: git_bin
            .bundled_candidates
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
    }
}

pub async fn summary(root: &str) -> Result<GitSummary, String> {
    let root = parse_root(root)?;
    let Some(repo_root) = discover_repo_root(root.as_path()).await? else {
        return Ok(non_repo_summary());
    };
    read_repo_summary(repo_root.as_path()).await
}

pub async fn branches(root: &str) -> Result<GitBranches, String> {
    let repo_root = require_repo_root(root).await?;
    let current = read_repo_summary(repo_root.as_path()).await?.current_branch;

    let locals_output = git_output(
        repo_root.as_path(),
        [
            "for-each-ref",
            "--format=%(refname:short)%00%(objectname:short)%00%(subject)%00%(upstream:short)",
            "refs/heads",
        ],
        DEFAULT_GIT_TIMEOUT,
    )
    .await?;
    let mut locals = Vec::new();
    for line in locals_output.stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\0').collect();
        let name = fields.first().copied().unwrap_or("").trim();
        if name.is_empty() {
            continue;
        }
        let upstream = non_empty(fields.get(3).copied().unwrap_or(""));
        let (ahead, behind) = if let Some(upstream_ref) = upstream.as_deref() {
            ahead_behind(repo_root.as_path(), name, upstream_ref)
                .await
                .unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        locals.push(GitBranchInfo {
            name: name.to_string(),
            short_name: Some(name.to_string()),
            current: current.as_deref() == Some(name),
            upstream,
            remote: None,
            tracked_by: None,
            ahead,
            behind,
            last_commit: non_empty(fields.get(1).copied().unwrap_or("")),
            last_commit_subject: non_empty(fields.get(2).copied().unwrap_or("")),
        });
    }

    let upstream_to_local = locals
        .iter()
        .filter_map(|branch| {
            branch
                .upstream
                .as_ref()
                .map(|upstream| (upstream.clone(), branch.name.clone()))
        })
        .collect::<HashMap<_, _>>();

    let remotes_output = git_output(
        repo_root.as_path(),
        [
            "for-each-ref",
            "--format=%(refname:short)%00%(objectname:short)%00%(subject)",
            "refs/remotes",
        ],
        DEFAULT_GIT_TIMEOUT,
    )
    .await?;
    let mut remotes = Vec::new();
    for line in remotes_output.stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\0').collect();
        let name = fields.first().copied().unwrap_or("").trim();
        if name.is_empty() || name.ends_with("/HEAD") {
            continue;
        }
        let (remote, short_name) = split_remote_branch(name);
        remotes.push(GitBranchInfo {
            name: name.to_string(),
            short_name,
            current: false,
            upstream: None,
            remote,
            tracked_by: upstream_to_local.get(name).cloned(),
            ahead: 0,
            behind: 0,
            last_commit: non_empty(fields.get(1).copied().unwrap_or("")),
            last_commit_subject: non_empty(fields.get(2).copied().unwrap_or("")),
        });
    }

    locals.sort_by(|left, right| left.name.cmp(&right.name));
    remotes.sort_by(|left, right| left.name.cmp(&right.name));

    Ok(GitBranches {
        current,
        locals,
        remotes,
    })
}

pub async fn status(root: &str) -> Result<GitStatus, String> {
    let repo_root = require_repo_root(root).await?;
    let output = git_output(
        repo_root.as_path(),
        ["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        DEFAULT_GIT_TIMEOUT,
    )
    .await?;
    Ok(GitStatus {
        files: parse_status_files(&output.stdout),
    })
}

pub async fn compare(query: GitCompareQuery) -> Result<GitCompareResult, String> {
    let repo_root = require_repo_root(&query.root).await?;
    let (current, target, range) = comparison_range(repo_root.as_path(), query.target.trim()).await?;

    let diff_output = git_output(
        repo_root.as_path(),
        vec!["diff", "--name-status", "-z", range.as_str()],
        DEFAULT_GIT_TIMEOUT,
    )
    .await?;
    let log_output = git_output(
        repo_root.as_path(),
        vec![
            "log",
            "--left-right",
            "--cherry-pick",
            "--format=%m%x1f%h%x1f%s",
            range.as_str(),
        ],
        DEFAULT_GIT_TIMEOUT,
    )
    .await?;

    Ok(GitCompareResult {
        current,
        target,
        files: parse_name_status_z(&diff_output.stdout),
        commits: parse_compare_commits(&log_output.stdout),
    })
}

pub async fn file_diff(query: GitDiffQuery) -> Result<GitFileDiff, String> {
    let repo_root = require_repo_root(&query.root).await?;
    let paths = validate_relative_paths(&[query.path.clone()])?;
    let path = paths
        .first()
        .cloned()
        .ok_or_else(|| "path 不能为空".to_string())?;
    let staged = query.staged.unwrap_or(false);
    let mut output = if let Some(target) = query
        .target
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let (_, _, range) = comparison_range(repo_root.as_path(), target).await?;
        git_output(
            repo_root.as_path(),
            vec!["diff", range.as_str(), "--", path.as_str()],
            DEFAULT_GIT_TIMEOUT,
        )
        .await?
    } else if staged {
        git_output(
            repo_root.as_path(),
            vec!["diff", "--cached", "--", path.as_str()],
            DEFAULT_GIT_TIMEOUT,
        )
        .await?
    } else {
        git_output(
            repo_root.as_path(),
            vec!["diff", "--", path.as_str()],
            DEFAULT_GIT_TIMEOUT,
        )
        .await?
    };
    if query.target.is_none()
        && !staged
        && output.stdout.trim().is_empty()
        && !is_tracked_path(repo_root.as_path(), path.as_str()).await
    {
        output.stdout = untracked_file_patch(repo_root.as_path(), path.as_str()).await?;
    }
    Ok(GitFileDiff {
        path,
        target: query.target,
        staged,
        patch: output.stdout,
    })
}
