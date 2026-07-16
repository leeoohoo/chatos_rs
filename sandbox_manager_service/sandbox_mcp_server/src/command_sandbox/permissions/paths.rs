// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::config::*;
use super::super::*;

pub(in crate::command_sandbox) fn resolve_entry_paths(
    config: &CommandSandboxConfig,
    cwd: &Path,
    entry: &FileSystemSandboxEntry,
) -> Result<Vec<PathBuf>, String> {
    match &entry.path {
        FileSystemPath::Path { path } => Ok(vec![resolve_permission_path(config, cwd, path)?]),
        FileSystemPath::GlobPattern { .. } => {
            Err("glob entries must be expanded separately".to_string())
        }
        FileSystemPath::Special { value } => match value {
            FileSystemSpecialPath::Root => Ok(filesystem_root_paths(config, cwd)),
            FileSystemSpecialPath::Minimal => Ok(minimal_platform_paths()),
            FileSystemSpecialPath::ProjectRoots { subpath } => config
                .runtime_workspace_roots
                .iter()
                .map(|root| {
                    let path = match subpath {
                        Some(subpath) => join_beneath(root.as_path(), subpath)?,
                        None => root.clone(),
                    };
                    normalize_policy_path_preserving_symlinks(path.as_path())
                })
                .collect(),
            FileSystemSpecialPath::Tmpdir => Ok(vec![config.temp.clone()]),
            FileSystemSpecialPath::SlashTmp => Ok(vec![PathBuf::from("/tmp")]),
            FileSystemSpecialPath::Unknown { path, subpath } => {
                let base = resolve_permission_path(config, cwd, path)?;
                let path = match subpath {
                    Some(subpath) => join_beneath(base.as_path(), subpath)?,
                    None => base,
                };
                Ok(vec![normalize_policy_path_preserving_symlinks(
                    path.as_path(),
                )?])
            }
        },
    }
}

pub(in crate::command_sandbox) fn filesystem_root_paths(
    config: &CommandSandboxConfig,
    cwd: &Path,
) -> Vec<PathBuf> {
    let mut candidates = vec![
        cwd,
        config.workspace.as_path(),
        config.state_root.as_path(),
        config.temp.as_path(),
    ];
    candidates.extend(config.runtime_workspace_roots.iter().map(PathBuf::as_path));
    if let Some(host_home) = config.host_home.as_deref() {
        candidates.push(host_home);
    }
    chatos_sandbox_contract::filesystem_roots_for_paths(candidates)
}

pub(in crate::command_sandbox) fn resolve_permission_path(
    config: &CommandSandboxConfig,
    cwd: &Path,
    value: &str,
) -> Result<PathBuf, String> {
    let path = if value == "~" {
        config
            .host_home
            .clone()
            .ok_or_else(|| "host home directory is unavailable".to_string())?
    } else if let Some(relative) = value.strip_prefix("~/") {
        config
            .host_home
            .as_deref()
            .ok_or_else(|| "host home directory is unavailable".to_string())?
            .join(relative)
    } else {
        let path = Path::new(value);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        }
    };
    normalize_policy_path_preserving_symlinks(path.as_path())
}

pub(in crate::command_sandbox) fn join_beneath(
    root: &Path,
    subpath: &str,
) -> Result<PathBuf, String> {
    let subpath = Path::new(subpath);
    if subpath.is_absolute()
        || subpath
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(format!(
            "permission subpath must stay under {}",
            root.display()
        ));
    }
    Ok(root.join(subpath))
}

pub(in crate::command_sandbox) fn normalize_policy_path_preserving_symlinks(
    path: &Path,
) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!(
            "permission path must resolve to absolute: {}",
            path.display()
        ));
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(format!(
                        "permission path escapes the filesystem root: {}",
                        path.display()
                    ));
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    if !normalized.is_absolute() {
        return Err(format!(
            "permission path must resolve to absolute: {}",
            path.display()
        ));
    }
    Ok(normalized)
}

pub(in crate::command_sandbox) fn canonicalize_preserving_missing(
    path: &Path,
) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!(
            "permission path must resolve to absolute: {}",
            path.display()
        ));
    }
    if path.exists() {
        return path.canonicalize().map_err(|err| err.to_string());
    }
    let mut missing = Vec::new();
    let mut ancestor = path;
    while !ancestor.exists() {
        let name = ancestor
            .file_name()
            .ok_or_else(|| format!("cannot normalize permission path {}", path.display()))?;
        missing.push(name.to_os_string());
        ancestor = ancestor
            .parent()
            .ok_or_else(|| format!("cannot normalize permission path {}", path.display()))?;
    }
    let mut normalized = ancestor.canonicalize().map_err(|err| err.to_string())?;
    for part in missing.into_iter().rev() {
        normalized.push(part);
    }
    Ok(normalized)
}

pub(in crate::command_sandbox) fn expand_deny_glob(
    pattern: &str,
    cwd: &Path,
    max_depth: Option<usize>,
) -> Result<Vec<PathBuf>, String> {
    let absolute_pattern = if Path::new(pattern).is_absolute() {
        PathBuf::from(pattern)
    } else {
        cwd.join(pattern)
    };
    let pattern_text = absolute_pattern.to_string_lossy();
    let first_meta = pattern_text
        .char_indices()
        .find_map(|(index, ch)| matches!(ch, '*' | '?' | '[' | ']').then_some(index))
        .ok_or_else(|| "glob pattern does not contain a glob expression".to_string())?;
    let static_prefix = &pattern_text[..first_meta];
    let search_root = if static_prefix.ends_with('/') {
        PathBuf::from(static_prefix.trim_end_matches('/'))
    } else {
        Path::new(static_prefix)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| cwd.to_path_buf())
    };
    if search_root == Path::new("/") || !search_root.is_dir() {
        return Err(format!(
            "deny glob requires a bounded existing search root: {pattern}"
        ));
    }
    let relative_pattern = absolute_pattern
        .strip_prefix(search_root.as_path())
        .map_err(|_| format!("glob pattern is outside its search root: {pattern}"))?
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string();
    let search_root = normalize_policy_path_preserving_symlinks(search_root.as_path())?;
    let mut builder = GlobSetBuilder::new();
    builder.add(
        GlobBuilder::new(relative_pattern.as_str())
            .literal_separator(true)
            .build()
            .map_err(|err| format!("invalid deny glob {pattern:?}: {err}"))?,
    );
    let matcher = builder
        .build()
        .map_err(|err| format!("build deny glob matcher failed: {err}"))?;
    let mut walker = WalkDir::new(search_root.as_path()).follow_links(false);
    if let Some(max_depth) = max_depth {
        walker = walker.max_depth(max_depth);
    }
    let mut matches = Vec::new();
    for entry in walker.into_iter() {
        let entry = entry.map_err(|err| err.to_string())?;
        if entry.path() == search_root {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(search_root.as_path())
            .map_err(|err| err.to_string())?;
        if matcher.is_match(relative) {
            let logical = normalize_policy_path_preserving_symlinks(entry.path())?;
            if let Some(target) = canonical_target_if_symlinked_path(logical.as_path()) {
                matches.push(target);
            }
            matches.push(logical);
            if matches.len() > MAX_GLOB_MATCHES {
                return Err(format!(
                    "deny glob matched more than {MAX_GLOB_MATCHES} paths"
                ));
            }
        }
    }
    matches.sort();
    matches.dedup();
    Ok(matches)
}

pub(in crate::command_sandbox) fn minimal_platform_paths() -> Vec<PathBuf> {
    [
        "/bin", "/sbin", "/usr", "/etc", "/lib", "/lib64", "/System", "/Library",
    ]
    .into_iter()
    .map(PathBuf::from)
    .filter(|path| path.exists())
    .collect()
}

pub(in crate::command_sandbox) fn canonical_target_if_symlinked_path(
    path: &Path,
) -> Option<PathBuf> {
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => continue,
            Component::ParentDir => {
                current.pop();
                continue;
            }
            Component::Normal(part) => current.push(part),
        }

        let metadata = std::fs::symlink_metadata(current.as_path()).ok()?;
        if metadata.file_type().is_symlink() {
            let target = canonicalize_preserving_missing(path).ok()?;
            return (target != path).then_some(target);
        }
    }
    None
}

#[cfg(target_os = "linux")]
pub(in crate::command_sandbox) fn path_depth(path: &Path) -> usize {
    path.components().count()
}

pub(in crate::command_sandbox) fn canonical_existing_directory(
    path: &Path,
) -> Result<PathBuf, String> {
    let path = path
        .canonicalize()
        .map_err(|err| format!("canonicalize {} failed: {err}", path.display()))?;
    if !path.is_dir() {
        return Err(format!(
            "sandbox path is not a directory: {}",
            path.display()
        ));
    }
    Ok(path)
}

pub(in crate::command_sandbox) fn existing_directory_preserving_symlinks(
    path: &Path,
) -> Result<PathBuf, String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|err| format!("resolve current directory failed: {err}"))?
            .join(path)
    };
    let path = normalize_policy_path_preserving_symlinks(path.as_path())?;
    if !path.is_dir() {
        return Err(format!(
            "sandbox path is not a directory: {}",
            path.display()
        ));
    }
    Ok(path)
}

#[cfg(target_os = "linux")]
pub(in crate::command_sandbox) fn find_executable(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .map(Path::new)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(target_os = "linux")]
pub(in crate::command_sandbox) fn first_missing_component(path: &Path) -> Option<PathBuf> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if !current.exists() {
            return Some(current);
        }
    }
    None
}
