use std::collections::HashMap;
use std::fs;
use std::path::{Path as FsPath, PathBuf};

use serde_json::Value;

use crate::models::MemorySkillPluginCommand;

use super::io_common::run_blocking_result;
use super::io_helpers::{is_skipped_repo_dir, path_to_unix_relative};
use super::io_types::SkillPluginExtractedContent;

pub async fn extract_plugin_content_async(
    plugin_root: PathBuf,
) -> Result<SkillPluginExtractedContent, String> {
    run_blocking_result(move || Ok(extract_plugin_content(plugin_root.as_path()))).await
}

fn extract_plugin_content(plugin_root: &FsPath) -> SkillPluginExtractedContent {
    let mut extracted = SkillPluginExtractedContent::default();

    let plugin_json = plugin_root.join(".claude-plugin").join("plugin.json");
    if plugin_json.exists() && plugin_json.is_file() {
        if let Ok(raw) = fs::read_to_string(plugin_json.as_path()) {
            if let Ok(value) = serde_json::from_str::<Value>(raw.as_str()) {
                extracted.name = value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(ToOwned::to_owned);
                extracted.description = value
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(ToOwned::to_owned);
                extracted.version = value
                    .get("version")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(ToOwned::to_owned);
            }
        }
    }

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
            let (frontmatter, body) = parse_markdown_frontmatter(trimmed_raw);
            if extracted.name.is_none() {
                extracted.name = frontmatter
                    .get("name")
                    .map(String::as_str)
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(ToOwned::to_owned);
            }
            if extracted.description.is_none() {
                extracted.description = frontmatter
                    .get("description")
                    .map(String::as_str)
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(ToOwned::to_owned);
            }

            let name = frontmatter
                .get("name")
                .map(String::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
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
            if let Some(description) = frontmatter
                .get("description")
                .map(String::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
            {
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
            let (frontmatter, body) = parse_markdown_frontmatter(trimmed_raw);
            let name = frontmatter
                .get("name")
                .map(String::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
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

fn collect_markdown_files(root: &FsPath) -> Vec<PathBuf> {
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
        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if key.is_empty() || value.is_empty() {
            continue;
        }
        out.insert(key.to_string(), value.to_string());
    }

    (HashMap::new(), raw)
}

fn first_markdown_heading(raw: &str) -> Option<&str> {
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('#') {
            let heading = rest.trim_start_matches('#').trim();
            if !heading.is_empty() {
                return Some(heading);
            }
        }
    }
    None
}
