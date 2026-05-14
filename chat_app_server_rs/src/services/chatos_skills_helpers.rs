use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::models::memory_skill::{MemorySkill, MemorySkillPlugin};
use crate::services::text_normalization::{normalize_optional_text_ref, normalize_string_vec};

pub use crate::services::text_normalization::resolve_visible_user_ids;

pub fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    normalize_optional_text_ref(value)
}

pub fn normalize_plugin_source(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    normalized.trim_matches('/').to_string()
}

pub fn sanitize_user_segment(value: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;
    for ch in value.trim().chars() {
        let normalized = if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            ch
        } else {
            '-'
        };
        if normalized == '-' {
            if last_dash {
                continue;
            }
            last_dash = true;
        } else {
            last_dash = false;
        }
        output.push(normalized);
    }
    let trimmed = output.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed
    }
}

pub fn resolve_skill_state_root(user_id: &str) -> PathBuf {
    let user_segment = sanitize_user_segment(user_id);
    if let Ok(raw) = std::env::var("MEMORY_SKILL_STATE_ROOT") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join(user_segment);
        }
    }

    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    home.join(".chatos")
        .join("memory_skill_center")
        .join(user_segment)
}

pub fn normalize_repo_relative_path(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    normalized.trim_matches('/').to_string()
}

pub fn unique_strings(values: Vec<String>) -> Vec<String> {
    normalize_string_vec(values)
}

pub fn merge_skills(target: &mut Vec<MemorySkill>, items: Vec<MemorySkill>) {
    let mut seen_ids = target
        .iter()
        .map(|item| item.id.clone())
        .collect::<std::collections::HashSet<_>>();
    for item in items {
        if seen_ids.insert(item.id.clone()) {
            target.push(item);
        }
    }
}

pub fn merge_plugins(target: &mut Vec<MemorySkillPlugin>, items: Vec<MemorySkillPlugin>) {
    let mut seen_sources = target
        .iter()
        .map(|item| item.source.clone())
        .collect::<std::collections::HashSet<_>>();
    for item in items {
        if seen_sources.insert(item.source.clone()) {
            target.push(item);
        }
    }
}

pub fn sort_skills_desc(items: &mut [MemorySkill]) {
    items.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.name.cmp(&right.name))
    });
}

pub fn sort_plugins_desc(items: &mut [MemorySkillPlugin]) {
    items.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.name.cmp(&right.name))
    });
}

pub fn paginate_items<T>(items: Vec<T>, limit: i64, offset: i64) -> Vec<T> {
    let offset = offset.max(0) as usize;
    let limit = limit.max(1).min(5000) as usize;
    items.into_iter().skip(offset).take(limit).collect()
}

pub fn hash_id(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    let digest = hasher.finalize();
    let mut out = String::new();
    for byte in digest {
        out.push_str(format!("{:02x}", byte).as_str());
    }
    out
}

pub fn path_to_unix_relative(base: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(base).ok()?;
    let rendered = rel.to_string_lossy().replace('\\', "/");
    let trimmed = rendered.trim_matches('/').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub fn has_parent_path_component(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
}

pub fn ensure_dir(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    fs::create_dir_all(path).map_err(|err| err.to_string())
}

pub fn sanitize_repo_name(value: &str) -> String {
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
