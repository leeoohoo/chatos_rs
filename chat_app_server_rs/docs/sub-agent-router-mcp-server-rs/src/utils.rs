use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct ParsedArgs {
    pub positional: Vec<String>,
    pub values: HashMap<String, String>,
    pub flags: HashSet<String>,
}

pub fn parse_args(argv: &[String]) -> ParsedArgs {
    let mut result = ParsedArgs::default();
    let mut i = 0;
    while i < argv.len() {
        let token = &argv[i];
        if !token.starts_with('-') {
            result.positional.push(token.to_string());
            i += 1;
            continue;
        }
        let key = token.trim_start_matches('-');
        if key.is_empty() {
            i += 1;
            continue;
        }
        if let Some((name, inline)) = key.split_once('=') {
            result.values.insert(name.to_string(), inline.to_string());
            i += 1;
            continue;
        }
        let next = argv.get(i + 1);
        if let Some(next_val) = next {
            if !next_val.starts_with('-') {
                result
                    .values
                    .insert(key.to_string(), next_val.to_string());
                i += 2;
                continue;
            }
        }
        result.flags.insert(key.to_string());
        i += 1;
    }
    result
}

pub fn normalize_name(value: &str) -> String {
    let mut out = String::new();
    let mut prev_underscore = false;
    for ch in value.trim().to_lowercase().chars() {
        let valid = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-';
        if valid {
            out.push(ch);
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "subagent_router".to_string()
    } else {
        trimmed
    }
}

pub fn normalize_id(value: Option<&String>) -> String {
    value.map(|v| v.trim().to_string()).unwrap_or_default()
}

pub fn generate_id(prefix: &str) -> String {
    let safe_prefix = normalize_name(prefix);
    format!("{safe_prefix}_{}", Uuid::new_v4())
}

pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    fs::create_dir_all(path)
}

pub fn get_home_dir() -> PathBuf {
    if let Ok(value) = env::var("HOME") {
        if !value.trim().is_empty() {
            return PathBuf::from(value);
        }
    }
    if let Ok(value) = env::var("USERPROFILE") {
        if !value.trim().is_empty() {
            return PathBuf::from(value);
        }
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

pub fn resolve_state_dir(server_name: &str) -> PathBuf {
    if let Ok(override_root) = env::var("SUBAGENT_STATE_ROOT") {
        if !override_root.trim().is_empty() {
            return PathBuf::from(override_root.trim()).join(normalize_name(server_name));
        }
    }
    if let Ok(root) = env::var("MCP_STATE_ROOT") {
        if !root.trim().is_empty() {
            return PathBuf::from(root.trim()).join(normalize_name(server_name));
        }
    }
    let home = get_home_dir();
    let legacy = home.join(".mcp_servers");
    let modern = home.join(".mcp-servers");
    let base = if legacy.exists() { legacy } else { modern };
    base.join(normalize_name(server_name))
}

pub fn safe_json_parse<T: serde::de::DeserializeOwned>(raw: &str, fallback: T) -> T {
    serde_json::from_str(raw).unwrap_or(fallback)
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

pub fn parse_command(input: Option<&str>) -> Option<Vec<String>> {
    let text = input.unwrap_or("").trim();
    if text.is_empty() {
        return None;
    }
    if text.starts_with('[') {
        if let Ok(parsed) = serde_json::from_str::<Vec<String>>(text) {
            return Some(parsed);
        }
    }
    Some(split_command(text))
}

fn split_command(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;
    for ch in text.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
            continue;
        }
        if ch == '\'' && !in_double {
            in_single = !in_single;
            continue;
        }
        if ch == '"' && !in_single {
            in_double = !in_double;
            continue;
        }
        if !in_single && !in_double && ch.is_whitespace() {
            if !current.is_empty() {
                args.push(current.clone());
                current.clear();
            }
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}
