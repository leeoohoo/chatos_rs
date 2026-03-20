use std::collections::HashSet;
use std::fs;
use std::path::{Path as FsPath, PathBuf};

use crate::models::MemorySkill;

use super::io_common::run_blocking_result;
use super::io_helpers::{
    contains_path_component, entries_len_to_i64, hash_id, is_skipped_repo_dir,
    normalize_repo_relative_path, path_to_unix_relative,
};

pub async fn discover_skill_entries_async(plugin_root: PathBuf) -> Result<Vec<String>, String> {
    run_blocking_result(move || Ok(discover_skill_entries(plugin_root.as_path()))).await
}

pub async fn build_skills_from_plugin_async(
    plugin_root: PathBuf,
    user_id: String,
    plugin_source: String,
    plugin_version: Option<String>,
) -> Result<(Vec<MemorySkill>, i64), String> {
    run_blocking_result(move || {
        let entries = discover_skill_entries(plugin_root.as_path());
        let discoverable_count = entries_len_to_i64(&entries);
        if discoverable_count <= 0 {
            return Ok((Vec::new(), 0));
        }

        let mut skills = Vec::new();
        for entry in entries.iter() {
            let Some(file_path) = normalize_skill_entry_to_file(plugin_root.as_path(), entry.as_str())
            else {
                continue;
            };
            let raw = match fs::read_to_string(file_path.as_path()) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let content = raw.trim().to_string();
            if content.is_empty() {
                continue;
            }
            let id = hash_id(&["skill", user_id.as_str(), plugin_source.as_str(), entry.as_str()]);
            let skill = MemorySkill {
                id,
                user_id: user_id.clone(),
                plugin_source: plugin_source.clone(),
                name: build_skill_name_from_entry(entry.as_str()),
                description: None,
                content,
                source_path: entry.clone(),
                version: plugin_version.clone(),
                updated_at: crate::repositories::now_rfc3339(),
            };
            skills.push(skill);
        }

        Ok((skills, discoverable_count))
    })
    .await
}

fn discover_skill_entries(plugin_root: &FsPath) -> Vec<String> {
    let root = plugin_root.join("skills");
    if !root.exists() || !root.is_dir() {
        return Vec::new();
    }

    let mut seen = HashSet::new();
    for path in collect_markdown_entries(root.as_path()) {
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");

        if file_name.eq_ignore_ascii_case("README.md") {
            continue;
        }

        if file_name.eq_ignore_ascii_case("SKILL.md") || file_name.eq_ignore_ascii_case("index.md") {
            let parent = path.parent().unwrap_or_else(|| root.as_path());
            if let Some(rel) = path_to_unix_relative(plugin_root, parent) {
                if !rel.trim().is_empty() {
                    seen.insert(rel);
                }
            }
            continue;
        }

        if contains_path_component(path.as_path(), "references") {
            continue;
        }

        if let Some(rel) = path_to_unix_relative(plugin_root, path.as_path()) {
            seen.insert(rel);
        }
    }

    let mut items = seen.into_iter().collect::<Vec<_>>();
    items.sort();
    items
}

fn collect_markdown_entries(root: &FsPath) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.exists() || !root.is_dir() {
        return out;
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(dir.as_path()) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(_) => continue,
            };

            if file_type.is_dir() {
                if !is_skipped_repo_dir(path.as_path()) {
                    stack.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let is_markdown = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("md"))
                .unwrap_or(false);
            if is_markdown {
                out.push(path);
            }
        }
    }

    out
}

fn normalize_skill_entry_to_file(plugin_root: &FsPath, entry: &str) -> Option<PathBuf> {
    let normalized = normalize_repo_relative_path(entry);
    if normalized.is_empty() {
        return None;
    }
    let path = plugin_root.join(normalized.as_str());
    if path.is_file() {
        return Some(path);
    }
    if path.is_dir() {
        let skill_md = path.join("SKILL.md");
        if skill_md.exists() && skill_md.is_file() {
            return Some(skill_md);
        }
        let index_md = path.join("index.md");
        if index_md.exists() && index_md.is_file() {
            return Some(index_md);
        }
    }
    None
}

fn build_skill_name_from_entry(entry: &str) -> String {
    let normalized = normalize_repo_relative_path(entry);
    if normalized.is_empty() {
        return "Skill".to_string();
    }

    let parts = normalized
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return "Skill".to_string();
    }

    let last = parts.last().copied().unwrap_or("");
    if last.eq_ignore_ascii_case("SKILL.md") || last.eq_ignore_ascii_case("index.md") {
        return parts
            .iter()
            .rev()
            .nth(1)
            .map(|value| (*value).to_string())
            .unwrap_or_else(|| "Skill".to_string());
    }
    if let Some(stem) = last.strip_suffix(".md") {
        return stem.to_string();
    }
    last.to_string()
}
