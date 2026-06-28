use std::fs;
use std::path::{Path, PathBuf};

use super::chatos_skills_file_limits::{
    read_plugin_text_limited, MAX_PLUGIN_MARKETPLACE_BYTES, MAX_PLUGIN_SCAN_ENTRIES,
};
use super::chatos_skills_helpers::{
    has_parent_path_component, normalize_plugin_source, normalize_repo_relative_path,
    path_to_unix_relative,
};
use super::chatos_skills_types::SkillPluginCandidate;

pub(crate) fn load_plugin_candidates_from_repo(
    repo_root: &Path,
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
        let raw = read_plugin_text_limited(file.as_path(), MAX_PLUGIN_MARKETPLACE_BYTES)?;
        let parsed = parse_marketplace_candidates(raw.as_str())?;
        if !parsed.is_empty() {
            return Ok(parsed);
        }
    } else if let Some(file) = find_default_file_recursively(repo_root, &["marketplace.json"]) {
        if let Ok(raw) = read_plugin_text_limited(file.as_path(), MAX_PLUGIN_MARKETPLACE_BYTES) {
            let parsed = parse_marketplace_candidates(raw.as_str())?;
            if !parsed.is_empty() {
                return Ok(parsed);
            }
        }
    }
    Ok(fallback_plugin_candidates(repo_root, plugins_path))
}

fn parse_marketplace_candidates(raw: &str) -> Result<Vec<SkillPluginCandidate>, String> {
    let value = serde_json::from_str::<serde_json::Value>(raw)
        .map_err(|err| format!("marketplace json parse failed: {}", err))?;
    let plugins = value
        .get("plugins")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut out = Vec::new();
    for item in plugins {
        let source = item
            .get("source")
            .and_then(serde_json::Value::as_str)
            .map(normalize_plugin_source)
            .unwrap_or_default();
        if source.is_empty() || has_parent_path_component(source.as_str()) {
            continue;
        }
        let name = item
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| source.clone());
        let category = item
            .get("category")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let description = item
            .get("description")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let version = item
            .get("version")
            .and_then(serde_json::Value::as_str)
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
    repo_root: &Path,
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
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if seen.insert(item.source.clone()) {
            out.push(item);
        }
    }
    out
}

fn find_default_file_recursively(root: &Path, names: &[&str]) -> Option<PathBuf> {
    let target_names = names
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    let mut stack = vec![root.to_path_buf()];
    let mut visited_entries = 0usize;
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(dir.as_path()).ok()?;
        for entry in entries.flatten() {
            visited_entries = visited_entries.saturating_add(1);
            if visited_entries > MAX_PLUGIN_SCAN_ENTRIES {
                return None;
            }
            let path = entry.path();
            let file_type = entry.file_type().ok()?;
            if file_type.is_dir() {
                if !matches!(
                    path.file_name().and_then(|value| value.to_str()),
                    Some(".git" | "node_modules" | "target" | ".next")
                ) {
                    stack.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let name = path.file_name()?.to_str()?.to_ascii_lowercase();
            if target_names.contains(name.as_str()) {
                return Some(path);
            }
        }
    }
    None
}
