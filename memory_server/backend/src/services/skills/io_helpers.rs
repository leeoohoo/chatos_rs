use std::path::Path as FsPath;

use sha2::{Digest, Sha256};

pub(super) fn normalize_repo_relative_path(value: &str) -> String {
    let mut normalized = value.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    normalized.trim_matches('/').to_string()
}

pub(super) fn path_to_unix_relative(base: &FsPath, path: &FsPath) -> Option<String> {
    let rel = path.strip_prefix(base).ok()?;
    let rendered = rel.to_string_lossy().replace('\\', "/");
    let trimmed = rendered.trim_matches('/').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub(super) fn contains_path_component(path: &FsPath, target: &str) -> bool {
    path.components().any(|comp| {
        comp.as_os_str()
            .to_str()
            .map(|name| name.eq_ignore_ascii_case(target))
            .unwrap_or(false)
    })
}

pub(super) fn has_parent_path_component(path: &str) -> bool {
    FsPath::new(path)
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
}

pub(super) fn is_skipped_repo_dir(path: &FsPath) -> bool {
    let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
        return false;
    };

    matches!(name, ".git" | "node_modules" | "target" | ".next")
}

pub(super) fn sanitize_user_segment(value: &str) -> String {
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

pub(super) fn entries_len_to_i64(entries: &[String]) -> i64 {
    entries.len().min(i64::MAX as usize) as i64
}

pub(super) fn hash_id(parts: &[&str]) -> String {
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
