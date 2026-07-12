// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::contracts::*;
use super::parsing::{
    non_repo_summary, parse_compare_commits, parse_name_status_z, parse_status_files,
    split_remote_branch, summary_from_status,
};
use super::process::{DEFAULT_GIT_TIMEOUT, REMOTE_GIT_TIMEOUT};
use super::validation::{ensure_safe_ref, merge_args, validate_relative_paths};
use crate::api::local_connectors::{
    call_local_mcp_tool, parse_local_connector_root_path, LOCAL_CONNECTOR_BUILTIN_TERMINAL,
};

#[derive(Debug, Clone)]
struct LocalGitOutput {
    stdout: String,
    stderr: String,
    success: bool,
}

pub async fn summary(
    root: &str,
    preferred_repo_root: Option<&str>,
    _force_refresh: bool,
) -> Result<GitSummary, String> {
    let root = preferred_repo_root
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(root)
        .trim();
    let output = git_exec_allow_failure(
        root,
        vec![
            "status".to_string(),
            "--porcelain=v2".to_string(),
            "--branch".to_string(),
            "--untracked-files=all".to_string(),
        ],
        DEFAULT_GIT_TIMEOUT.as_millis() as u64,
    )
    .await?;
    if !output.success {
        let mut summary = non_repo_summary();
        summary.query_root = Some(root.to_string());
        return Ok(summary);
    }

    let mut summary = summary_from_status(PathBuf::from(root), output.stdout.as_str());
    summary.query_root = Some(root.to_string());
    summary.resolved_root = Some(root.to_string());
    summary.selected_root = Some(root.to_string());
    summary.available_repositories = vec![GitRepositoryCandidate {
        root: root.to_string(),
        label: local_root_label(root),
        relative_path: String::new(),
    }];
    Ok(summary)
}

pub async fn branches(root: &str, _force_refresh: bool) -> Result<GitBranches, String> {
    let current = git_exec(
        root,
        vec!["branch".to_string(), "--show-current".to_string()],
        DEFAULT_GIT_TIMEOUT.as_millis() as u64,
    )
    .await
    .ok()
    .and_then(|output| non_empty(output.stdout.as_str()));

    let locals_output = git_exec(
        root,
        vec![
            "for-each-ref".to_string(),
            "--format=%(refname:short)%00%(objectname:short)%00%(subject)%00%(upstream:short)"
                .to_string(),
            "refs/heads".to_string(),
        ],
        DEFAULT_GIT_TIMEOUT.as_millis() as u64,
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
        locals.push(GitBranchInfo {
            name: name.to_string(),
            short_name: Some(name.to_string()),
            current: current.as_deref() == Some(name),
            upstream: non_empty(fields.get(3).copied().unwrap_or("")),
            remote: None,
            tracked_by: None,
            ahead: 0,
            behind: 0,
            last_commit: non_empty(fields.get(1).copied().unwrap_or("")),
            last_commit_subject: non_empty(fields.get(2).copied().unwrap_or("")),
        });
    }

    let remotes_output = git_exec(
        root,
        vec![
            "for-each-ref".to_string(),
            "--format=%(refname:short)%00%(objectname:short)%00%(subject)".to_string(),
            "refs/remotes".to_string(),
        ],
        DEFAULT_GIT_TIMEOUT.as_millis() as u64,
    )
    .await
    .unwrap_or(LocalGitOutput {
        stdout: String::new(),
        stderr: String::new(),
        success: true,
    });
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
            tracked_by: None,
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

pub async fn status(root: &str, _force_refresh: bool) -> Result<GitStatus, String> {
    let output = git_exec(
        root,
        vec![
            "status".to_string(),
            "--porcelain=v2".to_string(),
            "-z".to_string(),
            "--untracked-files=all".to_string(),
        ],
        DEFAULT_GIT_TIMEOUT.as_millis() as u64,
    )
    .await?;
    Ok(GitStatus {
        files: parse_status_files(Path::new(""), output.stdout.as_str()),
    })
}

pub async fn compare(query: GitCompareQuery) -> Result<GitCompareResult, String> {
    let target = query.target.trim();
    ensure_safe_ref(target, "target")?;
    let current = current_branch(query.root.as_str()).await?;
    let range = format!("{target}...HEAD");
    let files_output = git_exec(
        query.root.as_str(),
        vec![
            "diff".to_string(),
            "--name-status".to_string(),
            "-z".to_string(),
            range.clone(),
        ],
        DEFAULT_GIT_TIMEOUT.as_millis() as u64,
    )
    .await?;
    let commits_output = git_exec(
        query.root.as_str(),
        vec![
            "log".to_string(),
            "--left-right".to_string(),
            "--cherry-pick".to_string(),
            "--format=%m%x1f%h%x1f%s".to_string(),
            range,
        ],
        DEFAULT_GIT_TIMEOUT.as_millis() as u64,
    )
    .await
    .unwrap_or(LocalGitOutput {
        stdout: String::new(),
        stderr: String::new(),
        success: true,
    });
    Ok(GitCompareResult {
        current,
        target: target.to_string(),
        files: parse_name_status_z(files_output.stdout.as_str()),
        commits: parse_compare_commits(commits_output.stdout.as_str()),
    })
}

pub async fn file_diff(query: GitDiffQuery) -> Result<GitFileDiff, String> {
    let paths = validate_relative_paths(std::slice::from_ref(&query.path))?;
    let path = paths
        .first()
        .cloned()
        .ok_or_else(|| "path 不能为空".to_string())?;
    let staged = query.staged.unwrap_or(false);
    let mut args = vec!["diff".to_string()];
    if let Some(target) = query
        .target
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        ensure_safe_ref(target, "target")?;
        args.push(format!("{target}...HEAD"));
    } else if staged {
        args.push("--cached".to_string());
    }
    args.push("--".to_string());
    args.push(path.clone());
    let output = git_exec(
        query.root.as_str(),
        args,
        DEFAULT_GIT_TIMEOUT.as_millis() as u64,
    )
    .await?;
    Ok(GitFileDiff {
        path,
        target: query.target,
        staged,
        patch: output.stdout,
    })
}

pub async fn fetch(request: GitFetchRequest) -> Result<GitActionResult, String> {
    let remote = request.remote.as_deref().unwrap_or("origin").trim();
    ensure_safe_ref(remote, "remote")?;
    action_result(
        request.root.as_str(),
        git_exec(
            request.root.as_str(),
            vec![
                "fetch".to_string(),
                "--prune".to_string(),
                remote.to_string(),
            ],
            REMOTE_GIT_TIMEOUT.as_millis() as u64,
        )
        .await?,
    )
    .await
}

pub async fn pull(request: GitPullRequest) -> Result<GitActionResult, String> {
    let mode = request.mode.as_deref().unwrap_or("ff-only").trim();
    let args = match mode {
        "ff-only" | "" => vec!["pull".to_string(), "--ff-only".to_string()],
        "rebase" => vec!["pull".to_string(), "--rebase".to_string()],
        _ => return Err("不支持的 pull 模式".to_string()),
    };
    action_result(
        request.root.as_str(),
        git_exec(
            request.root.as_str(),
            args,
            REMOTE_GIT_TIMEOUT.as_millis() as u64,
        )
        .await?,
    )
    .await
}

pub async fn push(request: GitPushRequest) -> Result<GitActionResult, String> {
    let remote = request.remote.as_deref().unwrap_or("origin").trim();
    ensure_safe_ref(remote, "remote")?;
    let branch = match request
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => value.to_string(),
        None => current_branch(request.root.as_str()).await?,
    };
    ensure_safe_ref(branch.as_str(), "branch")?;
    let mut args = vec!["push".to_string()];
    if request.set_upstream.unwrap_or(false) {
        args.push("-u".to_string());
    }
    args.push(remote.to_string());
    args.push(branch);
    action_result(
        request.root.as_str(),
        git_exec(
            request.root.as_str(),
            args,
            REMOTE_GIT_TIMEOUT.as_millis() as u64,
        )
        .await?,
    )
    .await
}

pub async fn checkout(request: GitCheckoutRequest) -> Result<GitActionResult, String> {
    let args = if request.create_tracking.unwrap_or(false) {
        let remote_branch = request
            .remote_branch
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "remote_branch 不能为空".to_string())?;
        ensure_safe_ref(remote_branch, "remote_branch")?;
        let local_branch = request
            .branch
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                remote_branch
                    .rsplit('/')
                    .next()
                    .unwrap_or(remote_branch)
                    .to_string()
            });
        ensure_safe_ref(local_branch.as_str(), "branch")?;
        vec![
            "checkout".to_string(),
            "-b".to_string(),
            local_branch,
            "--track".to_string(),
            remote_branch.to_string(),
        ]
    } else {
        let branch = request
            .branch
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "branch 不能为空".to_string())?;
        ensure_safe_ref(branch, "branch")?;
        vec!["checkout".to_string(), branch.to_string()]
    };
    action_result(
        request.root.as_str(),
        git_exec(
            request.root.as_str(),
            args,
            DEFAULT_GIT_TIMEOUT.as_millis() as u64,
        )
        .await?,
    )
    .await
}

pub async fn create_branch(request: GitCreateBranchRequest) -> Result<GitActionResult, String> {
    let name = request.name.trim();
    ensure_safe_ref(name, "branch")?;
    let start_point = request
        .start_point
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(value) = start_point {
        ensure_safe_ref(value, "start_point")?;
    }
    let mut args = if request.checkout.unwrap_or(true) {
        vec!["checkout".to_string(), "-b".to_string(), name.to_string()]
    } else {
        vec!["branch".to_string(), name.to_string()]
    };
    if let Some(value) = start_point {
        args.push(value.to_string());
    }
    action_result(
        request.root.as_str(),
        git_exec(
            request.root.as_str(),
            args,
            DEFAULT_GIT_TIMEOUT.as_millis() as u64,
        )
        .await?,
    )
    .await
}

pub async fn merge(request: GitMergeRequest) -> Result<GitActionResult, String> {
    let branch = request.branch.trim();
    if branch.is_empty() {
        return Err("branch 不能为空".to_string());
    }
    ensure_safe_ref(branch, "branch")?;
    let args = merge_args(request.mode.as_deref(), branch)?
        .into_iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    action_result(
        request.root.as_str(),
        git_exec(
            request.root.as_str(),
            args,
            REMOTE_GIT_TIMEOUT.as_millis() as u64,
        )
        .await?,
    )
    .await
}

pub async fn stage(request: GitPathRequest) -> Result<GitActionResult, String> {
    let paths = validate_relative_paths(&request.paths)?;
    let mut args = vec!["add".to_string(), "--".to_string()];
    args.extend(paths);
    action_result(
        request.root.as_str(),
        git_exec(
            request.root.as_str(),
            args,
            DEFAULT_GIT_TIMEOUT.as_millis() as u64,
        )
        .await?,
    )
    .await
}

pub async fn unstage(request: GitPathRequest) -> Result<GitActionResult, String> {
    let paths = validate_relative_paths(&request.paths)?;
    let mut args = vec![
        "restore".to_string(),
        "--staged".to_string(),
        "--".to_string(),
    ];
    args.extend(paths);
    action_result(
        request.root.as_str(),
        git_exec(
            request.root.as_str(),
            args,
            DEFAULT_GIT_TIMEOUT.as_millis() as u64,
        )
        .await?,
    )
    .await
}

pub async fn commit(request: GitCommitRequest) -> Result<GitActionResult, String> {
    let message = request.message.trim();
    if message.is_empty() {
        return Err("commit message 不能为空".to_string());
    }
    if let Some(paths) = request.paths.as_ref().filter(|paths| !paths.is_empty()) {
        let paths = validate_relative_paths(paths)?;
        let mut args = vec!["add".to_string(), "--".to_string()];
        args.extend(paths);
        git_exec(
            request.root.as_str(),
            args,
            DEFAULT_GIT_TIMEOUT.as_millis() as u64,
        )
        .await?;
    }
    action_result(
        request.root.as_str(),
        git_exec(
            request.root.as_str(),
            vec!["commit".to_string(), "-m".to_string(), message.to_string()],
            DEFAULT_GIT_TIMEOUT.as_millis() as u64,
        )
        .await?,
    )
    .await
}

pub async fn discard(request: GitPathRequest) -> Result<GitActionResult, String> {
    let paths = validate_relative_paths(&request.paths)?;
    let mut args = vec!["checkout".to_string(), "--".to_string()];
    args.extend(paths);
    action_result(
        request.root.as_str(),
        git_exec(
            request.root.as_str(),
            args,
            DEFAULT_GIT_TIMEOUT.as_millis() as u64,
        )
        .await?,
    )
    .await
}

async fn current_branch(root: &str) -> Result<String, String> {
    let output = git_exec(
        root,
        vec!["branch".to_string(), "--show-current".to_string()],
        DEFAULT_GIT_TIMEOUT.as_millis() as u64,
    )
    .await?;
    non_empty(output.stdout.as_str()).ok_or_else(|| "当前不是分支状态".to_string())
}

async fn action_result(root: &str, output: LocalGitOutput) -> Result<GitActionResult, String> {
    Ok(GitActionResult {
        success: output.success,
        summary: summary(root, None, true).await?,
        stdout: non_empty(output.stdout.as_str()),
        stderr: non_empty(output.stderr.as_str()),
    })
}

async fn git_exec(
    root: &str,
    args: Vec<String>,
    timeout_ms: u64,
) -> Result<LocalGitOutput, String> {
    let output = git_exec_allow_failure(root, args, timeout_ms).await?;
    if output.success {
        return Ok(output);
    }
    Err(non_empty(output.stderr.as_str())
        .or_else(|| non_empty(output.stdout.as_str()))
        .unwrap_or_else(|| "Local Connector Git 命令执行失败".to_string()))
}

async fn git_exec_allow_failure(
    root: &str,
    args: Vec<String>,
    timeout_ms: u64,
) -> Result<LocalGitOutput, String> {
    let root_ref = parse_local_connector_root_path(root)
        .ok_or_else(|| "Local Connector root 格式错误".to_string())?;
    let command = git_command(args.as_slice());
    let value = call_local_mcp_tool(
        root_ref.device_id.as_str(),
        root_ref.workspace_id.as_str(),
        root_ref.relative_path.as_deref(),
        &[LOCAL_CONNECTOR_BUILTIN_TERMINAL],
        "execute_command",
        json!({
            "path": ".",
            "common": command,
            "background": false,
            "timeout_ms": timeout_ms,
        }),
    )
    .await
    .map_err(connector_error_message)?;
    Ok(LocalGitOutput {
        stdout: value
            .get("stdout")
            .or_else(|| value.get("output"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        stderr: value
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        success: value
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| value.get("exit_code").and_then(Value::as_i64) == Some(0)),
    })
}

fn git_command(args: &[String]) -> String {
    let mut parts = vec!["git".to_string()];
    parts.extend(args.iter().map(|arg| shell_quote(arg.as_str())));
    parts.join(" ")
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn connector_error_message(err: (axum::http::StatusCode, axum::Json<Value>)) -> String {
    let (status, axum::Json(value)) = err;
    value
        .get("error")
        .and_then(Value::as_str)
        .map(|message| format!("{message} ({status})"))
        .unwrap_or_else(|| format!("{value} ({status})"))
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn local_root_label(root: &str) -> String {
    root.trim_end_matches('/')
        .rsplit('/')
        .find(|part| !part.trim().is_empty())
        .unwrap_or("Local Connector Git")
        .to_string()
}
