use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::time::now_rfc3339;
use crate::models::memory_skill::{MemorySkill, MemorySkillPluginCommand};

use super::chatos_skills_helpers::{
    hash_id, normalize_repo_relative_path, path_to_unix_relative,
};

#[derive(Default)]
pub struct ExtractedPluginContent {
    pub content: Option<String>,
    pub commands: Vec<MemorySkillPluginCommand>,
}

pub fn discover_plugin_roots(plugins_root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut stack = vec![plugins_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(dir.as_path()) {
            Ok(entries) => entries,
            Err(err) => return Err(err.to_string()),
        };
        let mut children = Vec::new();
        let mut qualifies = false;
        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !file_type.is_dir() {
                continue;
            }
            let name = path.file_name().and_then(|value| value.to_str()).unwrap_or_default();
            if name.eq_ignore_ascii_case(".claude-plugin")
                || name.eq_ignore_ascii_case("skills")
                || name.eq_ignore_ascii_case("agents")
                || name.eq_ignore_ascii_case("commands")
            {
                qualifies = true;
            }
            if !is_skipped_repo_dir(path.as_path()) {
                children.push(path);
            }
        }
        if qualifies && dir != plugins_root {
            out.push(dir);
            continue;
        }
        stack.extend(children);
    }
    out.sort();
    Ok(out)
}

pub fn extract_plugin_content(plugin_root: &Path) -> ExtractedPluginContent {
    let mut extracted = ExtractedPluginContent::default();

    let agents_root = plugin_root.join("agents");
    let mut agent_sections = Vec::new();
    if agents_root.exists() && agents_root.is_dir() {
        let mut agent_files = collect_markdown_files(agents_root.as_path());
        agent_files.sort();
        for path in agent_files {
            let raw = match fs::read_to_string(path.as_path()) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let trimmed_raw = raw.trim();
            if trimmed_raw.is_empty() {
                continue;
            }
            let rel = path_to_unix_relative(plugin_root, path.as_path())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            let (metadata, body) = parse_markdown_metadata(trimmed_raw);
            let name = metadata_value(&metadata, &["name"])
                .map(ToOwned::to_owned)
                .or_else(|| first_markdown_heading(body).map(ToOwned::to_owned))
                .or_else(|| {
                    path.file_stem()
                        .and_then(|value| value.to_str())
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_else(|| rel.clone());
            let mut section = vec![format!("### {} ({})", name, rel)];
            if let Some(description) = metadata_value(&metadata, &["description"]) {
                section.push(format!("简介：{}", description));
            }
            let normalized_body = body.trim();
            if !normalized_body.is_empty() {
                section.push(normalized_body.to_string());
            } else {
                section.push(trimmed_raw.to_string());
            }
            agent_sections.push(section.join("\n"));
        }
    }
    if !agent_sections.is_empty() {
        extracted.content = Some(agent_sections.join("\n\n---\n\n"));
    }

    let commands_root = plugin_root.join("commands");
    if commands_root.exists() && commands_root.is_dir() {
        let mut command_files = collect_markdown_files(commands_root.as_path());
        command_files.sort();
        for path in command_files {
            let raw = match fs::read_to_string(path.as_path()) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let trimmed_raw = raw.trim();
            if trimmed_raw.is_empty() {
                continue;
            }
            let rel = path_to_unix_relative(plugin_root, path.as_path())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            let (metadata, body) = parse_markdown_metadata(trimmed_raw);
            let name = metadata_value(&metadata, &["name"])
                .map(ToOwned::to_owned)
                .or_else(|| first_markdown_heading(body).map(ToOwned::to_owned))
                .or_else(|| {
                    path.file_stem()
                        .and_then(|value| value.to_str())
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_else(|| rel.clone());
            let content = body.trim();
            extracted.commands.push(MemorySkillPluginCommand {
                name,
                source_path: rel,
                description: metadata_value(&metadata, &["description"]).map(ToOwned::to_owned),
                argument_hint: metadata_value(&metadata, &["argument-hint", "argument_hint"])
                    .map(ToOwned::to_owned),
                content: if content.is_empty() {
                    trimmed_raw.to_string()
                } else {
                    content.to_string()
                },
            });
        }
    }

    extracted
}

pub fn build_skills_from_plugin(
    plugin_root: &Path,
    user_id: &str,
    plugin_source: &str,
    plugin_version: Option<String>,
) -> Result<Vec<MemorySkill>, String> {
    let entries = discover_skill_entries(plugin_root);
    let mut skills = Vec::new();
    for entry in entries {
        let Some(file_path) = normalize_skill_entry_to_file(plugin_root, entry.as_str()) else {
            continue;
        };
        let raw = match fs::read_to_string(file_path.as_path()) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let trimmed_raw = raw.trim();
        if trimmed_raw.is_empty() {
            continue;
        }
        let (metadata, body) = parse_markdown_metadata(trimmed_raw);
        let normalized_body = body.trim();
        let content = if normalized_body.is_empty() {
            trimmed_raw.to_string()
        } else {
            normalized_body.to_string()
        };
        let id = hash_id(&["skill", user_id, plugin_source, entry.as_str()]);
        skills.push(MemorySkill {
            id,
            user_id: user_id.to_string(),
            plugin_source: plugin_source.to_string(),
            name: metadata_value(&metadata, &["name"])
                .map(ToOwned::to_owned)
                .or_else(|| first_markdown_heading(body).map(ToOwned::to_owned))
                .unwrap_or_else(|| build_skill_name_from_entry(entry.as_str())),
            description: metadata_value(&metadata, &["description"]).map(ToOwned::to_owned),
            content,
            source_path: entry,
            version: plugin_version.clone(),
            updated_at: now_rfc3339(),
        });
    }
    Ok(skills)
}

pub fn discover_skill_entries(plugin_root: &Path) -> Vec<String> {
    let root = plugin_root.join("skills");
    if !root.exists() || !root.is_dir() {
        return Vec::new();
    }

    let mut seen = std::collections::HashSet::new();
    for path in collect_markdown_entries(root.as_path()) {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");

        if file_name.eq_ignore_ascii_case("README.md") {
            continue;
        }
        if file_name.eq_ignore_ascii_case("SKILL.md") || file_name.eq_ignore_ascii_case("index.md")
        {
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

pub fn read_plugin_name(plugin_root: &Path) -> Option<String> {
    read_plugin_json_value(plugin_root, "name")
}

pub fn read_plugin_description(plugin_root: &Path) -> Option<String> {
    read_plugin_json_value(plugin_root, "description")
}

pub fn read_plugin_version(plugin_root: &Path) -> Option<String> {
    read_plugin_json_value(plugin_root, "version")
}

fn collect_markdown_entries(root: &Path) -> Vec<PathBuf> {
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

fn collect_markdown_files(root: &Path) -> Vec<PathBuf> {
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

fn contains_path_component(path: &Path, target: &str) -> bool {
    path.components().any(|comp| {
        comp.as_os_str()
            .to_str()
            .map(|name| name.eq_ignore_ascii_case(target))
            .unwrap_or(false)
    })
}

fn normalize_skill_entry_to_file(plugin_root: &Path, entry: &str) -> Option<PathBuf> {
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

fn is_skipped_repo_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
        return false;
    };
    matches!(name, ".git" | "node_modules" | "target" | ".next")
}

fn read_plugin_json_value(plugin_root: &Path, key: &str) -> Option<String> {
    let plugin_json = plugin_root.join(".claude-plugin").join("plugin.json");
    let raw = fs::read_to_string(plugin_json.as_path()).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(raw.as_str()).ok()?;
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_markdown_metadata(raw: &str) -> (HashMap<String, String>, &str) {
    parse_markdown_frontmatter(raw)
}

fn parse_markdown_frontmatter(raw: &str) -> (HashMap<String, String>, &str) {
    let mut out = HashMap::new();
    if !raw.starts_with("---\n") && !raw.starts_with("---\r\n") {
        return (out, raw);
    }
    let mut lines = raw.lines();
    let first = lines.next().unwrap_or_default();
    if first.trim() != "---" {
        return (out, raw);
    }

    let mut consumed = first.len();
    if raw.as_bytes().get(consumed) == Some(&b'\r') {
        consumed += 1;
    }
    if raw.as_bytes().get(consumed) == Some(&b'\n') {
        consumed += 1;
    }

    for line in lines {
        consumed += line.len();
        if raw.as_bytes().get(consumed) == Some(&b'\r') {
            consumed += 1;
        }
        if raw.as_bytes().get(consumed) == Some(&b'\n') {
            consumed += 1;
        }
        if line.trim() == "---" {
            let body = raw.get(consumed..).unwrap_or_default();
            return (out, body);
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = normalize_metadata_key(key);
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if key.is_empty() || value.is_empty() {
            continue;
        }
        out.insert(key, value.to_string());
    }
    (out, raw)
}

fn normalize_metadata_key(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('_', "-")
}

fn metadata_value<'a>(metadata: &'a HashMap<String, String>, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        let normalized = normalize_metadata_key(key);
        if let Some(value) = metadata.get(normalized.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

fn first_markdown_heading(body: &str) -> Option<&str> {
    body.lines()
        .map(str::trim)
        .find(|line| line.starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim())
        .filter(|line| !line.is_empty())
}
