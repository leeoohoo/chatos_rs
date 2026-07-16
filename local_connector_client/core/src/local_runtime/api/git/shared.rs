// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};

use crate::LocalRuntime;

use super::super::error::LocalRuntimeApiError;
use super::super::workspace_path::resolve_local_workspace_path;
pub(super) use super::contracts::*;

pub(super) struct GitCommandOutput {
    pub(super) success: bool,
    pub(super) stdout: String,
    pub(super) stderr: String,
}

pub(super) async fn resolve_repository(
    runtime: &LocalRuntime,
    root: &str,
) -> Result<PathBuf, LocalRuntimeApiError> {
    let resolved = resolve_local_workspace_path(runtime, root, false).await?;
    let raw = git_text(resolved.path.as_path(), &["rev-parse", "--show-toplevel"])
        .await
        .map_err(git_error)?;
    let repo_root = PathBuf::from(raw.trim())
        .canonicalize()
        .map_err(git_error)?;
    if !repo_root.starts_with(resolved.workspace.absolute_root.as_path()) {
        return Err(git_error("Git repository is outside the local workspace"));
    }
    Ok(repo_root)
}

pub(super) async fn git_text(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let args = args.iter().map(|value| value.to_string()).collect();
    git_command(cwd, args, false)
        .await
        .map(|output| output.stdout)
}

pub(super) async fn git_command(
    cwd: &Path,
    args: Vec<String>,
    allow_failure: bool,
) -> Result<GitCommandOutput, String> {
    let output = tokio::process::Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .await
        .map_err(|error| error.to_string())?;
    let result = GitCommandOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(output.stdout.as_slice()).to_string(),
        stderr: String::from_utf8_lossy(output.stderr.as_slice()).to_string(),
    };
    if !result.success && !allow_failure {
        return Err(result.stderr.trim().to_string());
    }
    Ok(result)
}

pub(super) fn validate_ref(value: &str, label: &str) -> Result<String, LocalRuntimeApiError> {
    let value = value.trim();
    if value.is_empty()
        || value.starts_with('-')
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(git_error(format!("Invalid Git {label}")));
    }
    Ok(value.to_string())
}

pub(super) fn validate_paths(paths: &[String]) -> Result<Vec<String>, LocalRuntimeApiError> {
    let mut valid = Vec::new();
    for raw in paths {
        let value = raw.trim().replace('\\', "/");
        let path = Path::new(value.as_str());
        if value.is_empty()
            || path.is_absolute()
            || path.components().any(|part| {
                matches!(
                    part,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(git_error("Git paths must be repository-relative"));
        }
        valid.push(value);
    }
    if valid.is_empty() {
        return Err(git_error("At least one Git path is required"));
    }
    Ok(valid)
}

pub(super) fn git_error(error: impl std::fmt::Display) -> LocalRuntimeApiError {
    LocalRuntimeApiError::bad_request("local_runtime_git_failed", error.to_string())
}

pub(super) fn change_counts(status: &str) -> Value {
    let mut staged = 0usize;
    let mut unstaged = 0usize;
    let mut untracked = 0usize;
    let mut conflicted = 0usize;
    for line in status.lines() {
        let bytes = line.as_bytes();
        if line.starts_with("??") {
            untracked += 1;
            continue;
        }
        staged += usize::from(bytes.first().is_some_and(|value| *value != b' '));
        unstaged += usize::from(bytes.get(1).is_some_and(|value| *value != b' '));
        conflicted += usize::from(matches!(&line[..2], "UU" | "AA" | "DD"));
    }
    json!({
        "staged": staged, "unstaged": unstaged,
        "untracked": untracked, "conflicted": conflicted,
    })
}

pub(super) async fn ahead_behind(root: &Path) -> (usize, usize) {
    let Ok(value) = git_text(
        root,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    )
    .await
    else {
        return (0, 0);
    };
    let mut values = value
        .split_whitespace()
        .filter_map(|value| value.parse().ok());
    (values.next().unwrap_or(0), values.next().unwrap_or(0))
}

pub(super) fn branch_value(name: &str, current: bool) -> Value {
    json!({
        "name": name, "short_name": name, "current": current,
        "upstream": Value::Null, "remote": Value::Null, "tracked_by": Value::Null,
        "ahead": 0, "behind": 0, "last_commit": Value::Null,
        "last_commit_subject": Value::Null,
    })
}

pub(super) fn status_file(line: &str) -> Option<Value> {
    if line.len() < 3 {
        return None;
    }
    let status = &line[..2];
    let path = line[3..].split(" -> ").last().unwrap_or("");
    Some(json!({
        "path": path, "old_path": Value::Null, "status": status.trim(),
        "staged": status.as_bytes().first().is_some_and(|value| *value != b' ' && *value != b'?'),
        "unstaged": status.as_bytes().get(1).is_some_and(|value| *value != b' '),
        "conflicted": matches!(status, "UU" | "AA" | "DD"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_git_paths_that_escape_the_local_repository() {
        assert!(validate_paths(&["src/main.rs".to_string()]).is_ok());
        assert!(validate_paths(&["../secret".to_string()]).is_err());
        assert!(validate_paths(&["/etc/passwd".to_string()]).is_err());
    }

    #[test]
    fn parses_local_git_status_without_cloud_services() {
        let file = status_file(" M src/main.rs").expect("status file");
        assert_eq!(file["path"], "src/main.rs");
        assert_eq!(file["unstaged"], true);
    }
}
