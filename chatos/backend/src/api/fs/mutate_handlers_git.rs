// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::services::code_nav::symbol_index::invalidate_project_symbol_indexes_for_path;
use crate::services::git;
use crate::services::project_fs_cache::invalidate_directory_listing_cache_for_path;
use crate::services::workspace_realtime_watcher::{
    note_workspace_path_changed, suppress_logged_path,
};

use super::super::contracts::{
    FsAppendGitignoreRequest, FsDiscardGitChangesRequest, FsOpenPathRequest,
};
use super::super::policy::FsPathPolicy;
use super::policy_error_tuple;

fn normalize_relative_for_gitignore(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn gitignore_parent_for_mode(
    authorized: &crate::api::fs::policy::AuthorizedPath,
    mode: &str,
) -> PathBuf {
    match mode {
        "folder" => {
            if authorized.path.is_dir() {
                authorized.path.clone()
            } else {
                authorized
                    .path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| authorized.navigation_root.clone())
            }
        }
        _ => authorized
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| authorized.navigation_root.clone()),
    }
}

fn gitignore_pattern(
    gitignore_dir: &Path,
    target_path: &Path,
    mode: &str,
) -> Result<String, String> {
    target_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "路径名称不合法".to_string())?;

    match mode {
        "file" => {
            let relative = pathdiff::diff_paths(target_path, gitignore_dir)
                .ok_or_else(|| "无法生成 .gitignore 相对路径".to_string())?;
            Ok(normalize_relative_for_gitignore(relative.as_path()))
        }
        "folder" => {
            let relative = pathdiff::diff_paths(target_path, gitignore_dir)
                .ok_or_else(|| "无法生成 .gitignore 相对路径".to_string())?;
            let normalized = normalize_relative_for_gitignore(relative.as_path());
            Ok(format!("{}/", normalized.trim_end_matches('/')))
        }
        "extension" => {
            let ext = target_path
                .extension()
                .and_then(|value| value.to_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "当前文件没有扩展名，无法忽略同扩展名文件".to_string())?;
            Ok(format!("*.{}", ext))
        }
        _ => Err("不支持的 .gitignore 模式".to_string()),
    }
    .and_then(|pattern| {
        if pattern.is_empty() {
            Err("生成的 .gitignore 规则为空".to_string())
        } else {
            Ok(pattern)
        }
    })
}

fn append_unique_gitignore_line(gitignore_path: &Path, pattern: &str) -> Result<bool, String> {
    let normalized_pattern = pattern.trim();
    if normalized_pattern.is_empty() {
        return Err("忽略规则不能为空".to_string());
    }

    let existing = fs::read_to_string(gitignore_path).unwrap_or_default();
    if existing
        .lines()
        .map(str::trim)
        .any(|line| line == normalized_pattern)
    {
        return Ok(false);
    }

    let mut next_content = existing;
    if !next_content.is_empty() && !next_content.ends_with('\n') {
        next_content.push('\n');
    }
    next_content.push_str(normalized_pattern);
    next_content.push('\n');
    fs::write(gitignore_path, next_content)
        .map_err(|err| format!("写入 .gitignore 失败: {}", err))?;
    Ok(true)
}

fn run_open_command(args: &[&str]) -> Result<(), String> {
    let status = Command::new("open")
        .args(args)
        .status()
        .map_err(|err| format!("调用 open 失败: {}", err))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("open 命令执行失败: {}", status))
    }
}

pub(in super::super) async fn append_gitignore_entry(
    auth: AuthUser,
    Json(req): Json<FsAppendGitignoreRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    let raw = req
        .path
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(raw) = raw else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "路径不能为空" })),
        );
    };
    if super::super::harness_project_bridge::is_harness_project_path(raw.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "云端项目请直接编辑 Harness 仓库中的 .gitignore" })),
        );
    }
    let mode = req
        .mode
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("file");

    let authorized = match policy.authorize_existing_entry(raw.as_str(), "路径不存在", "路径不合法")
    {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    let gitignore_dir = gitignore_parent_for_mode(&authorized, mode);
    let gitignore_authorized = match policy.authorize_existing_dir(
        gitignore_dir.to_string_lossy().as_ref(),
        "目标目录不存在",
        "目标路径不是目录",
    ) {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    if let Err(err) = policy.require_write(&gitignore_authorized) {
        return policy_error_tuple(err);
    }

    let repo_root = match git::discover_repo_root(gitignore_authorized.path.as_path()).await {
        Ok(Some(root)) => root,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "当前路径不在 Git 仓库内，无法写入 .gitignore" })),
            );
        }
        Err(message) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))),
    };
    if !authorized.path.starts_with(repo_root.as_path()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "目标路径不在 Git 仓库内" })),
        );
    }

    let pattern = match gitignore_pattern(
        gitignore_authorized.path.as_path(),
        authorized.path.as_path(),
        mode,
    ) {
        Ok(value) => value,
        Err(message) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))),
    };
    let gitignore_path = gitignore_authorized.path.join(".gitignore");
    let created = !gitignore_path.exists();
    let appended = match append_unique_gitignore_line(gitignore_path.as_path(), pattern.as_str()) {
        Ok(value) => value,
        Err(message) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": message })),
            );
        }
    };

    invalidate_project_symbol_indexes_for_path(gitignore_path.as_path());
    if let Some(project_root) = gitignore_authorized.project_root.as_ref() {
        let _ = invalidate_directory_listing_cache_for_path(
            project_root.to_string_lossy().as_ref(),
            gitignore_path.as_path(),
        );
    }
    let gitignore_path_text = gitignore_path.to_string_lossy().to_string();
    let gitignore_display_path = policy.display_path(gitignore_path.as_path());
    suppress_logged_path(gitignore_path_text.as_str());
    note_workspace_path_changed(gitignore_path_text.as_str());

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "path": gitignore_display_path,
            "display_path": gitignore_display_path,
            "pattern": pattern,
            "created": created,
            "appended": appended,
        })),
    )
}

pub(in super::super) async fn open_path_externally(
    auth: AuthUser,
    Json(req): Json<FsOpenPathRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    let raw = req
        .path
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(raw) = raw else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "路径不能为空" })),
        );
    };
    if super::super::harness_project_bridge::is_harness_project_path(raw.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "云端项目文件不能在本机程序中打开" })),
        );
    }
    let mode = req
        .mode
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("default");

    let authorized = match policy.authorize_existing_entry(raw.as_str(), "路径不存在", "路径不合法")
    {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };

    let open_result = match mode {
        "reveal" => run_open_command(&["-R", authorized.path.to_string_lossy().as_ref()]),
        "code" => run_open_command(&[
            "-a",
            "Visual Studio Code",
            authorized.path.to_string_lossy().as_ref(),
        ]),
        "default" => run_open_command(&[authorized.path.to_string_lossy().as_ref()]),
        _ => Err("不支持的打开方式".to_string()),
    };
    if let Err(message) = open_result {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": message })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "path": policy.display_path(authorized.path.as_path()),
            "display_path": policy.display_path(authorized.path.as_path()),
            "mode": mode,
        })),
    )
}

pub(in super::super) async fn discard_git_changes(
    auth: AuthUser,
    Json(req): Json<FsDiscardGitChangesRequest>,
) -> (StatusCode, Json<Value>) {
    let policy = match FsPathPolicy::for_user(&auth).await {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    let raw = req
        .path
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(raw) = raw else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "路径不能为空" })),
        );
    };
    if super::super::harness_project_bridge::is_harness_project_path(raw.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "云端项目不支持从本地 Git 工作区回滚" })),
        );
    }

    let authorized = match policy.authorize_existing_entry(raw.as_str(), "路径不存在", "路径不合法")
    {
        Ok(value) => value,
        Err(err) => return policy_error_tuple(err),
    };
    if let Err(err) = policy.require_write(&authorized) {
        return policy_error_tuple(err);
    }
    if let Err(err) = policy.forbid_root_mutation(authorized.path.as_path()) {
        return policy_error_tuple(err);
    }

    let repo_root = match git::discover_repo_root(authorized.path.as_path()).await {
        Ok(Some(root)) => root,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "当前路径不在 Git 仓库内，无法回滚变更" })),
            );
        }
        Err(message) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))),
    };
    let relative_path = match pathdiff::diff_paths(authorized.path.as_path(), repo_root.as_path()) {
        Some(value) => normalize_relative_for_gitignore(value.as_path()),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "无法解析仓库相对路径" })),
            );
        }
    };

    let result = match git::discard(git::GitPathRequest {
        root: repo_root.to_string_lossy().to_string(),
        paths: vec![relative_path.clone()],
    })
    .await
    {
        Ok(value) => value,
        Err(message) => return (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))),
    };

    invalidate_project_symbol_indexes_for_path(authorized.path.as_path());
    if let Some(project_root) = authorized.project_root.as_ref() {
        let _ = invalidate_directory_listing_cache_for_path(
            project_root.to_string_lossy().as_ref(),
            authorized.path.as_path(),
        );
    }
    let target_path = authorized.path.to_string_lossy().to_string();
    let display_target_path = policy.display_path(authorized.path.as_path());
    suppress_logged_path(target_path.as_str());
    note_workspace_path_changed(target_path.as_str());

    (
        StatusCode::OK,
        Json(json!({
            "success": result.success,
            "path": display_target_path,
            "display_path": display_target_path,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "summary": result.summary,
        })),
    )
}
