use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
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

pub fn normalize_id(value: Option<&String>) -> String {
    value.map(|v| v.trim().to_string()).unwrap_or_default()
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
        "code_maintainer".to_string()
    } else {
        trimmed
    }
}

pub fn generate_id(prefix: &str) -> String {
    let safe_prefix = normalize_name(prefix);
    format!("{safe_prefix}_{}", Uuid::new_v4())
}

pub fn clamp_number(value: Option<&String>, min: i64, max: i64, fallback: i64) -> i64 {
    let parsed = value
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(fallback);
    parsed.clamp(min, max)
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
    if let Ok(root) = env::var("MCP_STATE_ROOT") {
        if !root.trim().is_empty() {
            return PathBuf::from(root.trim()).join(normalize_name(server_name));
        }
    }
    get_home_dir().join(".mcp-servers").join(normalize_name(server_name))
}

pub fn ensure_path_inside_root(root: &Path, target: &Path) -> Result<PathBuf, String> {
    let resolved_root = normalize_path(root);
    let candidate = if target.is_absolute() {
        target.to_path_buf()
    } else {
        resolved_root.join(target)
    };
    let resolved = normalize_path(&candidate);
    if !resolved.starts_with(&resolved_root) {
        return Err(format!(
            "Path is outside workspace root: {}",
            target.display()
        ));
    }
    Ok(resolved)
}

pub fn is_binary_buffer(buffer: &[u8]) -> bool {
    let limit = buffer.len().min(8000);
    buffer.iter().take(limit).any(|b| *b == 0)
}

pub fn sha256_bytes(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}

pub fn format_bytes(bytes: i64) -> String {
    if bytes <= 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut idx = 0usize;
    while value >= 1024.0 && idx < units.len() - 1 {
        value /= 1024.0;
        idx += 1;
    }
    if value < 10.0 && idx > 0 {
        format!("{:.1} {}", value, units[idx])
    } else {
        format!("{:.0} {}", value, units[idx])
    }
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                components.push(component);
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(last) = components.pop() {
                    if matches!(last, Component::Prefix(_) | Component::RootDir) {
                        components.push(last);
                    }
                }
            }
            Component::Normal(_) => components.push(component),
        }
    }
    let mut normalized = PathBuf::new();
    for component in components {
        normalized.push(component.as_os_str());
    }
    normalized
}
