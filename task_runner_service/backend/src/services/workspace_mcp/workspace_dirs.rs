// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::config::AppConfig;
use crate::models::{ModelConfigRecord, TaskRecord};
pub(in crate::services) fn ensure_effective_task_workspace_dir(
    config: &AppConfig,
    task: &TaskRecord,
    model_config: &ModelConfigRecord,
) -> Result<String, String> {
    let configured = task
        .mcp_config
        .workspace_dir
        .as_deref()
        .or(model_config.request_cwd.as_deref());
    if configured.is_some() {
        return ensure_workspace_dir_available(config.default_workspace_dir.as_str(), configured);
    }

    ensure_default_user_workspace_dir_available(
        config.default_workspace_dir.as_str(),
        task.subject_id.as_str(),
    )
}

pub(in crate::services) fn resolve_workspace_dir_with_base(
    base_dir: &str,
    configured: Option<&str>,
) -> String {
    let candidate = configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(base_dir);
    let path = PathBuf::from(candidate);
    let resolved = if path.is_absolute() {
        path
    } else {
        PathBuf::from(base_dir).join(path)
    };
    std::fs::canonicalize(&resolved)
        .unwrap_or(resolved)
        .to_string_lossy()
        .to_string()
}

pub(in crate::services) fn ensure_workspace_dir_available(
    base_dir: &str,
    configured: Option<&str>,
) -> Result<String, String> {
    let resolved = resolve_workspace_dir_with_base(base_dir, configured);
    ensure_workspace_is_inside_base(base_dir, resolved.as_str())?;
    let path = PathBuf::from(&resolved);

    match std::fs::metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(format!("工作目录不是目录: {}", path.display()));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(&path).map_err(|create_err| {
                format!(
                    "create workspace dir {} failed: {}",
                    path.display(),
                    create_err
                )
            })?;
        }
        Err(err) => {
            return Err(format!(
                "read workspace dir {} failed: {}",
                path.display(),
                err
            ));
        }
    }

    Ok(path
        .canonicalize()
        .unwrap_or(path)
        .to_string_lossy()
        .to_string())
}

fn ensure_default_user_workspace_dir_available(
    base_dir: &str,
    subject_id: &str,
) -> Result<String, String> {
    let user_component = user_workspace_component(subject_id);
    let relative = PathBuf::from("users")
        .join(user_component)
        .join("workspaces")
        .join("default");
    ensure_workspace_dir_available(base_dir, relative.to_str())
}

pub(in crate::services) fn default_user_workspace_dir(
    base_dir: &str,
    subject_id: Option<&str>,
) -> PathBuf {
    let subject_id = subject_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("task_runner");
    PathBuf::from(base_dir)
        .join("users")
        .join(user_workspace_component(subject_id))
        .join("workspaces")
        .join("default")
}

pub(super) fn ensure_workspace_is_inside_base(
    base_dir: &str,
    workspace_dir: &str,
) -> Result<(), String> {
    let base = canonical_or_absolute(Path::new(base_dir));
    let workspace = canonical_or_absolute(Path::new(workspace_dir));
    if path_is_within_root(workspace.as_path(), base.as_path()) {
        Ok(())
    } else {
        Err(format!(
            "workspace dir is outside task runner workspace base: {}",
            workspace.display()
        ))
    }
}

fn canonical_or_absolute(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    canonicalize_existing_prefix(&absolute)
}

fn canonicalize_existing_prefix(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    let mut missing = Vec::<OsString>::new();
    while !current.exists() {
        let Some(file_name) = current.file_name() else {
            break;
        };
        missing.push(file_name.to_os_string());
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }
    let mut resolved = std::fs::canonicalize(&current).unwrap_or(current);
    for component in missing.into_iter().rev() {
        resolved.push(component);
    }
    resolved
}

fn path_is_within_root(candidate: &Path, root: &Path) -> bool {
    let candidate = normalize_path_for_compare(candidate);
    let root = normalize_path_for_compare(root);
    candidate == root || candidate.starts_with(format!("{root}/").as_str())
}

fn user_workspace_component(value: &str) -> String {
    let normalized = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches(['.', '_', '-'])
        .chars()
        .take(80)
        .collect::<String>();
    let prefix = if normalized.is_empty() {
        "user".to_string()
    } else {
        normalized
    };
    format!("{prefix}-{:016x}", stable_hash64(value.trim().as_bytes()))
}

fn stable_hash64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn normalize_path_for_compare(path: &Path) -> String {
    let mut value = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        if let Some(stripped) = value.strip_prefix("//?/UNC/") {
            value = format!("//{stripped}");
        } else if let Some(stripped) = value.strip_prefix("//?/") {
            value = stripped.to_string();
        }
    }
    let (prefix, rest) = if value.len() >= 2 && value.as_bytes()[1] == b':' {
        (value[..2].to_string(), &value[2..])
    } else {
        (String::new(), value.as_str())
    };
    let absolute = rest.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in rest.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                let _ = segments.pop();
            }
            value => segments.push(value),
        }
    }
    let mut out = String::new();
    out.push_str(prefix.as_str());
    if absolute {
        out.push('/');
    }
    out.push_str(segments.join("/").as_str());
    while out.ends_with('/') && out.len() > 1 {
        out.pop();
    }
    if cfg!(windows) {
        out.make_ascii_lowercase();
    }
    out
}
