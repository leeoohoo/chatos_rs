use std::collections::HashSet;
use std::time::SystemTime;

use chrono::{DateTime, Utc};

pub fn normalize_string(value: &str) -> String {
    value.trim().to_string()
}

pub fn normalize_title(value: &str) -> String {
    let out = normalize_string(value);
    if out.is_empty() {
        String::new()
    } else {
        out.chars().take(120).collect()
    }
}

pub fn normalize_tag(value: &str) -> String {
    normalize_string(value)
}

pub fn unique_tags(tags: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for tag in tags {
        let normalized = normalize_tag(tag);
        if normalized.is_empty() {
            continue;
        }
        let key = normalized.to_lowercase();
        if seen.insert(key) {
            out.push(normalized);
        }
    }
    out
}

fn is_valid_path_segment(segment: &str) -> bool {
    let s = segment.trim();
    if s.is_empty() || s == "." || s == ".." {
        return false;
    }
    if s.chars().any(|ch| {
        matches!(
            ch,
            '<' | '>' | ':' | '\"' | '/' | '\\' | '|' | '?' | '*' | '\0'
        )
    }) {
        return false;
    }
    !s.chars().any(|ch| (ch as u32) < 32)
}

pub fn normalize_folder_path(value: &str) -> Result<String, String> {
    let raw = normalize_string(value).replace('\\', "/");
    if raw.is_empty() {
        return Ok(String::new());
    }

    let cleaned = raw.trim_matches('/').to_string();
    if cleaned.is_empty() {
        return Ok(String::new());
    }

    let mut out = Vec::new();
    for part in cleaned.split('/').filter(|item| !item.trim().is_empty()) {
        if !is_valid_path_segment(part) {
            return Err(format!("Invalid folder segment: {part}"));
        }
        out.push(part.trim().to_string());
    }

    Ok(out.join("/"))
}

pub fn split_folder(folder: &str) -> Vec<String> {
    folder
        .trim()
        .replace('\\', "/")
        .split('/')
        .filter(|item| !item.trim().is_empty())
        .map(|item| item.trim().to_string())
        .collect()
}

pub fn extract_title_from_markdown(markdown: &str) -> String {
    let normalized = markdown.replace("\r\n", "\n");
    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            let heading = rest.trim_start_matches('#').trim();
            if !heading.is_empty() {
                return heading.chars().take(120).collect();
            }
        }
        return trimmed.chars().take(120).collect();
    }
    String::new()
}

pub fn now_iso() -> String {
    crate::core::time::now_rfc3339()
}

pub fn ts_to_rfc3339(ts: SystemTime) -> String {
    let datetime: DateTime<Utc> = DateTime::<Utc>::from(ts);
    datetime.to_rfc3339()
}
