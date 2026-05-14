use super::contracts::*;
use super::process::{git_output, git_output_with_status, DEFAULT_GIT_TIMEOUT, REMOTE_GIT_TIMEOUT};
use super::shared::{
    action_result, action_result_with_status, read_repo_summary, require_current_branch,
    stage_paths, unstage_paths,
};
use super::validation::{
    ensure_safe_ref, merge_args, require_repo_root, validate_branch_name, validate_relative_paths,
};

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
    action_result(repo_root.as_path(), output).await
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
    action_result(repo_root.as_path(), output).await
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
        None => require_current_branch(repo_root.as_path(), "push").await?,
    };
    ensure_safe_ref(branch.as_str(), "branch")?;
    let args = if request.set_upstream.unwrap_or(false) {
        vec!["push", "-u", remote, branch.as_str()]
    } else {
        vec!["push", remote, branch.as_str()]
    };
    let output = git_output(repo_root.as_path(), args, REMOTE_GIT_TIMEOUT).await?;
    action_result(repo_root.as_path(), output).await
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
    action_result(repo_root.as_path(), output).await
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
    action_result(repo_root.as_path(), output).await
}

pub async fn merge(request: GitMergeRequest) -> Result<GitActionResult, String> {
    let repo_root = require_repo_root(&request.root).await?;
    let branch = request.branch.trim();
    if branch.is_empty() {
        return Err("branch 不能为空".to_string());
    }
    ensure_safe_ref(branch, "branch")?;

    let current_summary = read_repo_summary(repo_root.as_path()).await?;
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
    action_result_with_status(repo_root.as_path(), output).await
}

pub async fn stage(request: GitPathRequest) -> Result<GitActionResult, String> {
    let repo_root = require_repo_root(&request.root).await?;
    let paths = validate_relative_paths(&request.paths)?;
    let output = stage_paths(repo_root.as_path(), &paths).await?;
    action_result(repo_root.as_path(), output).await
}

pub async fn unstage(request: GitPathRequest) -> Result<GitActionResult, String> {
    let repo_root = require_repo_root(&request.root).await?;
    let paths = validate_relative_paths(&request.paths)?;
    let output = unstage_paths(repo_root.as_path(), &paths).await?;
    action_result(repo_root.as_path(), output).await
}

pub async fn commit(request: GitCommitRequest) -> Result<GitActionResult, String> {
    let repo_root = require_repo_root(&request.root).await?;
    let message = request.message.trim();
    if message.is_empty() {
        return Err("commit message 不能为空".to_string());
    }
    if let Some(paths) = request.paths.as_ref().filter(|paths| !paths.is_empty()) {
        let paths = validate_relative_paths(paths)?;
        stage_paths(repo_root.as_path(), &paths).await?;
    }
    let output = git_output(
        repo_root.as_path(),
        vec!["commit", "-m", message],
        DEFAULT_GIT_TIMEOUT,
    )
    .await?;
    action_result(repo_root.as_path(), output).await
}
