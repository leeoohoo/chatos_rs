// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use super::contracts::*;
use super::inspection::{ahead_behind, is_tracked_path, untracked_file_patch};
use super::local_connector;
use super::parsing::{
    non_empty, non_repo_summary, parse_compare_commits, parse_name_status_z, parse_status_files,
    split_remote_branch,
};
use super::process::{git_output, git_version, resolve_git_binary, DEFAULT_GIT_TIMEOUT};
use super::shared::{comparison_range, read_repo_summary};
use super::validation::{
    discover_child_repo_roots, discover_repo_root, parse_optional_root, parse_root,
    require_repo_root, validate_relative_paths,
};
use crate::services::project_local_cache::is_local_connector_project_root;
use crate::services::project_local_cache::{cache_key, read_cache_json, write_cache_json};

const CHILD_REPO_DISCOVERY_LIMIT: usize = 32;
const GIT_SUMMARY_CACHE_NAMESPACE: &str = "git";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct GitSummaryCacheEntry {
    summary: GitSummary,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct GitBranchesCacheEntry {
    branches: GitBranches,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct GitStatusCacheEntry {
    status: GitStatus,
}

fn git_summary_cache_path(query_root: &str, preferred_repo_root: Option<&str>) -> String {
    let key = format!(
        "{}|{}",
        query_root.trim(),
        preferred_repo_root.unwrap_or("").trim(),
    );
    format!(
        "{GIT_SUMMARY_CACHE_NAMESPACE}/summary-{}.json",
        cache_key(key.as_str())
    )
}

fn git_branches_cache_path(root: &str) -> String {
    format!(
        "{GIT_SUMMARY_CACHE_NAMESPACE}/branches-{}.json",
        cache_key(root)
    )
}

fn git_status_cache_path(root: &str) -> String {
    format!(
        "{GIT_SUMMARY_CACHE_NAMESPACE}/status-{}.json",
        cache_key(root)
    )
}

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

pub async fn summary(
    root: &str,
    preferred_repo_root: Option<&str>,
    force_refresh: bool,
) -> Result<GitSummary, String> {
    if is_local_connector_project_root(root) {
        return local_connector::summary(root, preferred_repo_root, force_refresh).await;
    }
    let preferred_repo_root_text = preferred_repo_root.map(|value| value.trim().to_string());
    if !force_refresh {
        if let Some(cached) = read_cache_json::<GitSummaryCacheEntry>(
            root,
            git_summary_cache_path(root, preferred_repo_root_text.as_deref()).as_str(),
        )? {
            return Ok(cached.summary);
        }
    }
    let query_root = parse_root(root)?;
    let preferred_root = parse_optional_root(preferred_repo_root)?;
    if let Some(preferred_root) = preferred_root.as_ref() {
        if !preferred_root.starts_with(query_root.as_path()) {
            return Err("preferred_repo_root 不在当前项目路径内".to_string());
        }
    }

    let direct_repo_root = discover_repo_root(query_root.as_path()).await?;
    let preferred_repo_root =
        resolve_preferred_repo_root(query_root.as_path(), preferred_root.as_deref()).await?;
    let available_roots = collect_available_repo_roots(
        query_root.as_path(),
        direct_repo_root.as_deref(),
        preferred_repo_root.as_deref(),
    )
    .await?;
    let selected_repo_root = select_repo_root(
        direct_repo_root.as_deref(),
        preferred_repo_root.as_deref(),
        &available_roots,
    );

    let query_root_text = normalize_path_string(query_root.as_path());
    let candidates = available_roots
        .iter()
        .map(|repo_root| build_repository_candidate(query_root.as_path(), repo_root.as_path()))
        .collect::<Vec<_>>();

    let Some(selected_repo_root) = selected_repo_root else {
        let mut summary = non_repo_summary();
        summary.query_root = Some(query_root_text);
        summary.available_repositories = candidates;
        return Ok(summary);
    };

    let mut summary = read_repo_summary(selected_repo_root.as_path()).await?;
    let selected_root_text = normalize_path_string(selected_repo_root.as_path());
    summary.query_root = Some(query_root_text);
    summary.resolved_root = Some(selected_root_text.clone());
    summary.selected_root = Some(selected_root_text);
    summary.available_repositories = candidates;
    let _ = write_cache_json(
        root,
        git_summary_cache_path(root, preferred_repo_root_text.as_deref()).as_str(),
        &GitSummaryCacheEntry {
            summary: summary.clone(),
        },
    );
    Ok(summary)
}

async fn resolve_preferred_repo_root(
    query_root: &Path,
    preferred_root: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    let Some(preferred_root) = preferred_root else {
        return Ok(None);
    };
    let Some(repo_root) = discover_repo_root(preferred_root).await? else {
        return Ok(None);
    };
    if !repo_root.starts_with(query_root) {
        return Ok(None);
    }
    Ok(Some(repo_root))
}

async fn collect_available_repo_roots(
    query_root: &Path,
    direct_repo_root: Option<&Path>,
    preferred_repo_root: Option<&Path>,
) -> Result<Vec<PathBuf>, String> {
    let mut repos = BTreeMap::<String, PathBuf>::new();
    if let Some(repo_root) = direct_repo_root {
        repos.insert(normalize_path_string(repo_root), repo_root.to_path_buf());
    } else {
        for repo_root in discover_child_repo_roots(query_root, CHILD_REPO_DISCOVERY_LIMIT).await? {
            repos.insert(normalize_path_string(repo_root.as_path()), repo_root);
        }
    }
    if let Some(repo_root) = preferred_repo_root {
        repos.insert(normalize_path_string(repo_root), repo_root.to_path_buf());
    }
    Ok(repos.into_values().collect())
}

fn select_repo_root(
    direct_repo_root: Option<&Path>,
    preferred_repo_root: Option<&Path>,
    available_roots: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(preferred_repo_root) = preferred_repo_root {
        if let Some(found) = available_roots
            .iter()
            .find(|repo_root| repo_root.as_path() == preferred_repo_root)
        {
            return Some(found.clone());
        }
    }
    if let Some(direct_repo_root) = direct_repo_root {
        if let Some(found) = available_roots
            .iter()
            .find(|repo_root| repo_root.as_path() == direct_repo_root)
        {
            return Some(found.clone());
        }
        return Some(direct_repo_root.to_path_buf());
    }
    available_roots.first().cloned()
}

fn build_repository_candidate(query_root: &Path, repo_root: &Path) -> GitRepositoryCandidate {
    let relative_path = repo_root
        .strip_prefix(query_root)
        .ok()
        .map(normalize_path_string)
        .unwrap_or_default();
    let label = if relative_path.is_empty() {
        repo_root
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| normalize_path_string(repo_root))
    } else {
        relative_path.clone()
    };
    GitRepositoryCandidate {
        root: normalize_path_string(repo_root),
        label,
        relative_path,
    }
}

fn normalize_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub async fn branches(root: &str, force_refresh: bool) -> Result<GitBranches, String> {
    if is_local_connector_project_root(root) {
        return local_connector::branches(root, force_refresh).await;
    }
    if !force_refresh {
        if let Some(cached) =
            read_cache_json::<GitBranchesCacheEntry>(root, git_branches_cache_path(root).as_str())?
        {
            return Ok(cached.branches);
        }
    }
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

    let branches = GitBranches {
        current,
        locals,
        remotes,
    };
    let _ = write_cache_json(
        root,
        git_branches_cache_path(root).as_str(),
        &GitBranchesCacheEntry {
            branches: branches.clone(),
        },
    );
    Ok(branches)
}

pub async fn status(root: &str, force_refresh: bool) -> Result<GitStatus, String> {
    if is_local_connector_project_root(root) {
        return local_connector::status(root, force_refresh).await;
    }
    if !force_refresh {
        if let Some(cached) =
            read_cache_json::<GitStatusCacheEntry>(root, git_status_cache_path(root).as_str())?
        {
            return Ok(cached.status);
        }
    }
    let repo_root = require_repo_root(root).await?;
    let output = git_output(
        repo_root.as_path(),
        ["status", "--porcelain=v2", "-z", "--untracked-files=all"],
        DEFAULT_GIT_TIMEOUT,
    )
    .await?;
    let status = GitStatus {
        files: parse_status_files(repo_root.as_path(), &output.stdout),
    };
    let _ = write_cache_json(
        root,
        git_status_cache_path(root).as_str(),
        &GitStatusCacheEntry {
            status: status.clone(),
        },
    );
    Ok(status)
}

pub async fn compare(query: GitCompareQuery) -> Result<GitCompareResult, String> {
    if is_local_connector_project_root(query.root.as_str()) {
        return local_connector::compare(query).await;
    }
    let repo_root = require_repo_root(&query.root).await?;
    let (current, target, range) =
        comparison_range(repo_root.as_path(), query.target.trim()).await?;

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
    if is_local_connector_project_root(query.root.as_str()) {
        return local_connector::file_diff(query).await;
    }
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
