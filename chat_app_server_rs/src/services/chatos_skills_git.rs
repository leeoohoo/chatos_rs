// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

use super::chatos_skills_helpers::{
    ensure_dir, has_parent_path_component, normalize_plugin_source, sanitize_repo_name,
};
use crate::utils::process_output::run_command_limited;

const SKILL_GIT_STDOUT_LIMIT_BYTES: usize = 1024 * 1024;
const SKILL_GIT_STDERR_LIMIT_BYTES: usize = 1024 * 1024;
const SKILL_GIT_TIMEOUT: Duration = Duration::from_secs(120);

pub async fn ensure_git_repo(
    repo_url: &str,
    branch: Option<&str>,
    cache_root: &Path,
) -> Result<PathBuf, String> {
    ensure_dir(cache_root)?;
    let safe_name = sanitize_repo_name(repo_url);
    let repo_path = cache_root.join(safe_name);

    if repo_path.exists() {
        let path = repo_path.clone();
        tokio::task::spawn_blocking(move || fs::remove_dir_all(path.as_path()))
            .await
            .map_err(|err| format!("remove old repo task failed: {err}"))?
            .map_err(|err| {
                format!(
                    "remove old repo failed ({}): {}",
                    repo_path.to_string_lossy(),
                    err
                )
            })?;
    }

    let mut args = vec!["clone".to_string(), "--depth".to_string(), "1".to_string()];
    if let Some(value) = branch {
        args.push("--branch".to_string());
        args.push(value.to_string());
    }
    args.push(repo_url.to_string());
    args.push(repo_path.to_string_lossy().to_string());
    run_git(args.as_slice()).await?;
    Ok(repo_path)
}

pub fn copy_plugin_source_from_repo(
    repo_root: &Path,
    plugins_root: &Path,
    source: &str,
) -> Result<String, String> {
    let normalized = normalize_plugin_source(source);
    if normalized.is_empty() {
        return Err("plugin source is empty".to_string());
    }
    if has_parent_path_component(normalized.as_str()) {
        return Err("plugin source cannot contain ..".to_string());
    }
    let src = repo_root.join(normalized.as_str());
    if !src.exists() {
        return Err(format!(
            "plugin source not found in repository: {}",
            normalized
        ));
    }
    let dest_rel = plugin_install_destination(normalized.as_str());
    if dest_rel.is_empty() {
        return Err("plugin source normalization failed".to_string());
    }
    let dest = plugins_root.join(dest_rel.as_str());
    copy_path(src.as_path(), dest.as_path())?;
    Ok(dest_rel)
}

async fn run_git(args: &[String]) -> Result<(), String> {
    let mut command = Command::new("git");
    command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_MERGE_AUTOEDIT", "no");
    let output = run_command_limited(
        command,
        SKILL_GIT_TIMEOUT,
        SKILL_GIT_STDOUT_LIMIT_BYTES,
        SKILL_GIT_STDERR_LIMIT_BYTES,
        "plugin git",
    )
    .await
    .map_err(|err| format!("git execution failed: {err}"))?;

    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(output.stderr.as_slice())
        .trim()
        .to_string();
    let stdout = String::from_utf8_lossy(output.stdout.as_slice())
        .trim()
        .to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    Err(format!(
        "git command failed (exit={}): {}",
        output.status.code().unwrap_or(-1),
        detail
    ))
}

fn copy_path(src: &Path, dest: &Path) -> Result<(), String> {
    if !src.exists() {
        return Err(format!("source not found: {}", src.to_string_lossy()));
    }
    if dest.exists() {
        if dest.is_dir() {
            fs::remove_dir_all(dest).map_err(|err| err.to_string())?;
        } else {
            fs::remove_file(dest).map_err(|err| err.to_string())?;
        }
    }
    if src.is_file() {
        if let Some(parent) = dest.parent() {
            ensure_dir(parent)?;
        }
        fs::copy(src, dest).map_err(|err| err.to_string())?;
        return Ok(());
    }

    ensure_dir(dest)?;
    for entry in fs::read_dir(src).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let next = dest.join(entry.file_name());
        let file_type = entry.file_type().map_err(|err| err.to_string())?;
        if file_type.is_dir() {
            copy_path(path.as_path(), next.as_path())?;
        } else if file_type.is_file() {
            if let Some(parent) = next.parent() {
                ensure_dir(parent)?;
            }
            fs::copy(path.as_path(), next.as_path()).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn plugin_install_destination(source: &str) -> String {
    let normalized = normalize_plugin_source(source);
    if let Some(stripped) = normalized.strip_prefix("plugins/") {
        stripped.trim_matches('/').to_string()
    } else {
        normalized
    }
}
