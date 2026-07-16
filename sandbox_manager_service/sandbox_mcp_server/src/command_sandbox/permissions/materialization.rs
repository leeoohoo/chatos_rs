// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::config::*;
use super::super::*;
use super::paths::*;

#[derive(Debug)]
pub(in crate::command_sandbox) struct MaterializedPermissions {
    pub(in crate::command_sandbox) unrestricted: bool,
    #[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
    pub(in crate::command_sandbox) full_disk_read: bool,
    #[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
    pub(in crate::command_sandbox) include_platform_defaults: bool,
    pub(in crate::command_sandbox) entries: Vec<MaterializedEntry>,
}

impl MaterializedPermissions {
    #[cfg(target_os = "linux")]
    pub(in crate::command_sandbox) fn access_for_path(&self, path: &Path) -> FileSystemAccessMode {
        self.entries
            .iter()
            .filter(|entry| path == entry.path || path.starts_with(entry.path.as_path()))
            .max_by_key(|entry| path_depth_all_platforms(entry.path.as_path()))
            .map(|entry| entry.access)
            .unwrap_or(if self.unrestricted {
                FileSystemAccessMode::Write
            } else {
                FileSystemAccessMode::Deny
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::command_sandbox) struct MaterializedEntry {
    pub(in crate::command_sandbox) access: FileSystemAccessMode,
    pub(in crate::command_sandbox) path: PathBuf,
}

pub(in crate::command_sandbox) fn materialize_permissions(
    config: &CommandSandboxConfig,
    cwd: &Path,
    granted: Option<&GrantedPermissionProfile>,
) -> Result<MaterializedPermissions, String> {
    let mut entries = BTreeMap::new();
    let mut include_platform_defaults = false;
    let unrestricted = matches!(
        config.base_file_system,
        FileSystemPermissionPolicy::Unrestricted
    );
    if let FileSystemPermissionPolicy::Restricted {
        entries: base_entries,
        glob_scan_max_depth,
    } = &config.base_file_system
    {
        materialize_file_system_entries(
            config,
            cwd,
            base_entries.as_slice(),
            *glob_scan_max_depth,
            &mut entries,
            &mut include_platform_defaults,
            false,
        )?;
    }
    if let Some(file_system) = granted.and_then(|grant| grant.file_system.as_ref()) {
        materialize_file_system_entries(
            config,
            cwd,
            file_system.normalized_entries().as_slice(),
            file_system.glob_scan_max_depth,
            &mut entries,
            &mut include_platform_defaults,
            true,
        )?;
    }
    let full_disk_read = unrestricted
        || filesystem_root_paths(config, cwd).iter().all(|root| {
            entries
                .get(root)
                .is_some_and(|access| *access != FileSystemAccessMode::Deny)
        });
    Ok(MaterializedPermissions {
        unrestricted,
        full_disk_read,
        include_platform_defaults,
        entries: entries
            .into_iter()
            .map(|(path, access)| MaterializedEntry { access, path })
            .collect(),
    })
}

pub(in crate::command_sandbox) fn materialize_file_system_entries(
    config: &CommandSandboxConfig,
    cwd: &Path,
    source: &[FileSystemSandboxEntry],
    glob_scan_max_depth: Option<usize>,
    entries: &mut BTreeMap<PathBuf, FileSystemAccessMode>,
    include_platform_defaults: &mut bool,
    command_overlay: bool,
) -> Result<(), String> {
    for entry in source {
        if matches!(
            entry.path,
            FileSystemPath::Special {
                value: FileSystemSpecialPath::Minimal
            }
        ) {
            *include_platform_defaults = entry.access != FileSystemAccessMode::Deny;
        }
        let paths = match &entry.path {
            FileSystemPath::GlobPattern { pattern } => {
                if command_overlay || Path::new(pattern).is_absolute() {
                    expand_deny_glob(pattern, cwd, glob_scan_max_depth)?
                } else if let Some(relative) = pattern
                    .strip_prefix("~/")
                    .or_else(|| pattern.strip_prefix("~\\"))
                {
                    let home = config
                        .host_home
                        .as_deref()
                        .ok_or_else(|| "host home directory is unavailable".to_string())?;
                    expand_deny_glob(
                        home.join(relative).to_string_lossy().as_ref(),
                        cwd,
                        glob_scan_max_depth,
                    )?
                } else {
                    let mut matches = Vec::new();
                    for root in &config.runtime_workspace_roots {
                        matches.extend(expand_deny_glob(
                            pattern,
                            root.as_path(),
                            glob_scan_max_depth,
                        )?);
                    }
                    matches
                }
            }
            _ => resolve_entry_paths(config, cwd, entry)?,
        };
        for path in paths {
            let existing = entries.get(&path).copied();
            let access = if existing == Some(FileSystemAccessMode::Deny)
                && command_overlay
                && entry.access != FileSystemAccessMode::Deny
            {
                FileSystemAccessMode::Deny
            } else {
                entry.access
            };
            entries.insert(path, access);
        }
    }
    Ok(())
}
