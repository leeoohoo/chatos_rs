use std::collections::HashSet;
use std::fs;
use std::path::{Path as FsPath, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::io_common::{ensure_dir, normalize_plugin_source, run_blocking_result};
use super::io_helpers::{
    has_parent_path_component, is_skipped_repo_dir, normalize_repo_relative_path,
    path_to_unix_relative,
};
use super::io_types::SkillPluginCandidate;

pub async fn ensure_git_repo_async(
    repo_url: String,
    branch: Option<String>,
    cache_root: PathBuf,
) -> Result<PathBuf, String> {
    run_blocking_result(move || {
        ensure_git_repo(repo_url.as_str(), branch.as_deref(), cache_root.as_path())
    })
    .await
}

pub async fn load_plugin_candidates_from_repo_async(
    repo_root: PathBuf,
    marketplace_path: Option<String>,
    plugins_path: Option<String>,
) -> Result<Vec<SkillPluginCandidate>, String> {
    run_blocking_result(move || {
        load_plugin_candidates_from_repo(
            repo_root.as_path(),
            marketplace_path.as_deref(),
            plugins_path.as_deref(),
        )
    })
    .await
}

fn ensure_git_repo(
    repo_url: &str,
    branch: Option<&str>,
    cache_root: &FsPath,
) -> Result<PathBuf, String> {
    ensure_dir(cache_root)?;
    let safe_name = sanitize_repo_name(repo_url);
    let repo_path = cache_root.join(safe_name);

    if repo_path.exists() {
        fs::remove_dir_all(repo_path.as_path()).map_err(|err| {
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
    run_git(args.as_slice())?;
    Ok(repo_path)
}

fn run_git(args: &[String]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|err| format!("git execution failed: {}", err))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    Err(format!(
        "git command failed (exit={}): {}",
        output.status.code().unwrap_or(-1),
        detail
    ))
}

fn sanitize_repo_name(value: &str) -> String {
    let mut raw = value.trim().to_string();
    if let Some(stripped) = raw.strip_prefix("https://") {
        raw = stripped.to_string();
    } else if let Some(stripped) = raw.strip_prefix("http://") {
        raw = stripped.to_string();
    }
    if let Some(stripped) = raw.strip_prefix("git@") {
        raw = stripped.to_string();
    }

    raw = raw.replace([':', '/'], "-");
    if raw.ends_with(".git") {
        raw.truncate(raw.len().saturating_sub(4));
    }

    let mut cleaned = String::new();
    let mut last_dash = false;
    for ch in raw.chars() {
        let valid = ch.is_ascii_alphanumeric() || ch == '_' || ch == '-';
        if valid {
            cleaned.push(ch);
            last_dash = false;
        } else if !last_dash {
            cleaned.push('-');
            last_dash = true;
        }
    }

    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "repo".to_string()
    } else {
        trimmed
    }
}

fn load_plugin_candidates_from_repo(
    repo_root: &FsPath,
    marketplace_path: Option<&str>,
    plugins_path: Option<&str>,
) -> Result<Vec<SkillPluginCandidate>, String> {
    if let Some(path) = marketplace_path
        .map(normalize_repo_relative_path)
        .filter(|value| !value.is_empty())
    {
        let file = repo_root.join(path.as_str());
        if !file.exists() || !file.is_file() {
            return Err(format!(
                "marketplace path not found: {}",
                file.to_string_lossy()
            ));
        }
        let raw = fs::read_to_string(file.as_path()).map_err(|err| err.to_string())?;
        let parsed = parse_marketplace_candidates(raw.as_str())?;
        if !parsed.is_empty() {
            return Ok(parsed);
        }
    } else if let Some(file) = find_default_file_recursively(repo_root, &["marketplace.json"]) {
        if let Ok(raw) = fs::read_to_string(file.as_path()) {
            let parsed = parse_marketplace_candidates(raw.as_str())?;
            if !parsed.is_empty() {
                return Ok(parsed);
            }
        }
    }

    Ok(fallback_plugin_candidates(repo_root, plugins_path))
}

fn parse_marketplace_candidates(raw: &str) -> Result<Vec<SkillPluginCandidate>, String> {
    let value = serde_json::from_str::<Value>(raw)
        .map_err(|err| format!("marketplace json parse failed: {}", err))?;
    let plugins = value
        .get("plugins")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut out = Vec::new();
    for item in plugins {
        let source = item
            .get("source")
            .and_then(Value::as_str)
            .map(normalize_plugin_source)
            .unwrap_or_default();
        if source.is_empty() || has_parent_path_component(source.as_str()) {
            continue;
        }
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| source.clone());
        let category = item
            .get("category")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let description = item
            .get("description")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let version = item
            .get("version")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        out.push(SkillPluginCandidate {
            source,
            name,
            category,
            description,
            version,
        });
    }

    Ok(unique_plugin_candidates(out))
}

fn fallback_plugin_candidates(
    repo_root: &FsPath,
    plugins_path: Option<&str>,
) -> Vec<SkillPluginCandidate> {
    let root = plugins_path
        .map(normalize_repo_relative_path)
        .filter(|value| !value.is_empty())
        .map(|value| repo_root.join(value))
        .unwrap_or_else(|| repo_root.join("plugins"));
    if !root.exists() || !root.is_dir() {
        return Vec::new();
    }

    let entries = match fs::read_dir(root.as_path()) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let rel = path_to_unix_relative(repo_root, path.as_path());
        let Some(rel) = rel else {
            continue;
        };
        let source = normalize_plugin_source(rel.as_str());
        if source.is_empty() || has_parent_path_component(source.as_str()) {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| source.clone());
        out.push(SkillPluginCandidate {
            source,
            name,
            category: None,
            description: None,
            version: None,
        });
    }

    unique_plugin_candidates(out)
}

fn unique_plugin_candidates(items: Vec<SkillPluginCandidate>) -> Vec<SkillPluginCandidate> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        if seen.insert(item.source.clone()) {
            out.push(item);
        }
    }
    out
}

fn find_default_file_recursively(root: &FsPath, names: &[&str]) -> Option<PathBuf> {
    let mut candidate_names = HashSet::new();
    for name in names {
        candidate_names.insert((*name).to_ascii_lowercase());
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(dir.as_path()) {
            Ok(value) => value,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let file_name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if candidate_names.contains(file_name.as_str()) {
                    return Some(path);
                }
                continue;
            }
            if path.is_dir() && !is_skipped_repo_dir(path.as_path()) {
                stack.push(path);
            }
        }
    }
    None
}
