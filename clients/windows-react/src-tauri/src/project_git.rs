use serde::Serialize;
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::{AppHandle, Manager};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    time::timeout,
};

use crate::{
    host_lifecycle::validate_owner, platform_execution_context::resolve_project_root,
    terminal_runtime::process::ProcessTermination,
};

const DEFAULT_GIT_TIMEOUT: Duration = Duration::from_secs(120);
const NETWORK_GIT_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const TERMINATION_GRACE_PERIOD: Duration = Duration::from_secs(1);
const MAXIMUM_GIT_OUTPUT_BYTES: usize = 16 * 1_024 * 1_024;
const MAXIMUM_DIFF_BYTES: usize = 4 * 1_024 * 1_024;
const MAXIMUM_PATHS_PER_MUTATION: usize = 1_000;
const MAXIMUM_PORTABLE_PATH_BYTES: usize = 16 * 1_024;
const MAXIMUM_BRANCH_BYTES: usize = 1_024;
const MAXIMUM_REMOTE_NAME_BYTES: usize = 1_024;
const MAXIMUM_REMOTE_URL_BYTES: usize = 16 * 1_024;
const MAXIMUM_COMMIT_MESSAGE_BYTES: usize = 64 * 1_024;
const MAXIMUM_METADATA_FIELD_BYTES: usize = 64 * 1_024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGitChange {
    path: String,
    original_path: Option<String>,
    index_status: String,
    work_tree_status: String,
    kind: ProjectGitChangeKind,
    has_staged_changes: bool,
    has_working_tree_changes: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum ProjectGitChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Conflicted,
    TypeChanged,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGitBranch {
    name: String,
    is_current: bool,
    upstream: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGitCommit {
    id: String,
    short_id: String,
    parent_ids: Vec<String>,
    author: String,
    authored_at: Option<String>,
    decorations: Vec<String>,
    subject: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGitRemote {
    name: String,
    url: String,
    url_is_redacted: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGitSnapshot {
    project_id: String,
    is_repository: bool,
    current_branch: Option<String>,
    detached_commit: Option<String>,
    upstream: Option<String>,
    ahead_count: u64,
    behind_count: u64,
    has_head: bool,
    changes: Vec<ProjectGitChange>,
    branches: Vec<ProjectGitBranch>,
    commits: Vec<ProjectGitCommit>,
    remotes: Vec<ProjectGitRemote>,
}

impl ProjectGitSnapshot {
    fn unavailable(project_id: &str) -> Self {
        Self {
            project_id: project_id.to_owned(),
            is_repository: false,
            current_branch: None,
            detached_commit: None,
            upstream: None,
            ahead_count: 0,
            behind_count: 0,
            has_head: false,
            changes: Vec::new(),
            branches: Vec::new(),
            commits: Vec::new(),
            remotes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGitDiff {
    project_id: String,
    path: String,
    is_staged: bool,
    content: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGitMutationResult {
    project_id: String,
    snapshot: Option<ProjectGitSnapshot>,
    committed_without_refresh: bool,
}

#[derive(Clone)]
struct GitProjectContext {
    project_id: String,
    project_root: PathBuf,
    workspace_root: PathBuf,
}

#[derive(Debug, Clone)]
struct GitRepositoryContext {
    root: PathBuf,
}

struct RepositorySnapshot {
    repository: GitRepositoryContext,
    snapshot: ProjectGitSnapshot,
}

struct GitProcessOutput {
    exit_code: i32,
    stdout: Vec<u8>,
}

#[tauri::command]
pub async fn load_project_git_snapshot(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
) -> Result<ProjectGitSnapshot, String> {
    let context = resolve_git_project(&app, owner_user_id, project_id).await?;
    load_snapshot(&context).await
}

#[tauri::command]
pub async fn load_project_git_diff(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
    path: String,
    staged: bool,
) -> Result<ProjectGitDiff, String> {
    let context = resolve_git_project(&app, owner_user_id, project_id).await?;
    let path = normalized_relative_path(&path)?;
    let current = required_snapshot(&context).await?;
    let change = current
        .snapshot
        .changes
        .iter()
        .find(|change| change.path == path)
        .ok_or_else(invalid_change)?;
    if staged && !change.has_staged_changes || !staged && !change.has_working_tree_changes {
        return Err(invalid_change());
    }
    let arguments = if change.kind == ProjectGitChangeKind::Untracked && !staged {
        vec![
            "diff".to_owned(),
            "--no-index".to_owned(),
            "--no-color".to_owned(),
            "--no-ext-diff".to_owned(),
            "--".to_owned(),
            null_device_path().to_owned(),
            current
                .repository
                .root
                .join(&path)
                .to_string_lossy()
                .into_owned(),
        ]
    } else {
        let mut arguments = vec![
            "diff".to_owned(),
            "--no-ext-diff".to_owned(),
            "--no-color".to_owned(),
        ];
        if staged {
            arguments.push("--cached".to_owned());
        }
        arguments.extend(["--".to_owned(), path.clone()]);
        arguments
    };
    let output = run_git(
        &arguments,
        &current.repository.root,
        &[0, 1],
        DEFAULT_GIT_TIMEOUT,
        MAXIMUM_DIFF_BYTES,
        "无法读取 Git 差异。",
    )
    .await?;
    Ok(ProjectGitDiff {
        project_id: context.project_id,
        path,
        is_staged: staged,
        content: output_text(output.stdout)?,
    })
}

#[tauri::command]
pub async fn initialize_project_git_repository(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
) -> Result<ProjectGitMutationResult, String> {
    let context = resolve_git_project(&app, owner_user_id, project_id).await?;
    run_git(
        &["init".into(), "-b".into(), "main".into()],
        &context.project_root,
        &[0],
        DEFAULT_GIT_TIMEOUT,
        MAXIMUM_GIT_OUTPUT_BYTES,
        "无法初始化 Git 仓库。",
    )
    .await?;
    refresh_after_mutation(&context).await
}

#[tauri::command]
pub async fn stage_project_git_paths(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
    paths: Vec<String>,
) -> Result<ProjectGitMutationResult, String> {
    let context = resolve_git_project(&app, owner_user_id, project_id).await?;
    let current = required_snapshot(&context).await?;
    let paths = validate_requested_paths(&paths, &current.snapshot, false)?;
    if !paths.is_empty() {
        let mut arguments = vec!["add".to_owned(), "--".to_owned()];
        arguments.extend(paths);
        run_git(
            &arguments,
            &current.repository.root,
            &[0],
            DEFAULT_GIT_TIMEOUT,
            MAXIMUM_GIT_OUTPUT_BYTES,
            "无法暂存所选变更。",
        )
        .await?;
    }
    refresh_after_mutation(&context).await
}

#[tauri::command]
pub async fn unstage_project_git_paths(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
    paths: Vec<String>,
) -> Result<ProjectGitMutationResult, String> {
    let context = resolve_git_project(&app, owner_user_id, project_id).await?;
    let current = required_snapshot(&context).await?;
    let paths = validate_requested_paths(&paths, &current.snapshot, true)?;
    if !paths.is_empty() {
        let mut arguments = if current.snapshot.has_head {
            vec!["restore".to_owned(), "--staged".to_owned(), "--".to_owned()]
        } else {
            vec!["rm".to_owned(), "--cached".to_owned(), "--".to_owned()]
        };
        arguments.extend(paths);
        run_git(
            &arguments,
            &current.repository.root,
            &[0],
            DEFAULT_GIT_TIMEOUT,
            MAXIMUM_GIT_OUTPUT_BYTES,
            "无法取消暂存所选变更。",
        )
        .await?;
    }
    refresh_after_mutation(&context).await
}

#[tauri::command]
pub async fn commit_project_git(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
    message: String,
) -> Result<ProjectGitMutationResult, String> {
    let message = normalized_commit_message(message)?;
    let context = resolve_git_project(&app, owner_user_id, project_id).await?;
    let repository = required_repository(&context).await?;
    run_git(
        &["commit".into(), "-m".into(), message],
        &repository.root,
        &[0],
        DEFAULT_GIT_TIMEOUT,
        MAXIMUM_GIT_OUTPUT_BYTES,
        "无法提交暂存的 Git 变更。",
    )
    .await?;
    refresh_after_mutation(&context).await
}

#[tauri::command]
pub async fn switch_project_git_branch(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
    branch: String,
) -> Result<ProjectGitMutationResult, String> {
    let context = resolve_git_project(&app, owner_user_id, project_id).await?;
    let repository = required_repository(&context).await?;
    let branch = validated_branch(&repository, branch).await?;
    run_git(
        &["switch".into(), branch],
        &repository.root,
        &[0],
        DEFAULT_GIT_TIMEOUT,
        MAXIMUM_GIT_OUTPUT_BYTES,
        "无法切换 Git 分支。",
    )
    .await?;
    refresh_after_mutation(&context).await
}

#[tauri::command]
pub async fn create_project_git_branch(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
    name: String,
    switch_to_branch: bool,
) -> Result<ProjectGitMutationResult, String> {
    let context = resolve_git_project(&app, owner_user_id, project_id).await?;
    let repository = required_repository(&context).await?;
    let name = validated_branch(&repository, name).await?;
    let arguments = if switch_to_branch {
        vec!["switch".into(), "-c".into(), name]
    } else {
        vec!["branch".into(), name]
    };
    run_git(
        &arguments,
        &repository.root,
        &[0],
        DEFAULT_GIT_TIMEOUT,
        MAXIMUM_GIT_OUTPUT_BYTES,
        "无法创建 Git 分支。",
    )
    .await?;
    refresh_after_mutation(&context).await
}

#[tauri::command]
pub async fn merge_project_git_branch(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
    branch: String,
) -> Result<ProjectGitMutationResult, String> {
    let context = resolve_git_project(&app, owner_user_id, project_id).await?;
    let repository = required_repository(&context).await?;
    let branch = validated_branch(&repository, branch).await?;
    run_git(
        &["merge".into(), "--no-edit".into(), branch],
        &repository.root,
        &[0],
        DEFAULT_GIT_TIMEOUT,
        MAXIMUM_GIT_OUTPUT_BYTES,
        "无法合并 Git 分支。",
    )
    .await?;
    refresh_after_mutation(&context).await
}

#[tauri::command]
pub async fn save_project_git_remote(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
    original_name: Option<String>,
    name: String,
    url: Option<String>,
) -> Result<ProjectGitMutationResult, String> {
    let context = resolve_git_project(&app, owner_user_id, project_id).await?;
    let current = required_snapshot(&context).await?;
    let name = validated_remote_name(&current.repository, name).await?;
    let url = url.map(normalized_remote_url).transpose()?;
    if let Some(original_name) = original_name {
        let original_name = validated_remote_name(&current.repository, original_name).await?;
        if !current
            .snapshot
            .remotes
            .iter()
            .any(|remote| remote.name == original_name)
        {
            return Err(invalid_remote());
        }
        if original_name != name {
            run_git(
                &[
                    "remote".into(),
                    "rename".into(),
                    original_name,
                    name.clone(),
                ],
                &current.repository.root,
                &[0],
                DEFAULT_GIT_TIMEOUT,
                MAXIMUM_GIT_OUTPUT_BYTES,
                "无法重命名 Git 远端。",
            )
            .await?;
        }
        if let Some(url) = url {
            run_git(
                &["remote".into(), "set-url".into(), name, url],
                &current.repository.root,
                &[0],
                DEFAULT_GIT_TIMEOUT,
                MAXIMUM_GIT_OUTPUT_BYTES,
                "无法更新 Git 远端地址。",
            )
            .await?;
        }
    } else {
        let url = url.ok_or_else(invalid_remote)?;
        run_git(
            &["remote".into(), "add".into(), name, url],
            &current.repository.root,
            &[0],
            DEFAULT_GIT_TIMEOUT,
            MAXIMUM_GIT_OUTPUT_BYTES,
            "无法添加 Git 远端。",
        )
        .await?;
    }
    refresh_after_mutation(&context).await
}

#[tauri::command]
pub async fn remove_project_git_remote(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
    name: String,
) -> Result<ProjectGitMutationResult, String> {
    let context = resolve_git_project(&app, owner_user_id, project_id).await?;
    let current = required_snapshot(&context).await?;
    let name = validated_remote_name(&current.repository, name).await?;
    if !current
        .snapshot
        .remotes
        .iter()
        .any(|remote| remote.name == name)
    {
        return Err(invalid_remote());
    }
    run_git(
        &["remote".into(), "remove".into(), name],
        &current.repository.root,
        &[0],
        DEFAULT_GIT_TIMEOUT,
        MAXIMUM_GIT_OUTPUT_BYTES,
        "无法移除 Git 远端。",
    )
    .await?;
    refresh_after_mutation(&context).await
}

#[tauri::command]
pub async fn pull_project_git(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
) -> Result<ProjectGitMutationResult, String> {
    let context = resolve_git_project(&app, owner_user_id, project_id).await?;
    let repository = required_repository(&context).await?;
    run_git(
        &["pull".into(), "--ff-only".into()],
        &repository.root,
        &[0],
        NETWORK_GIT_TIMEOUT,
        MAXIMUM_GIT_OUTPUT_BYTES,
        "无法快进拉取 Git 远端变更。",
    )
    .await?;
    refresh_after_mutation(&context).await
}

#[tauri::command]
pub async fn push_project_git(
    app: AppHandle,
    owner_user_id: String,
    project_id: String,
) -> Result<ProjectGitMutationResult, String> {
    let context = resolve_git_project(&app, owner_user_id, project_id).await?;
    let current = required_snapshot(&context).await?;
    let arguments = if current.snapshot.upstream.is_some() {
        vec!["push".to_owned()]
    } else {
        let branch = current
            .snapshot
            .current_branch
            .clone()
            .ok_or_else(|| "当前 Git HEAD 没有可发布的分支。".to_owned())?;
        let remote = current
            .snapshot
            .remotes
            .first()
            .ok_or_else(|| "项目尚未配置 Git 远端。".to_owned())?;
        vec![
            "push".to_owned(),
            "--set-upstream".to_owned(),
            remote.name.clone(),
            branch,
        ]
    };
    run_git(
        &arguments,
        &current.repository.root,
        &[0],
        NETWORK_GIT_TIMEOUT,
        MAXIMUM_GIT_OUTPUT_BYTES,
        "无法推送 Git 变更。",
    )
    .await?;
    refresh_after_mutation(&context).await
}

async fn resolve_git_project(
    app: &AppHandle,
    owner_user_id: String,
    project_id: String,
) -> Result<GitProjectContext, String> {
    validate_owner(&owner_user_id)?;
    let project_id = required_identifier("project_id", project_id)?;
    let app_data_directory = app
        .path()
        .app_data_dir()
        .map_err(|_| project_unavailable())?;
    let resolved_project_id = project_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let resolved = resolve_project_root(
            &app_data_directory.join("NativeConnector"),
            &owner_user_id,
            &resolved_project_id,
        )?;
        Ok(GitProjectContext {
            project_id: resolved_project_id,
            project_root: resolved.root,
            workspace_root: resolved.workspace_root,
        })
    })
    .await
    .map_err(|_| project_unavailable())?
}

async fn load_snapshot(context: &GitProjectContext) -> Result<ProjectGitSnapshot, String> {
    let Some(repository) = repository_context(context).await? else {
        return Ok(ProjectGitSnapshot::unavailable(&context.project_id));
    };
    load_repository_snapshot(context, &repository).await
}

async fn required_repository(context: &GitProjectContext) -> Result<GitRepositoryContext, String> {
    repository_context(context)
        .await?
        .ok_or_else(|| "该项目还不是 Git 仓库。".to_owned())
}

async fn required_snapshot(context: &GitProjectContext) -> Result<RepositorySnapshot, String> {
    let repository = required_repository(context).await?;
    let snapshot = load_repository_snapshot(context, &repository).await?;
    Ok(RepositorySnapshot {
        repository,
        snapshot,
    })
}

async fn repository_context(
    context: &GitProjectContext,
) -> Result<Option<GitRepositoryContext>, String> {
    let output = run_git(
        &["rev-parse".into(), "--show-toplevel".into()],
        &context.project_root,
        &[0, 128],
        DEFAULT_GIT_TIMEOUT,
        MAXIMUM_GIT_OUTPUT_BYTES,
        "无法确认 Git 仓库边界。",
    )
    .await?;
    if output.exit_code != 0 {
        return Ok(None);
    }
    let value = output_text(output.stdout)?;
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(repository_outside_workspace());
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(repository_outside_workspace());
    }
    let canonical = std::fs::canonicalize(path).map_err(|_| repository_outside_workspace())?;
    if !canonical.starts_with(&context.workspace_root)
        || !canonical
            .metadata()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
    {
        return Err(repository_outside_workspace());
    }
    Ok(Some(GitRepositoryContext { root: canonical }))
}

async fn load_repository_snapshot(
    context: &GitProjectContext,
    repository: &GitRepositoryContext,
) -> Result<ProjectGitSnapshot, String> {
    let head = run_git(
        &["rev-parse".into(), "--verify".into(), "HEAD".into()],
        &repository.root,
        &[0, 128],
        DEFAULT_GIT_TIMEOUT,
        MAXIMUM_GIT_OUTPUT_BYTES,
        "无法读取 Git HEAD。",
    )
    .await?;
    let has_head = head.exit_code == 0;
    let branch = run_git(
        &[
            "symbolic-ref".into(),
            "--quiet".into(),
            "--short".into(),
            "HEAD".into(),
        ],
        &repository.root,
        &[0, 1],
        DEFAULT_GIT_TIMEOUT,
        MAXIMUM_GIT_OUTPUT_BYTES,
        "无法读取当前 Git 分支。",
    )
    .await?;
    let current_branch = if branch.exit_code == 0 {
        optional_safe_line(branch.stdout, MAXIMUM_BRANCH_BYTES)?
    } else {
        None
    };
    let detached_commit = if current_branch.is_none() && has_head {
        optional_safe_line(
            run_git(
                &["rev-parse".into(), "--short".into(), "HEAD".into()],
                &repository.root,
                &[0],
                DEFAULT_GIT_TIMEOUT,
                MAXIMUM_GIT_OUTPUT_BYTES,
                "无法读取 Git HEAD。",
            )
            .await?
            .stdout,
            128,
        )?
    } else {
        None
    };

    let changes = parse_changes(
        run_git(
            &[
                "status".into(),
                "--porcelain=v1".into(),
                "-z".into(),
                "--untracked-files=all".into(),
            ],
            &repository.root,
            &[0],
            DEFAULT_GIT_TIMEOUT,
            MAXIMUM_GIT_OUTPUT_BYTES,
            "无法读取 Git 工作区状态。",
        )
        .await?
        .stdout,
    )?;
    let branches = parse_branches(
        run_git(
            &[
                "for-each-ref".into(),
                "--sort=-committerdate".into(),
                "--format=%(refname:short)%00%(HEAD)%00%(upstream:short)".into(),
                "refs/heads".into(),
            ],
            &repository.root,
            &[0],
            DEFAULT_GIT_TIMEOUT,
            MAXIMUM_GIT_OUTPUT_BYTES,
            "无法读取 Git 分支列表。",
        )
        .await?
        .stdout,
    )?;
    let remotes = load_remotes(repository).await?;

    let mut upstream = None;
    let mut ahead_count = 0;
    let mut behind_count = 0;
    if current_branch.is_some() {
        let result = run_git(
            &[
                "rev-parse".into(),
                "--abbrev-ref".into(),
                "--symbolic-full-name".into(),
                "@{upstream}".into(),
            ],
            &repository.root,
            &[0, 128],
            DEFAULT_GIT_TIMEOUT,
            MAXIMUM_GIT_OUTPUT_BYTES,
            "无法读取 Git upstream。",
        )
        .await?;
        if result.exit_code == 0 {
            upstream = optional_safe_line(result.stdout, MAXIMUM_METADATA_FIELD_BYTES)?;
        }
        if upstream.is_some() {
            let counts = output_text(
                run_git(
                    &[
                        "rev-list".into(),
                        "--left-right".into(),
                        "--count".into(),
                        "HEAD...@{upstream}".into(),
                    ],
                    &repository.root,
                    &[0],
                    DEFAULT_GIT_TIMEOUT,
                    MAXIMUM_GIT_OUTPUT_BYTES,
                    "无法读取 Git ahead/behind 状态。",
                )
                .await?
                .stdout,
            )?;
            let values = counts
                .split_whitespace()
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| invalid_git_output())?;
            if values.len() != 2 {
                return Err(invalid_git_output());
            }
            ahead_count = values[0];
            behind_count = values[1];
        }
    }

    let commits = if has_head {
        parse_commits(
            run_git(
                &[
                    "log".into(),
                    "--all".into(),
                    "-n".into(),
                    "80".into(),
                    "--date=iso-strict".into(),
                    "-z".into(),
                    "--pretty=format:%H%x00%h%x00%P%x00%an%x00%aI%x00%D%x00%s".into(),
                ],
                &repository.root,
                &[0],
                DEFAULT_GIT_TIMEOUT,
                MAXIMUM_GIT_OUTPUT_BYTES,
                "无法读取 Git 提交历史。",
            )
            .await?
            .stdout,
        )?
    } else {
        Vec::new()
    };

    Ok(ProjectGitSnapshot {
        project_id: context.project_id.clone(),
        is_repository: true,
        current_branch,
        detached_commit,
        upstream,
        ahead_count,
        behind_count,
        has_head,
        changes,
        branches,
        commits,
        remotes,
    })
}

async fn load_remotes(repository: &GitRepositoryContext) -> Result<Vec<ProjectGitRemote>, String> {
    let names = output_text(
        run_git(
            &["remote".into()],
            &repository.root,
            &[0],
            DEFAULT_GIT_TIMEOUT,
            MAXIMUM_GIT_OUTPUT_BYTES,
            "无法读取 Git 远端列表。",
        )
        .await?
        .stdout,
    )?;
    let mut remotes = Vec::new();
    for name in names.lines() {
        validate_safe_field(name, MAXIMUM_REMOTE_NAME_BYTES)?;
        if name.is_empty() {
            continue;
        }
        let output = run_git(
            &["remote".into(), "get-url".into(), name.to_owned()],
            &repository.root,
            &[0, 2, 128],
            DEFAULT_GIT_TIMEOUT,
            MAXIMUM_GIT_OUTPUT_BYTES,
            "无法读取 Git 远端地址。",
        )
        .await?;
        let url = if output.exit_code == 0 {
            optional_safe_line(output.stdout, MAXIMUM_REMOTE_URL_BYTES)?.unwrap_or_default()
        } else {
            String::new()
        };
        let (url, url_is_redacted) = sanitize_remote_url(&url);
        remotes.push(ProjectGitRemote {
            name: name.to_owned(),
            url,
            url_is_redacted,
        });
    }
    Ok(remotes)
}

async fn refresh_after_mutation(
    context: &GitProjectContext,
) -> Result<ProjectGitMutationResult, String> {
    let snapshot = load_snapshot(context).await.ok();
    Ok(ProjectGitMutationResult {
        project_id: context.project_id.clone(),
        committed_without_refresh: snapshot.is_none(),
        snapshot,
    })
}

async fn validated_branch(
    repository: &GitRepositoryContext,
    branch: String,
) -> Result<String, String> {
    if branch.is_empty()
        || branch.trim() != branch
        || branch.len() > MAXIMUM_BRANCH_BYTES
        || branch.chars().any(char::is_control)
    {
        return Err(invalid_branch());
    }
    let output = run_git(
        &["check-ref-format".into(), "--branch".into(), branch.clone()],
        &repository.root,
        &[0, 1, 128],
        DEFAULT_GIT_TIMEOUT,
        MAXIMUM_GIT_OUTPUT_BYTES,
        "无法验证 Git 分支名称。",
    )
    .await?;
    if output.exit_code != 0 {
        return Err(invalid_branch());
    }
    Ok(branch)
}

async fn validated_remote_name(
    repository: &GitRepositoryContext,
    name: String,
) -> Result<String, String> {
    if name.is_empty()
        || name.trim() != name
        || name.starts_with('-')
        || name.len() > MAXIMUM_REMOTE_NAME_BYTES
        || name.chars().any(char::is_control)
    {
        return Err(invalid_remote());
    }
    let output = run_git(
        &["check-ref-format".into(), format!("refs/remotes/{name}")],
        &repository.root,
        &[0, 1, 128],
        DEFAULT_GIT_TIMEOUT,
        MAXIMUM_GIT_OUTPUT_BYTES,
        "无法验证 Git 远端名称。",
    )
    .await?;
    if output.exit_code != 0 {
        return Err(invalid_remote());
    }
    Ok(name)
}

fn normalized_remote_url(url: String) -> Result<String, String> {
    let value = url.trim();
    if value.is_empty()
        || value != url
        || value.starts_with('-')
        || value.len() > MAXIMUM_REMOTE_URL_BYTES
        || value.chars().any(char::is_control)
    {
        Err(invalid_remote())
    } else {
        Ok(url)
    }
}

fn normalized_commit_message(message: String) -> Result<String, String> {
    let value = message.trim();
    if value.is_empty()
        || value.len() > MAXIMUM_COMMIT_MESSAGE_BYTES
        || value.chars().any(|character| {
            character.is_control() && character != '\n' && character != '\r' && character != '\t'
        })
    {
        Err("请输入有效的 Git 提交说明。".into())
    } else {
        Ok(value.to_owned())
    }
}

fn validate_requested_paths(
    requested: &[String],
    snapshot: &ProjectGitSnapshot,
    staged: bool,
) -> Result<Vec<String>, String> {
    if requested.len() > MAXIMUM_PATHS_PER_MUTATION {
        return Err(invalid_change());
    }
    let mut seen = HashSet::new();
    let mut result = Vec::with_capacity(requested.len());
    for value in requested {
        let path = normalized_relative_path(value)?;
        if !seen.insert(path.clone()) {
            return Err(invalid_change());
        }
        let change = snapshot
            .changes
            .iter()
            .find(|change| change.path == path)
            .ok_or_else(invalid_change)?;
        if staged && !change.has_staged_changes || !staged && !change.has_working_tree_changes {
            return Err(invalid_change());
        }
        result.push(path);
    }
    Ok(result)
}

fn parse_changes(bytes: Vec<u8>) -> Result<Vec<ProjectGitChange>, String> {
    let tokens = bytes
        .split(|byte| *byte == 0)
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let mut changes = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if token.len() < 4 || token[2] != b' ' {
            return Err(invalid_git_output());
        }
        let index_status = status_character(token[0])?;
        let work_tree_status = status_character(token[1])?;
        let path = parse_portable_path(&token[3..])?;
        let original_path = if index_status == "R" || index_status == "C" {
            index += 1;
            Some(parse_portable_path(
                tokens.get(index).ok_or_else(invalid_git_output)?,
            )?)
        } else {
            None
        };
        let has_staged_changes = index_status != " " && index_status != "?";
        let has_working_tree_changes =
            work_tree_status != " " || index_status == "?" && work_tree_status == "?";
        changes.push(ProjectGitChange {
            path,
            original_path,
            kind: change_kind(&index_status, &work_tree_status),
            index_status,
            work_tree_status,
            has_staged_changes,
            has_working_tree_changes,
        });
        index += 1;
    }
    changes.sort_by(|left, right| {
        let left_conflict = left.kind == ProjectGitChangeKind::Conflicted;
        let right_conflict = right.kind == ProjectGitChangeKind::Conflicted;
        right_conflict
            .cmp(&left_conflict)
            .then(left.path.cmp(&right.path))
    });
    Ok(changes)
}

fn parse_branches(bytes: Vec<u8>) -> Result<Vec<ProjectGitBranch>, String> {
    let value = output_text(bytes)?;
    value
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let fields = line.split('\0').collect::<Vec<_>>();
            if fields.len() != 3 || fields[0].is_empty() {
                return Err(invalid_git_output());
            }
            fields
                .iter()
                .try_for_each(|field| validate_safe_field(field, MAXIMUM_METADATA_FIELD_BYTES))?;
            Ok(ProjectGitBranch {
                name: fields[0].to_owned(),
                is_current: fields[1] == "*",
                upstream: (!fields[2].is_empty()).then(|| fields[2].to_owned()),
            })
        })
        .collect()
}

fn parse_commits(bytes: Vec<u8>) -> Result<Vec<ProjectGitCommit>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let fields = bytes.split(|byte| *byte == 0).collect::<Vec<_>>();
    if fields.len() % 7 != 0 {
        return Err(invalid_git_output());
    }
    fields
        .chunks_exact(7)
        .map(|fields| {
            let values = fields
                .iter()
                .map(|field| String::from_utf8(field.to_vec()).map_err(|_| invalid_git_output()))
                .collect::<Result<Vec<_>, _>>()?;
            values
                .iter()
                .try_for_each(|value| validate_safe_field(value, MAXIMUM_METADATA_FIELD_BYTES))?;
            if !valid_object_id(&values[0])
                || !valid_object_id(&values[1])
                || values[2]
                    .split_whitespace()
                    .any(|parent| !valid_object_id(parent))
            {
                return Err(invalid_git_output());
            }
            Ok(ProjectGitCommit {
                id: values[0].clone(),
                short_id: values[1].clone(),
                parent_ids: values[2].split_whitespace().map(str::to_owned).collect(),
                author: values[3].clone(),
                authored_at: (!values[4].is_empty()).then(|| values[4].clone()),
                decorations: values[5]
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .collect(),
                subject: values[6].clone(),
            })
        })
        .collect()
}

fn status_character(byte: u8) -> Result<String, String> {
    const ALLOWED: &[u8] = b" MADRCUT?!";
    if ALLOWED.contains(&byte) {
        Ok(char::from(byte).to_string())
    } else {
        Err(invalid_git_output())
    }
}

fn change_kind(index: &str, work_tree: &str) -> ProjectGitChangeKind {
    match format!("{index}{work_tree}").as_str() {
        "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU" => ProjectGitChangeKind::Conflicted,
        "??" => ProjectGitChangeKind::Untracked,
        _ if index == "D" || work_tree == "D" => ProjectGitChangeKind::Deleted,
        _ if index == "R" || work_tree == "R" => ProjectGitChangeKind::Renamed,
        _ if index == "C" || work_tree == "C" => ProjectGitChangeKind::Copied,
        _ if index == "A" || work_tree == "A" => ProjectGitChangeKind::Added,
        _ if index == "T" || work_tree == "T" => ProjectGitChangeKind::TypeChanged,
        _ => ProjectGitChangeKind::Modified,
    }
}

fn parse_portable_path(bytes: &[u8]) -> Result<String, String> {
    let value = String::from_utf8(bytes.to_vec()).map_err(|_| invalid_git_output())?;
    normalized_relative_path(&value).map_err(|_| invalid_git_output())
}

fn normalized_relative_path(value: &str) -> Result<String, String> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > MAXIMUM_PORTABLE_PATH_BYTES
        || value.trim() != value
        || value.contains('\\')
        || value.contains(':')
        || value.chars().any(char::is_control)
        || path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        Err(invalid_change())
    } else {
        Ok(value.to_owned())
    }
}

fn optional_safe_line(bytes: Vec<u8>, maximum: usize) -> Result<Option<String>, String> {
    let value = output_text(bytes)?;
    let value = value.trim_end_matches(['\r', '\n']);
    validate_safe_field(value, maximum)?;
    Ok((!value.is_empty()).then(|| value.to_owned()))
}

fn output_text(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| invalid_git_output())
}

fn validate_safe_field(value: &str, maximum: usize) -> Result<(), String> {
    if value.len() > maximum || value.chars().any(char::is_control) {
        Err(invalid_git_output())
    } else {
        Ok(())
    }
}

fn valid_object_id(value: &str) -> bool {
    (7..=64).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sanitize_remote_url(value: &str) -> (String, bool) {
    if value.is_empty() {
        return (String::new(), false);
    }
    if value.len() > MAXIMUM_REMOTE_URL_BYTES || value.chars().any(char::is_control) {
        return ("[远端地址已隐藏]".into(), true);
    }
    let without_suffix = value
        .split_once(['?', '#'])
        .map(|(prefix, _)| prefix)
        .unwrap_or(value);
    if let Some((scheme, remainder)) = without_suffix.split_once("://") {
        if scheme.eq_ignore_ascii_case("file") {
            return ("[本地远端地址已隐藏]".into(), true);
        }
        let authority_end = remainder.find('/').unwrap_or(remainder.len());
        let authority = &remainder[..authority_end];
        let host = authority
            .rsplit_once('@')
            .map(|(_, host)| host)
            .unwrap_or(authority);
        let sanitized = format!("{scheme}://{host}{}", &remainder[authority_end..]);
        let was_redacted = sanitized != value;
        return (sanitized, was_redacted);
    }
    if let Some((userinfo, location)) = without_suffix.split_once('@') {
        if let Some((user, _)) = userinfo.split_once(':') {
            if !user.is_empty() {
                return (format!("{user}@{location}"), true);
            }
            return (location.to_owned(), true);
        }
    }
    if Path::new(without_suffix).is_absolute()
        || without_suffix.starts_with('/')
        || without_suffix.starts_with('\\')
        || without_suffix
            .as_bytes()
            .get(1)
            .is_some_and(|value| *value == b':')
    {
        return ("[本地远端地址已隐藏]".into(), true);
    }
    (without_suffix.to_owned(), without_suffix != value)
}

async fn run_git(
    arguments: &[String],
    directory: &Path,
    allowed_exit_codes: &[i32],
    duration: Duration,
    maximum_output_bytes: usize,
    failure_message: &'static str,
) -> Result<GitProcessOutput, String> {
    let mut command = Command::new("git");
    command
        .args(arguments)
        .current_dir(directory)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "无法启动本机 Git 进程。".to_owned())?;
    let termination = ProcessTermination::attach(&child).map_err(|_| {
        let _ = child.start_kill();
        "无法隔离本机 Git 进程。".to_owned()
    })?;
    let stdout = child.stdout.take().ok_or_else(invalid_git_output)?;
    let stderr = child.stderr.take().ok_or_else(invalid_git_output)?;
    let byte_count = Arc::new(AtomicUsize::new(0));
    let exceeded = Arc::new(AtomicBool::new(false));
    let stdout_task = tokio::spawn(read_bounded_pipe(
        stdout,
        byte_count.clone(),
        exceeded.clone(),
        maximum_output_bytes,
        true,
    ));
    let stderr_task = tokio::spawn(read_bounded_pipe(
        stderr,
        byte_count,
        exceeded.clone(),
        maximum_output_bytes,
        false,
    ));

    let status = match timeout(duration, child.wait()).await {
        Ok(result) => result.map_err(|_| failure_message.to_owned())?,
        Err(_) => {
            let _ = termination.terminate(false);
            if timeout(TERMINATION_GRACE_PERIOD, child.wait())
                .await
                .is_err()
            {
                let _ = termination.terminate(true);
                let _ = timeout(TERMINATION_GRACE_PERIOD, child.wait()).await;
            }
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err("Git 操作超时，相关进程已终止。".into());
        }
    };
    // Hooks and credential helpers can outlive Git while retaining its pipes. Closing the
    // isolated tree here ensures output collection cannot wait forever after Git exits.
    let _ = termination.terminate(true);
    let stdout = stdout_task
        .await
        .map_err(|_| invalid_git_output())?
        .map_err(|_| invalid_git_output())?;
    stderr_task
        .await
        .map_err(|_| invalid_git_output())?
        .map_err(|_| invalid_git_output())?;
    if exceeded.load(Ordering::SeqCst) {
        return Err("Git 输出超过安全上限。".into());
    }
    let exit_code = status.code().unwrap_or(-1);
    if !allowed_exit_codes.contains(&exit_code) {
        return Err(failure_message.into());
    }
    Ok(GitProcessOutput { exit_code, stdout })
}

async fn read_bounded_pipe(
    mut reader: impl AsyncRead + Unpin,
    byte_count: Arc<AtomicUsize>,
    exceeded: Arc<AtomicBool>,
    maximum: usize,
    retain: bool,
) -> std::io::Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut buffer = [0_u8; 8 * 1_024];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let previous = byte_count.fetch_add(count, Ordering::SeqCst);
        if previous.saturating_add(count) > maximum {
            exceeded.store(true, Ordering::SeqCst);
        }
        if retain && previous < maximum {
            let retained = count.min(maximum - previous);
            result.extend_from_slice(&buffer[..retained]);
        }
    }
    Ok(result)
}

fn required_identifier(field: &str, value: String) -> Result<String, String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 1_024
        || value.chars().any(char::is_control)
    {
        Err(format!("项目 Git 字段无效：{field}。"))
    } else {
        Ok(value)
    }
}

#[cfg(windows)]
fn null_device_path() -> &'static str {
    "NUL"
}

#[cfg(not(windows))]
fn null_device_path() -> &'static str {
    "/dev/null"
}

fn project_unavailable() -> String {
    "当前账号无法访问这个项目。".into()
}

fn repository_outside_workspace() -> String {
    "Git 仓库超出了项目所属 Workspace 的授权边界。".into()
}

fn invalid_change() -> String {
    "所选 Git 变更已经失效，请刷新后重试。".into()
}

fn invalid_branch() -> String {
    "Git 分支名称无效。".into()
}

fn invalid_remote() -> String {
    "Git 远端配置无效。".into()
}

fn invalid_git_output() -> String {
    "Git 返回了无法安全解析的数据。".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::time::Instant;
    use std::{fs, process::Command as StdCommand};

    fn context(root: &Path) -> GitProjectContext {
        let root = fs::canonicalize(root).unwrap();
        GitProjectContext {
            project_id: "project-1".into(),
            project_root: root.clone(),
            workspace_root: root,
        }
    }

    async fn initialize(root: &Path) {
        run_git(
            &["init".into(), "-b".into(), "main".into()],
            root,
            &[0],
            DEFAULT_GIT_TIMEOUT,
            MAXIMUM_GIT_OUTPUT_BYTES,
            "init failed",
        )
        .await
        .unwrap();
        for arguments in [
            ["config", "user.name", "ChatOS Tests"],
            ["config", "user.email", "tests@chatos.local"],
        ] {
            let status = StdCommand::new("git")
                .args(arguments)
                .current_dir(root)
                .status()
                .unwrap();
            assert!(status.success());
        }
    }

    async fn mutate(root: &Path, arguments: &[&str]) {
        run_git(
            &arguments
                .iter()
                .map(|value| (*value).to_owned())
                .collect::<Vec<_>>(),
            root,
            &[0],
            DEFAULT_GIT_TIMEOUT,
            MAXIMUM_GIT_OUTPUT_BYTES,
            "mutation failed",
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn loads_changes_stages_commits_and_builds_diff() {
        let directory = tempfile::tempdir().unwrap();
        initialize(directory.path()).await;
        fs::write(directory.path().join("README.md"), "# First\n").unwrap();
        let project = context(directory.path());

        let mut snapshot = load_snapshot(&project).await.unwrap();
        assert!(snapshot.is_repository);
        assert_eq!(snapshot.current_branch.as_deref(), Some("main"));
        let change = snapshot
            .changes
            .iter()
            .find(|change| change.path == "README.md")
            .unwrap();
        assert_eq!(change.kind, ProjectGitChangeKind::Untracked);
        assert!(change.has_working_tree_changes);

        let current = required_snapshot(&project).await.unwrap();
        let paths =
            validate_requested_paths(&["README.md".into()], &current.snapshot, false).unwrap();
        let mut arguments = vec!["add".to_owned(), "--".to_owned()];
        arguments.extend(paths);
        run_git(
            &arguments,
            &current.repository.root,
            &[0],
            DEFAULT_GIT_TIMEOUT,
            MAXIMUM_GIT_OUTPUT_BYTES,
            "stage failed",
        )
        .await
        .unwrap();
        mutate(directory.path(), &["commit", "-m", "docs: add readme"]).await;

        snapshot = load_snapshot(&project).await.unwrap();
        assert!(snapshot.changes.is_empty());
        assert_eq!(snapshot.commits[0].subject, "docs: add readme");
        fs::write(
            directory.path().join("README.md"),
            "# First\n\nSecond line\n",
        )
        .unwrap();
        let current = required_snapshot(&project).await.unwrap();
        assert!(current.snapshot.changes[0].has_working_tree_changes);
        let output = run_git(
            &[
                "diff".into(),
                "--no-ext-diff".into(),
                "--no-color".into(),
                "--".into(),
                "README.md".into(),
            ],
            &current.repository.root,
            &[0],
            DEFAULT_GIT_TIMEOUT,
            MAXIMUM_DIFF_BYTES,
            "diff failed",
        )
        .await
        .unwrap();
        assert!(output_text(output.stdout).unwrap().contains("+Second line"));
    }

    #[tokio::test]
    async fn creates_switches_merges_and_manages_remotes() {
        let directory = tempfile::tempdir().unwrap();
        initialize(directory.path()).await;
        fs::write(directory.path().join("value.txt"), "main\n").unwrap();
        mutate(directory.path(), &["add", "--", "value.txt"]).await;
        mutate(directory.path(), &["commit", "-m", "initial"]).await;
        mutate(directory.path(), &["switch", "-c", "feature/git-workbench"]).await;
        fs::write(directory.path().join("value.txt"), "feature\n").unwrap();
        mutate(directory.path(), &["add", "--", "value.txt"]).await;
        mutate(directory.path(), &["commit", "-m", "feature change"]).await;
        mutate(directory.path(), &["switch", "main"]).await;
        mutate(
            directory.path(),
            &["merge", "--no-edit", "feature/git-workbench"],
        )
        .await;
        mutate(
            directory.path(),
            &[
                "remote",
                "add",
                "origin",
                "https://token:secret@example.com/team/project.git?access=secret#fragment",
            ],
        )
        .await;

        let snapshot = load_snapshot(&context(directory.path())).await.unwrap();
        assert_eq!(snapshot.current_branch.as_deref(), Some("main"));
        assert_eq!(snapshot.commits[0].subject, "feature change");
        assert_eq!(
            snapshot.remotes[0].url,
            "https://example.com/team/project.git"
        );
        assert!(snapshot.remotes[0].url_is_redacted);
        assert!(!serde_json::to_string(&snapshot).unwrap().contains("secret"));
        let serialized = serde_json::to_string(&snapshot).unwrap();
        assert!(!serialized.contains(directory.path().to_string_lossy().as_ref()));
        assert!(!serialized.contains("repositoryRoot"));
        assert!(!serialized.contains("absolutePath"));
    }

    #[tokio::test]
    async fn rejects_paths_not_present_in_the_fresh_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        initialize(directory.path()).await;
        fs::write(directory.path().join("present.txt"), "value").unwrap();
        let snapshot = load_snapshot(&context(directory.path())).await.unwrap();
        for path in ["../outside", "/absolute", "missing.txt", "dir\\file"] {
            assert!(validate_requested_paths(&[path.into()], &snapshot, false).is_err());
        }
    }

    #[tokio::test]
    async fn rejects_a_repository_above_the_workspace_boundary() {
        let directory = tempfile::tempdir().unwrap();
        initialize(directory.path()).await;
        let workspace = directory.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        let workspace = fs::canonicalize(workspace).unwrap();
        let project = GitProjectContext {
            project_id: "project-1".into(),
            project_root: workspace.clone(),
            workspace_root: workspace,
        };
        assert_eq!(
            repository_context(&project).await.unwrap_err(),
            repository_outside_workspace()
        );
    }

    #[tokio::test]
    async fn rejects_oversized_output_after_draining_the_process() {
        let directory = tempfile::tempdir().unwrap();
        initialize(directory.path()).await;
        fs::write(directory.path().join("large.txt"), vec![b'x'; 512 * 1_024]).unwrap();
        let result = run_git(
            &[
                "diff".into(),
                "--no-index".into(),
                "--no-color".into(),
                "--".into(),
                null_device_path().into(),
                directory
                    .path()
                    .join("large.txt")
                    .to_string_lossy()
                    .into_owned(),
            ],
            directory.path(),
            &[0, 1],
            DEFAULT_GIT_TIMEOUT,
            64 * 1_024,
            "diff failed",
        )
        .await;
        assert_eq!(result.err().as_deref(), Some("Git 输出超过安全上限。"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_terminates_the_git_process_group() {
        let directory = tempfile::tempdir().unwrap();
        let started = Instant::now();
        let result = run_git(
            &[
                "-c".into(),
                "alias.chatos-wait=!sleep 60".into(),
                "chatos-wait".into(),
            ],
            directory.path(),
            &[0],
            Duration::from_millis(50),
            MAXIMUM_GIT_OUTPUT_BYTES,
            "wait failed",
        )
        .await;
        assert_eq!(
            result.err().as_deref(),
            Some("Git 操作超时，相关进程已终止。")
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn redacts_remote_credentials_query_and_fragment() {
        assert_eq!(
            sanitize_remote_url("https://user:password@example.com/team/repo.git?token=x#part"),
            ("https://example.com/team/repo.git".into(), true)
        );
        assert_eq!(
            sanitize_remote_url("user:password@example.com:team/repo.git"),
            ("user@example.com:team/repo.git".into(), true)
        );
        assert_eq!(
            sanitize_remote_url("git@example.com:team/repo.git"),
            ("git@example.com:team/repo.git".into(), false)
        );
        assert_eq!(
            sanitize_remote_url("ssh://user:password@example.com/team/repo.git?token=x"),
            ("ssh://example.com/team/repo.git".into(), true)
        );
        assert_eq!(
            sanitize_remote_url("file:///Users/person/secret/repo.git"),
            ("[本地远端地址已隐藏]".into(), true)
        );
        assert_eq!(
            sanitize_remote_url("/Users/person/secret/repo.git"),
            ("[本地远端地址已隐藏]".into(), true)
        );
    }

    #[tokio::test]
    async fn committed_without_refresh_is_explicit_and_does_not_expose_paths() {
        let directory = tempfile::tempdir().unwrap();
        let invalid = GitProjectContext {
            project_id: "project-1".into(),
            project_root: directory.path().join("missing"),
            workspace_root: directory.path().to_owned(),
        };
        let result = refresh_after_mutation(&invalid).await.unwrap();
        assert!(result.snapshot.is_none());
        assert!(result.committed_without_refresh);
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains(directory.path().to_string_lossy().as_ref()));
        assert!(!serialized.contains("repositoryRoot"));
        assert!(!serialized.contains("absolutePath"));
    }
}
