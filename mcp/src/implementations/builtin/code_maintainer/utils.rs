// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::Utc;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

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
    get_home_dir()
        .join(".mcp-servers")
        .join(normalize_name(server_name))
}

pub fn ensure_path_inside_root(root: &Path, target: &Path) -> Result<PathBuf, String> {
    let resolved_root = root
        .canonicalize()
        .map_err(|err| format!("Resolve workspace root failed: {err}"))?;
    let candidate = if target.is_absolute() {
        target.to_path_buf()
    } else {
        root.join(target)
    };
    let lexical_candidate = normalize_path(&candidate);
    if !lexical_candidate.starts_with(normalize_path(root)) {
        return Err(format!(
            "Path is outside workspace root: {}",
            target.display()
        ));
    }
    let resolved = canonicalize_preserving_missing(lexical_candidate.as_path())?;
    if !resolved.starts_with(&resolved_root) {
        return Err(format!(
            "Path is outside workspace root: {}",
            target.display()
        ));
    }
    Ok(resolved)
}

fn canonicalize_preserving_missing(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return path
            .canonicalize()
            .map_err(|err| format!("Resolve path failed: {err}"));
    }
    let mut missing = Vec::new();
    let mut ancestor = path;
    while !ancestor.exists() {
        let name = ancestor.file_name().ok_or_else(|| {
            format!(
                "Resolve path failed: {} has no existing ancestor",
                path.display()
            )
        })?;
        missing.push(name.to_os_string());
        ancestor = ancestor
            .parent()
            .ok_or_else(|| format!("Resolve path failed: {} has no parent", path.display()))?;
    }
    let mut resolved = ancestor
        .canonicalize()
        .map_err(|err| format!("Resolve path failed: {err}"))?;
    for name in missing.into_iter().rev() {
        resolved.push(name);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_child_is_resolved_under_canonical_workspace() {
        let root =
            std::env::temp_dir().join(format!("chatos-code-root-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("root");
        let resolved = ensure_path_inside_root(root.as_path(), Path::new("new/child.txt"))
            .expect("resolve child");
        assert!(resolved.starts_with(root.canonicalize().expect("canonical root")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_to_outside_workspace_is_rejected() {
        use std::os::unix::fs::symlink;

        let root =
            std::env::temp_dir().join(format!("chatos-code-symlink-test-{}", uuid::Uuid::new_v4()));
        let workspace = root.join("workspace");
        let outside = root.join("outside");
        std::fs::create_dir_all(&workspace).expect("workspace");
        std::fs::create_dir_all(&outside).expect("outside");
        symlink(&outside, workspace.join("escape")).expect("symlink");

        let err = ensure_path_inside_root(workspace.as_path(), Path::new("escape/secret.txt"))
            .expect_err("symlink escape must fail");
        assert!(err.contains("outside workspace"));
        let _ = std::fs::remove_dir_all(root);
    }
}
