use std::collections::HashSet;
use std::fs;
use std::path::Path;

use uuid::Uuid;

pub fn normalize_name(value: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = false;
    for ch in value.trim().to_lowercase().chars() {
        let valid = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-';
        if valid {
            out.push(ch);
            prev_sep = false;
        } else if !prev_sep {
            out.push('_');
            prev_sep = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "sub_agent_router".to_string()
    } else {
        trimmed
    }
}

pub fn normalize_id(value: Option<&str>) -> String {
    value.unwrap_or("").trim().to_string()
}

pub fn generate_id(prefix: &str) -> String {
    let safe_prefix = normalize_name(prefix);
    format!("{safe_prefix}_{}", Uuid::new_v4())
}

pub fn ensure_dir(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    fs::create_dir_all(path).map_err(|err| err.to_string())
}

pub fn tokenize(text: Option<&str>) -> Vec<String> {
    let raw = text.unwrap_or("").trim().to_lowercase();
    if raw.is_empty() {
        return Vec::new();
    }
    raw.split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '|' || c == '/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

pub fn unique_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            out.push(value);
        }
    }
    out
}

pub fn safe_json_parse<T: serde::de::DeserializeOwned>(raw: &str, fallback: T) -> T {
    serde_json::from_str(raw).unwrap_or(fallback)
}
