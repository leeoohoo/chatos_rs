// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;

use axum::extract::{Query, State};
use axum::Json;
use serde_json::{json, Value};

use crate::LocalRuntime;

use super::super::error::LocalRuntimeApiError;
use super::shared::{
    git_command, git_error, git_text, resolve_repository, validate_paths, validate_ref,
    GitCompareQuery, GitDiffQuery,
};

pub(super) async fn compare(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<GitCompareQuery>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let repo = resolve_repository(&runtime, query.root.as_str()).await?;
    let target = validate_ref(query.target.as_str(), "target")?;
    let current = git_text(repo.as_path(), &["branch", "--show-current"])
        .await
        .unwrap_or_else(|_| "HEAD".to_string())
        .trim()
        .to_string();
    let current = if current.is_empty() {
        "HEAD".to_string()
    } else {
        current
    };
    let range = format!("{current}...{target}");
    let files = git_text(repo.as_path(), &["diff", "--name-status", range.as_str()])
        .await
        .map_err(git_error)?
        .lines()
        .filter_map(parse_changed_file)
        .collect::<Vec<_>>();
    let commits = git_text(
        repo.as_path(),
        &[
            "log",
            "--left-right",
            "--cherry-pick",
            "--format=%m%x1f%h%x1f%s",
            range.as_str(),
        ],
    )
    .await
    .map_err(git_error)?
    .lines()
    .filter_map(parse_commit)
    .collect::<Vec<_>>();
    Ok(Json(json!({
        "current": current, "target": target, "files": files, "commits": commits,
    })))
}

pub(super) async fn diff(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<GitDiffQuery>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let repo = resolve_repository(&runtime, query.root.as_str()).await?;
    let path = validate_paths(std::slice::from_ref(&query.path))?.remove(0);
    let staged = query.staged.unwrap_or(false);
    let mut args = vec!["diff".to_string()];
    if let Some(target) = query
        .target
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        args.push(format!("HEAD...{}", validate_ref(target, "target")?));
    } else if staged {
        args.push("--cached".to_string());
    }
    args.extend(["--".to_string(), path.clone()]);
    let mut patch = git_command(repo.as_path(), args, false)
        .await
        .map_err(git_error)?
        .stdout;
    if query.target.is_none()
        && !staged
        && patch.trim().is_empty()
        && !is_tracked(repo.as_path(), path.as_str()).await
    {
        patch = untracked_patch(repo.as_path(), path.as_str())?;
    }
    Ok(Json(json!({
        "path": path, "target": query.target, "staged": staged, "patch": patch,
    })))
}

async fn is_tracked(repo: &std::path::Path, path: &str) -> bool {
    git_text(repo, &["ls-files", "--error-unmatch", "--", path])
        .await
        .is_ok()
}

fn untracked_patch(repo: &std::path::Path, path: &str) -> Result<String, LocalRuntimeApiError> {
    let absolute = repo.join(path).canonicalize().map_err(git_error)?;
    if !absolute.starts_with(repo) || !absolute.is_file() {
        return Err(git_error("Untracked diff path is invalid"));
    }
    let bytes = fs::read(absolute).map_err(git_error)?;
    if bytes.len() > 256 * 1024 {
        return Ok(binary_patch(path));
    }
    let Ok(content) = String::from_utf8(bytes) else {
        return Ok(binary_patch(path));
    };
    let mut patch = format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n",
        content.lines().count().max(1)
    );
    for line in content.lines() {
        patch.push_str(format!("+{line}\n").as_str());
    }
    Ok(patch)
}

fn binary_patch(path: &str) -> String {
    format!("diff --git a/{path} b/{path}\nnew file mode 100644\nBinary file b/{path} differs\n")
}

fn parse_changed_file(line: &str) -> Option<Value> {
    let mut parts = line.split('\t');
    let status = parts.next()?.trim();
    let first = parts.next()?.trim();
    let second = parts.next().map(str::trim);
    Some(json!({
        "path": second.unwrap_or(first), "old_path": second.map(|_| first), "status": status,
    }))
}

fn parse_commit(line: &str) -> Option<Value> {
    let mut parts = line.splitn(3, '\u{1f}');
    Some(json!({
        "side": parts.next()?.trim(),
        "hash": parts.next()?.trim(),
        "subject": parts.next()?.trim(),
    }))
}
