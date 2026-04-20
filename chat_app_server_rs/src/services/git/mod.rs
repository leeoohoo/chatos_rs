pub mod contracts;

use std::collections::HashMap;
use std::env;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use tokio::fs;
use tokio::process::Command;
use tokio::time::timeout;

pub use contracts::*;

const DEFAULT_GIT_TIMEOUT: Duration = Duration::from_secs(20);
const REMOTE_GIT_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_UNTRACKED_DIFF_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone)]
struct GitCommandOutput {
    stdout: String,
    stderr: String,
}

#[derive(Debug, Clone)]
struct GitCommandStatusOutput {
    success: bool,
    status: String,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Clone)]
struct GitBinaryResolution {
    path: PathBuf,
    source: GitBinarySource,
    bundled_candidates: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
enum GitBinarySource {
    Env,
    Bundled,
    System,
}

impl GitBinarySource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Bundled => "bundled",
            Self::System => "system",
        }
    }
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

pub async fn summary(root: &str) -> Result<GitSummary, String> {
    let root = parse_root(root)?;
    let Some(repo_root) = discover_repo_root(root.as_path()).await? else {
        return Ok(non_repo_summary());
    };

    let status = git_output(
        repo_root.as_path(),
        [
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
        ],
        DEFAULT_GIT_TIMEOUT,
    )
    .await?;
    Ok(summary_from_status(repo_root, &status.stdout))
}

pub async fn branches(root: &str) -> Result<GitBranches, String> {
    let repo_root = require_repo_root(root).await?;
    let current = summary(root).await?.current_branch;

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
    let target = query.target.trim();
    ensure_safe_ref(target, "target")?;
    let current = summary(&query.root)
        .await?
        .current_branch
        .unwrap_or_else(|| "HEAD".to_string());
    ensure_safe_ref(current.as_str(), "current")?;
    let range = format!("{}...{}", current, target);

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
        target: target.to_string(),
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
        ensure_safe_ref(target, "target")?;
        let current = summary(&query.root)
            .await?
            .current_branch
            .unwrap_or_else(|| "HEAD".to_string());
        ensure_safe_ref(current.as_str(), "current")?;
        let range = format!("{}...{}", current, target);
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

pub async fn fetch(request: GitFetchRequest) -> Result<GitActionResult, String> {
    let repo_root = require_repo_root(&request.root).await?;
    let remote = request.remote.as_deref().unwrap_or("origin").trim();
    ensure_safe_ref(remote, "remote")?;
    let output = git_output(
        repo_root.as_path(),
        ["fetch", "--prune", remote],
        REMOTE_GIT_TIMEOUT,
    )
    .await?;
    action_result(&request.root, output).await
}

pub async fn pull(request: GitPullRequest) -> Result<GitActionResult, String> {
    let repo_root = require_repo_root(&request.root).await?;
    let mode = request.mode.as_deref().unwrap_or("ff-only").trim();
    let args: Vec<&str> = match mode {
        "ff-only" | "" => vec!["pull", "--ff-only"],
        "rebase" => vec!["pull", "--rebase"],
        _ => return Err("不支持的 pull 模式".to_string()),
    };
    let output = git_output(repo_root.as_path(), args, REMOTE_GIT_TIMEOUT).await?;
    action_result(&request.root, output).await
}

pub async fn push(request: GitPushRequest) -> Result<GitActionResult, String> {
    let repo_root = require_repo_root(&request.root).await?;
    let remote = request.remote.as_deref().unwrap_or("origin").trim();
    ensure_safe_ref(remote, "remote")?;
    let branch = match request
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => value.to_string(),
        None => summary(&request.root)
            .await?
            .current_branch
            .ok_or_else(|| "当前不是分支状态，无法 push".to_string())?,
    };
    ensure_safe_ref(branch.as_str(), "branch")?;
    let args = if request.set_upstream.unwrap_or(false) {
        vec!["push", "-u", remote, branch.as_str()]
    } else {
        vec!["push", remote, branch.as_str()]
    };
    let output = git_output(repo_root.as_path(), args, REMOTE_GIT_TIMEOUT).await?;
    action_result(&request.root, output).await
}

pub async fn checkout(request: GitCheckoutRequest) -> Result<GitActionResult, String> {
    let repo_root = require_repo_root(&request.root).await?;
    let output = if request.create_tracking.unwrap_or(false) {
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
        validate_branch_name(repo_root.as_path(), local_branch.as_str()).await?;
        git_output(
            repo_root.as_path(),
            vec![
                "checkout",
                "-b",
                local_branch.as_str(),
                "--track",
                remote_branch,
            ],
            DEFAULT_GIT_TIMEOUT,
        )
        .await?
    } else {
        let branch = request
            .branch
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "branch 不能为空".to_string())?;
        ensure_safe_ref(branch, "branch")?;
        git_output(
            repo_root.as_path(),
            vec!["checkout", branch],
            DEFAULT_GIT_TIMEOUT,
        )
        .await?
    };
    action_result(&request.root, output).await
}

pub async fn create_branch(request: GitCreateBranchRequest) -> Result<GitActionResult, String> {
    let repo_root = require_repo_root(&request.root).await?;
    let name = request.name.trim();
    validate_branch_name(repo_root.as_path(), name).await?;
    let start_point = request
        .start_point
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(value) = start_point {
        ensure_safe_ref(value, "start_point")?;
    }
    let output = if request.checkout.unwrap_or(true) {
        let mut args = vec!["checkout", "-b", name];
        if let Some(value) = start_point {
            args.push(value);
        }
        git_output(repo_root.as_path(), args, DEFAULT_GIT_TIMEOUT).await?
    } else {
        let mut args = vec!["branch", name];
        if let Some(value) = start_point {
            args.push(value);
        }
        git_output(repo_root.as_path(), args, DEFAULT_GIT_TIMEOUT).await?
    };
    action_result(&request.root, output).await
}

pub async fn merge(request: GitMergeRequest) -> Result<GitActionResult, String> {
    let repo_root = require_repo_root(&request.root).await?;
    let branch = request.branch.trim();
    if branch.is_empty() {
        return Err("branch 不能为空".to_string());
    }
    ensure_safe_ref(branch, "branch")?;

    let current_summary = summary(&request.root).await?;
    if let Some(operation_state) = current_summary.operation_state.as_deref() {
        return Err(format!("当前已有 Git 操作未完成: {}", operation_state));
    }
    let current_branch = current_summary
        .current_branch
        .as_deref()
        .ok_or_else(|| "当前不是分支状态，无法 merge".to_string())?;
    if current_branch == branch {
        return Err("不能将当前分支 merge 到自己".to_string());
    }

    let output = git_output_with_status(
        repo_root.as_path(),
        merge_args(request.mode.as_deref(), branch)?,
        REMOTE_GIT_TIMEOUT,
    )
    .await?;
    action_result_with_status(&request.root, output).await
}

pub async fn stage(request: GitPathRequest) -> Result<GitActionResult, String> {
    let repo_root = require_repo_root(&request.root).await?;
    let paths = validate_relative_paths(&request.paths)?;
    let mut args = vec!["add", "--"];
    args.extend(paths.iter().map(String::as_str));
    let output = git_output(repo_root.as_path(), args, DEFAULT_GIT_TIMEOUT).await?;
    action_result(&request.root, output).await
}

pub async fn unstage(request: GitPathRequest) -> Result<GitActionResult, String> {
    let repo_root = require_repo_root(&request.root).await?;
    let paths = validate_relative_paths(&request.paths)?;
    let mut args = vec!["restore", "--staged", "--"];
    args.extend(paths.iter().map(String::as_str));
    let output = git_output(repo_root.as_path(), args, DEFAULT_GIT_TIMEOUT).await?;
    action_result(&request.root, output).await
}

pub async fn commit(request: GitCommitRequest) -> Result<GitActionResult, String> {
    let repo_root = require_repo_root(&request.root).await?;
    let message = request.message.trim();
    if message.is_empty() {
        return Err("commit message 不能为空".to_string());
    }
    if let Some(paths) = request.paths.as_ref().filter(|paths| !paths.is_empty()) {
        let paths = validate_relative_paths(paths)?;
        let mut args = vec!["add", "--"];
        args.extend(paths.iter().map(String::as_str));
        git_output(repo_root.as_path(), args, DEFAULT_GIT_TIMEOUT).await?;
    }
    let output = git_output(
        repo_root.as_path(),
        vec!["commit", "-m", message],
        DEFAULT_GIT_TIMEOUT,
    )
    .await?;
    action_result(&request.root, output).await
}

async fn action_result(root: &str, output: GitCommandOutput) -> Result<GitActionResult, String> {
    Ok(GitActionResult {
        success: true,
        summary: summary(root).await?,
        stdout: compact_output(output.stdout.as_str()),
        stderr: compact_output(output.stderr.as_str()),
    })
}

async fn action_result_with_status(
    root: &str,
    output: GitCommandStatusOutput,
) -> Result<GitActionResult, String> {
    Ok(GitActionResult {
        success: output.success,
        summary: summary(root).await?,
        stdout: compact_output(output.stdout.as_str()),
        stderr: compact_output(output.stderr.as_str()),
    })
}

async fn require_repo_root(root: &str) -> Result<PathBuf, String> {
    let root = parse_root(root)?;
    discover_repo_root(root.as_path())
        .await?
        .ok_or_else(|| "当前项目不是 Git 仓库".to_string())
}

fn parse_root(root: &str) -> Result<PathBuf, String> {
    let root = root.trim();
    if root.is_empty() {
        return Err("root 不能为空".to_string());
    }
    let path = PathBuf::from(root);
    if !path.exists() {
        return Err("root 路径不存在".to_string());
    }
    if !path.is_dir() {
        return Err("root 不是目录".to_string());
    }
    std::fs::canonicalize(path).map_err(|err| format!("解析 root 路径失败: {}", err))
}

async fn discover_repo_root(root: &Path) -> Result<Option<PathBuf>, String> {
    match git_output(root, ["rev-parse", "--show-toplevel"], DEFAULT_GIT_TIMEOUT).await {
        Ok(output) => {
            let text = output.stdout.trim();
            if text.is_empty() {
                Ok(None)
            } else {
                let repo_root = std::fs::canonicalize(text).unwrap_or_else(|_| PathBuf::from(text));
                if !root.starts_with(repo_root.as_path()) {
                    return Err("Git 仓库根目录不在当前项目路径内".to_string());
                }
                Ok(Some(repo_root))
            }
        }
        Err(message)
            if message.contains("not a git repository") || message.contains("不是 git 仓库") =>
        {
            Ok(None)
        }
        Err(message) => Err(message),
    }
}

async fn git_output<I, S>(
    root: &Path,
    args: I,
    duration: Duration,
) -> Result<GitCommandOutput, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = git_output_with_status(root, args, duration).await?;
    if !output.success {
        let message = output.stderr.trim();
        if message.is_empty() {
            let stdout = output.stdout.trim();
            if !stdout.is_empty() {
                return Err(stdout.chars().take(1200).collect());
            }
            return Err(format!("git 命令失败: {}", output.status));
        }
        return Err(message.chars().take(1200).collect());
    }
    Ok(GitCommandOutput {
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

async fn git_output_with_status<I, S>(
    root: &Path,
    args: I,
    duration: Duration,
) -> Result<GitCommandStatusOutput, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let git_bin = resolve_git_binary();
    let mut command = Command::new(git_bin.path.as_os_str());
    command
        .arg("-C")
        .arg(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_MERGE_AUTOEDIT", "no")
        .args(args);
    add_git_bin_dir_to_path(&mut command, git_bin.path.as_path());
    let output = match timeout(duration, command.output()).await {
        Ok(result) => result.map_err(|err| git_launch_error(git_bin.path.as_path(), err))?,
        Err(_) => return Err("执行 git 命令超时".to_string()),
    };
    Ok(GitCommandStatusOutput {
        success: output.status.success(),
        status: output.status.to_string(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

async fn git_version(git_bin: &GitBinaryResolution) -> Result<String, String> {
    let mut command = Command::new(git_bin.path.as_os_str());
    command.arg("--version").env("GIT_TERMINAL_PROMPT", "0");
    add_git_bin_dir_to_path(&mut command, git_bin.path.as_path());
    let output = match timeout(DEFAULT_GIT_TIMEOUT, command.output()).await {
        Ok(result) => result.map_err(|err| git_launch_error(git_bin.path.as_path(), err))?,
        Err(_) => return Err("执行 git --version 超时".to_string()),
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        return Ok(if stdout.is_empty() {
            "git version unknown".to_string()
        } else {
            stdout
        });
    }
    Err(if stderr.is_empty() {
        format!("git --version 失败: {}", output.status)
    } else {
        stderr.chars().take(1200).collect()
    })
}

fn add_git_bin_dir_to_path(command: &mut Command, git_bin: &Path) {
    let Some(parent) = git_bin.parent().filter(|path| !path.as_os_str().is_empty()) else {
        return;
    };
    let git_root = parent.parent().unwrap_or(parent);
    let mut paths = vec![
        parent.to_path_buf(),
        git_root.join("libexec").join("git-core"),
        git_root.join("mingw64").join("libexec").join("git-core"),
        git_root.join("usr").join("bin"),
        git_root.join("cmd"),
    ];
    paths.retain(|path| path.is_dir());
    if let Some(existing) = env::var_os("PATH") {
        paths.extend(env::split_paths(&existing));
    }
    if let Ok(joined) = env::join_paths(paths) {
        command.env("PATH", joined);
    }
}

fn resolve_git_binary() -> GitBinaryResolution {
    let bundled_candidates = bundled_git_candidates();
    if let Some(path) = env::var_os("CHATOS_GIT_BIN")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return GitBinaryResolution {
            path,
            source: GitBinarySource::Env,
            bundled_candidates,
        };
    }
    if let Some(path) = bundled_candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
    {
        return GitBinaryResolution {
            path,
            source: GitBinarySource::Bundled,
            bundled_candidates,
        };
    }
    GitBinaryResolution {
        path: PathBuf::from("git"),
        source: GitBinarySource::System,
        bundled_candidates,
    }
}

fn bundled_git_candidates() -> Vec<PathBuf> {
    let git_exe = git_executable_name();
    let mut candidates = Vec::new();
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            candidates.push(exe_dir.join("git").join("bin").join(git_exe));
            candidates.push(exe_dir.join("git").join("cmd").join(git_exe));
            candidates.push(
                exe_dir
                    .join("resources")
                    .join("git")
                    .join("bin")
                    .join(git_exe),
            );
            candidates.push(
                exe_dir
                    .join("resources")
                    .join("git")
                    .join("cmd")
                    .join(git_exe),
            );
            candidates.push(
                exe_dir
                    .parent()
                    .unwrap_or(exe_dir)
                    .join("Resources")
                    .join("git")
                    .join("bin")
                    .join(git_exe),
            );
            candidates.push(
                exe_dir
                    .parent()
                    .unwrap_or(exe_dir)
                    .join("Resources")
                    .join("git")
                    .join("cmd")
                    .join(git_exe),
            );
        }
    }
    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("resources").join("git").join("bin").join(git_exe));
        candidates.push(cwd.join("resources").join("git").join("cmd").join(git_exe));
        candidates.push(
            cwd.join("resources")
                .join("git")
                .join("mingw64")
                .join("bin")
                .join(git_exe),
        );
        candidates.push(
            cwd.join("resources")
                .join("git")
                .join("usr")
                .join("bin")
                .join(git_exe),
        );
        candidates.push(cwd.join("vendor").join("git").join("bin").join(git_exe));
        candidates.push(cwd.join("vendor").join("git").join("cmd").join(git_exe));
    }
    candidates
}

fn git_executable_name() -> &'static str {
    if cfg!(windows) {
        "git.exe"
    } else {
        "git"
    }
}

fn git_launch_error(git_bin: &Path, err: std::io::Error) -> String {
    format!(
        "执行 Git 客户端失败: {}。请安装 Git，或设置 CHATOS_GIT_BIN 指向内置 Git 可执行文件。当前尝试路径: {}",
        err,
        git_bin.to_string_lossy()
    )
}

async fn validate_branch_name(repo_root: &Path, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("分支名不能为空".to_string());
    }
    if name.starts_with('-') || name.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        return Err("分支名不合法".to_string());
    }
    git_output(
        repo_root,
        vec!["check-ref-format", "--branch", name],
        DEFAULT_GIT_TIMEOUT,
    )
    .await
    .map(|_| ())
    .map_err(|_| "分支名不合法".to_string())
}

fn merge_args<'a>(mode: Option<&str>, branch: &'a str) -> Result<Vec<&'a str>, String> {
    match mode.unwrap_or("default").trim() {
        "" | "default" => Ok(vec!["merge", "--no-edit", branch]),
        "no-ff" => Ok(vec!["merge", "--no-ff", "--no-edit", branch]),
        "ff-only" => Ok(vec!["merge", "--ff-only", branch]),
        _ => Err("不支持的 merge 模式".to_string()),
    }
}

async fn is_tracked_path(repo_root: &Path, path: &str) -> bool {
    git_output(
        repo_root,
        vec!["ls-files", "--error-unmatch", "--", path],
        DEFAULT_GIT_TIMEOUT,
    )
    .await
    .is_ok()
}

async fn untracked_file_patch(repo_root: &Path, path: &str) -> Result<String, String> {
    let absolute_path = repo_root.join(path);
    let metadata = fs::symlink_metadata(absolute_path.as_path())
        .await
        .map_err(|err| format!("读取未跟踪文件失败: {}", err))?;
    if metadata.file_type().is_symlink() {
        return Err("未跟踪符号链接暂不支持预览 diff".to_string());
    }
    if !metadata.is_file() {
        return Err("只能预览文件 diff".to_string());
    }
    let canonical_path = std::fs::canonicalize(absolute_path.as_path())
        .map_err(|err| format!("解析未跟踪文件路径失败: {}", err))?;
    let canonical_repo_root = std::fs::canonicalize(repo_root)
        .map_err(|err| format!("解析 Git 仓库路径失败: {}", err))?;
    if !canonical_path.starts_with(canonical_repo_root.as_path()) {
        return Err("未跟踪文件不在 Git 仓库内，已拒绝预览".to_string());
    }
    if metadata.len() > MAX_UNTRACKED_DIFF_BYTES {
        return Ok(format!(
            "diff --git a/{0} b/{0}\nnew file mode 100644\n--- /dev/null\n+++ b/{0}\n@@ -0,0 +1 @@\n+未跟踪文件过大，已跳过内容预览（{1} bytes）。\n",
            path,
            metadata.len()
        ));
    }
    let bytes = fs::read(canonical_path.as_path())
        .await
        .map_err(|err| format!("读取未跟踪文件失败: {}", err))?;
    let content = match String::from_utf8(bytes) {
        Ok(value) => value,
        Err(_) => {
            return Ok(format!(
                "diff --git a/{0} b/{0}\nnew file mode 100644\nBinary file b/{0} differs\n",
                path
            ));
        }
    };
    let line_count = content.lines().count().max(1);
    let mut patch = format!(
        "diff --git a/{0} b/{0}\nnew file mode 100644\n--- /dev/null\n+++ b/{0}\n@@ -0,0 +1,{1} @@\n",
        path, line_count
    );
    if content.is_empty() {
        return Ok(patch);
    }
    for line in content.lines() {
        patch.push('+');
        patch.push_str(line);
        patch.push('\n');
    }
    if !content.ends_with('\n') {
        patch.push_str("\\ No newline at end of file\n");
    }
    Ok(patch)
}

fn validate_relative_paths(paths: &[String]) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Err("paths 不能为空".to_string());
    }
    let mut out = Vec::new();
    for raw in paths {
        let path = raw.trim().replace('\\', "/");
        if path.is_empty() {
            continue;
        }
        let parsed = Path::new(&path);
        if parsed.is_absolute()
            || parsed.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err("paths 只能是仓库内相对路径".to_string());
        }
        out.push(path);
    }
    if out.is_empty() {
        return Err("paths 不能为空".to_string());
    }
    Ok(out)
}

fn ensure_safe_ref(value: &str, label: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() || value.starts_with('-') || value.chars().any(|ch| ch.is_control()) {
        return Err(format!("{} 不合法", label));
    }
    Ok(())
}

async fn ahead_behind(
    repo_root: &Path,
    branch: &str,
    upstream: &str,
) -> Result<(usize, usize), String> {
    let range = format!("{}...{}", branch, upstream);
    let output = git_output(
        repo_root,
        vec!["rev-list", "--left-right", "--count", range.as_str()],
        DEFAULT_GIT_TIMEOUT,
    )
    .await?;
    let mut parts = output.stdout.split_whitespace();
    let ahead = parts
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let behind = parts
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    Ok((ahead, behind))
}

fn summary_from_status(repo_root: PathBuf, status: &str) -> GitSummary {
    let mut head = None;
    let mut current_branch = None;
    let mut detached = false;
    let mut upstream = None;
    let mut ahead = 0usize;
    let mut behind = 0usize;
    let mut changes = GitChangeCounts {
        staged: 0,
        unstaged: 0,
        untracked: 0,
        conflicted: 0,
    };

    for line in status.lines() {
        if let Some(value) = line.strip_prefix("# branch.oid ") {
            head = non_empty(value);
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.head ") {
            let value = value.trim();
            if value == "(detached)" {
                detached = true;
            } else {
                current_branch = non_empty(value);
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.upstream ") {
            upstream = non_empty(value);
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.ab ") {
            for part in value.split_whitespace() {
                if let Some(raw) = part.strip_prefix('+') {
                    ahead = raw.parse().unwrap_or(0);
                } else if let Some(raw) = part.strip_prefix('-') {
                    behind = raw.parse().unwrap_or(0);
                }
            }
            continue;
        }
        count_status_line(line, &mut changes);
    }

    let dirty = changes.staged > 0
        || changes.unstaged > 0
        || changes.untracked > 0
        || changes.conflicted > 0;
    GitSummary {
        is_repo: true,
        root: Some(repo_root.to_string_lossy().to_string()),
        worktree_root: Some(repo_root.to_string_lossy().to_string()),
        head,
        current_branch,
        detached,
        upstream,
        ahead,
        behind,
        dirty,
        operation_state: detect_operation_state(repo_root.as_path()),
        changes,
    }
}

fn non_repo_summary() -> GitSummary {
    GitSummary {
        is_repo: false,
        root: None,
        worktree_root: None,
        head: None,
        current_branch: None,
        detached: false,
        upstream: None,
        ahead: 0,
        behind: 0,
        dirty: false,
        operation_state: None,
        changes: GitChangeCounts {
            staged: 0,
            unstaged: 0,
            untracked: 0,
            conflicted: 0,
        },
    }
}

fn count_status_line(line: &str, changes: &mut GitChangeCounts) {
    if let Some(rest) = line.strip_prefix("1 ").or_else(|| line.strip_prefix("2 ")) {
        count_xy(rest, changes);
    } else if line.starts_with("u ") {
        changes.conflicted += 1;
    } else if line.starts_with("? ") {
        changes.untracked += 1;
    }
}

fn count_xy(rest: &str, changes: &mut GitChangeCounts) {
    let xy = rest.split_whitespace().next().unwrap_or("");
    let mut chars = xy.chars();
    let staged = chars.next().unwrap_or('.');
    let unstaged = chars.next().unwrap_or('.');
    if staged != '.' {
        changes.staged += 1;
    }
    if unstaged != '.' {
        changes.unstaged += 1;
    }
}

fn parse_status_files(raw: &str) -> Vec<GitStatusFile> {
    let mut files = Vec::new();
    let mut parts = raw.split('\0').peekable();
    while let Some(record) = parts.next() {
        if record.is_empty() || record.starts_with('#') {
            continue;
        }
        if let Some(path) = record.strip_prefix("? ") {
            files.push(GitStatusFile {
                path: path.to_string(),
                old_path: None,
                status: "untracked".to_string(),
                staged: false,
                unstaged: false,
                conflicted: false,
            });
            continue;
        }
        if record.starts_with("u ") {
            if let Some((xy, path)) = parse_status_record(record, 10) {
                files.push(GitStatusFile {
                    path,
                    old_path: None,
                    status: status_from_xy(xy, true),
                    staged: true,
                    unstaged: true,
                    conflicted: true,
                });
            }
            continue;
        }
        if record.starts_with("2 ") {
            if let Some((xy, path)) = parse_status_record(record, 9) {
                let old_path = parts.next().map(ToOwned::to_owned);
                files.push(GitStatusFile {
                    path,
                    old_path,
                    status: status_from_xy(xy, false),
                    staged: xy.chars().next().unwrap_or('.') != '.',
                    unstaged: xy.chars().nth(1).unwrap_or('.') != '.',
                    conflicted: false,
                });
            }
            continue;
        }
        if record.starts_with("1 ") {
            if let Some((xy, path)) = parse_status_record(record, 8) {
                files.push(GitStatusFile {
                    path,
                    old_path: None,
                    status: status_from_xy(xy, false),
                    staged: xy.chars().next().unwrap_or('.') != '.',
                    unstaged: xy.chars().nth(1).unwrap_or('.') != '.',
                    conflicted: false,
                });
            }
        }
    }
    files
}

fn parse_name_status_z(raw: &str) -> Vec<GitDiffFile> {
    let mut files = Vec::new();
    let mut parts = raw.split('\0').filter(|part| !part.is_empty()).peekable();
    while let Some(status) = parts.next() {
        let Some(path) = parts.next() else {
            break;
        };
        let code = status.chars().next().unwrap_or('M');
        if matches!(code, 'R' | 'C') {
            let Some(new_path) = parts.next() else {
                break;
            };
            files.push(GitDiffFile {
                path: new_path.to_string(),
                old_path: Some(path.to_string()),
                status: status_from_name_status(code),
            });
        } else {
            files.push(GitDiffFile {
                path: path.to_string(),
                old_path: None,
                status: status_from_name_status(code),
            });
        }
    }
    files
}

fn parse_compare_commits(raw: &str) -> Vec<GitCompareCommit> {
    raw.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\x1f');
            let side = match parts.next()?.trim() {
                "<" => "current",
                ">" => "target",
                _ => "unknown",
            };
            let hash = parts.next()?.trim();
            let subject = parts.next()?.trim();
            if hash.is_empty() {
                return None;
            }
            Some(GitCompareCommit {
                side: side.to_string(),
                hash: hash.to_string(),
                subject: subject.to_string(),
            })
        })
        .collect()
}

fn parse_status_record(record: &str, space_count_before_path: usize) -> Option<(&str, String)> {
    let xy = record.split_whitespace().nth(1)?;
    let mut seen = 0usize;
    for (index, ch) in record.char_indices() {
        if ch == ' ' {
            seen += 1;
            if seen == space_count_before_path {
                return Some((xy, record[index + 1..].to_string()));
            }
        }
    }
    None
}

fn status_from_name_status(code: char) -> String {
    match code {
        'A' => "added",
        'D' => "deleted",
        'R' => "renamed",
        'C' => "copied",
        'U' => "conflicted",
        _ => "modified",
    }
    .to_string()
}

fn status_from_xy(xy: &str, conflicted: bool) -> String {
    if conflicted {
        return "conflicted".to_string();
    }
    if xy.contains('R') {
        return "renamed".to_string();
    }
    if xy.contains('C') {
        return "copied".to_string();
    }
    if xy.contains('D') {
        return "deleted".to_string();
    }
    if xy.contains('A') {
        return "added".to_string();
    }
    "modified".to_string()
}

fn split_remote_branch(name: &str) -> (Option<String>, Option<String>) {
    let mut parts = name.splitn(2, '/');
    let remote = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let short_name = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    (remote, short_name)
}

fn detect_operation_state(repo_root: &Path) -> Option<String> {
    let git_dir = repo_root.join(".git");
    if git_dir.join("MERGE_HEAD").exists() {
        return Some("merge".to_string());
    }
    if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        return Some("rebase".to_string());
    }
    if git_dir.join("CHERRY_PICK_HEAD").exists() {
        return Some("cherry-pick".to_string());
    }
    None
}

fn compact_output(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.chars().take(1200).collect())
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        merge_args, parse_compare_commits, parse_name_status_z, parse_status_files,
        summary_from_status,
    };
    use std::path::PathBuf;

    #[test]
    fn parses_summary_from_porcelain_v2_branch_status() {
        let status = "\
# branch.oid abc123
# branch.head main
# branch.upstream origin/main
# branch.ab +2 -1
1 .M N... 100644 100644 100644 abc abc src/main.rs
1 A. N... 000000 100644 100644 000 abc src/new.rs
? src/loose.rs
";
        let summary = summary_from_status(PathBuf::from("/tmp/repo"), status);
        assert!(summary.is_repo);
        assert_eq!(summary.current_branch.as_deref(), Some("main"));
        assert_eq!(summary.ahead, 2);
        assert_eq!(summary.behind, 1);
        assert_eq!(summary.changes.unstaged, 1);
        assert_eq!(summary.changes.staged, 1);
        assert_eq!(summary.changes.untracked, 1);
        assert!(summary.dirty);
    }

    #[test]
    fn parses_porcelain_v2_z_status_files() {
        let raw = "1 .M N... 100644 100644 100644 abc abc src/main.rs\0\
2 R. N... 100644 100644 100644 abc def R100 src/new name.rs\0src/old name.rs\0\
? src/loose file.rs\0";
        let files = parse_status_files(raw);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].status, "modified");
        assert!(!files[0].staged);
        assert!(files[0].unstaged);
        assert_eq!(files[1].path, "src/new name.rs");
        assert_eq!(files[1].old_path.as_deref(), Some("src/old name.rs"));
        assert_eq!(files[1].status, "renamed");
        assert!(files[1].staged);
        assert!(!files[1].unstaged);
        assert_eq!(files[2].status, "untracked");
    }

    #[test]
    fn parses_name_status_z_diff_files() {
        let raw = "M\0src/main.rs\0R100\0src/old.rs\0src/new.rs\0D\0src/deleted.rs\0";
        let files = parse_name_status_z(raw);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].status, "modified");
        assert_eq!(files[1].status, "renamed");
        assert_eq!(files[1].old_path.as_deref(), Some("src/old.rs"));
        assert_eq!(files[1].path, "src/new.rs");
        assert_eq!(files[2].status, "deleted");
    }

    #[test]
    fn parses_compare_commits() {
        let commits = parse_compare_commits(
            "<\u{1f}abc123\u{1f}current only\n>\u{1f}def456\u{1f}target only\n",
        );
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].side, "current");
        assert_eq!(commits[0].hash, "abc123");
        assert_eq!(commits[1].side, "target");
    }

    #[test]
    fn builds_merge_args_without_editor() {
        assert_eq!(
            merge_args(None, "feature").unwrap(),
            vec!["merge", "--no-edit", "feature"]
        );
        assert_eq!(
            merge_args(Some("no-ff"), "feature").unwrap(),
            vec!["merge", "--no-ff", "--no-edit", "feature"]
        );
        assert_eq!(
            merge_args(Some("ff-only"), "feature").unwrap(),
            vec!["merge", "--ff-only", "feature"]
        );
        assert!(merge_args(Some("squash"), "feature").is_err());
    }
}
