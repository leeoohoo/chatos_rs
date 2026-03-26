use std::collections::{HashMap, HashSet};
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
            let Some(file_path) =
                normalize_skill_entry_to_file(plugin_root.as_path(), entry.as_str())
            else {
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
            let id = hash_id(&[
                "skill",
                user_id.as_str(),
                plugin_source.as_str(),
                entry.as_str(),
            ]);
            let skill = MemorySkill {
                id,
                user_id: user_id.clone(),
                plugin_source: plugin_source.clone(),
                name: metadata_value(&metadata, &["name"])
                    .map(ToOwned::to_owned)
                    .or_else(|| first_markdown_heading(body).map(ToOwned::to_owned))
                    .unwrap_or_else(|| build_skill_name_from_entry(entry.as_str())),
                description: metadata_value(&metadata, &["description"]).map(ToOwned::to_owned),
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

    (HashMap::new(), raw)
}

fn parse_markdown_metadata(raw: &str) -> (HashMap<String, String>, &str) {
    let (mut metadata, body) = parse_markdown_frontmatter(raw);
    let table_metadata = parse_leading_markdown_meta_table(body);
    for (key, value) in table_metadata {
        metadata.entry(key).or_insert(value);
    }
    (metadata, body)
}

fn metadata_value<'a>(metadata: &'a HashMap<String, String>, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        let normalized_key = normalize_metadata_key(key);
        if let Some(value) = metadata
            .get(normalized_key.as_str())
            .map(String::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            return Some(value);
        }
    }
    None
}

fn parse_leading_markdown_meta_table(raw: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let lines = raw.lines().collect::<Vec<_>>();
    if lines.len() < 2 {
        return out;
    }

    let mut start = 0usize;
    while start < lines.len() && lines[start].trim().is_empty() {
        start += 1;
    }
    if start + 1 >= lines.len() {
        return out;
    }

    let header = lines[start].trim();
    let separator = lines[start + 1].trim();
    if !is_markdown_table_row(header) || !is_markdown_table_separator(separator) {
        return out;
    }

    for cells in std::iter::once(split_markdown_table_cells(header)).chain(
        lines
            .iter()
            .skip(start + 2)
            .map(|line| line.trim())
            .take_while(|line| is_markdown_table_row(line))
            .map(split_markdown_table_cells),
    ) {
        if cells.len() < 2 {
            continue;
        }
        let key = normalize_metadata_key(cells[0]);
        if !is_supported_metadata_key(key.as_str()) {
            continue;
        }
        let value = cells[1].trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() {
            continue;
        }
        out.entry(key).or_insert_with(|| value.to_string());
    }

    out
}

fn split_markdown_table_cells(line: &str) -> Vec<&str> {
    line.trim()
        .trim_start_matches('|')
        .trim_end_matches('|')
        .split('|')
        .map(str::trim)
        .collect::<Vec<_>>()
}

fn is_markdown_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.contains('|') {
        return false;
    }
    split_markdown_table_cells(trimmed)
        .iter()
        .any(|cell| !cell.trim().is_empty())
}

fn is_markdown_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.contains('|') {
        return false;
    }
    let cells = split_markdown_table_cells(trimmed);
    if cells.is_empty() {
        return false;
    }
    cells.iter().all(|cell| {
        let normalized = cell.trim().trim_matches(':').trim_matches('-').trim();
        !cell.trim().is_empty() && normalized.is_empty()
    })
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

fn normalize_metadata_key(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn is_supported_metadata_key(key: &str) -> bool {
    matches!(
        key,
        "name" | "description" | "version" | "argument-hint" | "argument_hint"
    )
}

#[cfg(test)]
mod tests {
    use super::{metadata_value, parse_markdown_metadata};

    #[test]
    fn parses_skill_description_from_leading_markdown_table() {
        let raw = r#"
| name | parallel-feature-development |
| --- | --- |
| description | Coordinate parallel feature development with file ownership strategies |
| version | 1.0.2 |

# Parallel Feature Development
"#;
        let (metadata, _body) = parse_markdown_metadata(raw.trim());
        assert_eq!(
            metadata_value(&metadata, &["name"]),
            Some("parallel-feature-development")
        );
        assert_eq!(
            metadata_value(&metadata, &["description"]),
            Some("Coordinate parallel feature development with file ownership strategies")
        );
    }
}
