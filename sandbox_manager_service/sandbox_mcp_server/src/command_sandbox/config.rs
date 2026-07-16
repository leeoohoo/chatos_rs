// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::permissions::*;
use super::*;

pub(super) const MAX_GLOB_MATCHES: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommandSandboxBackend {
    Native,
    External,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandSandboxConfig {
    pub(super) backend: CommandSandboxBackend,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) workspace: PathBuf,
    pub(super) state_root: PathBuf,
    pub(super) temp: PathBuf,
    pub(super) host_home: Option<PathBuf>,
    pub(super) permission_profile: PermissionProfileId,
    pub(super) runtime_workspace_roots: Vec<PathBuf>,
    pub(super) base_file_system: FileSystemPermissionPolicy,
    pub(super) network_unrestricted: bool,
    pub(super) network_proxy: Option<NetworkProxyRuntime>,
}

impl CommandSandboxConfig {
    pub(crate) async fn from_server_config(config: &ServerConfig) -> Result<Self, String> {
        let backend = if config
            .command_sandbox_backend
            .trim()
            .eq_ignore_ascii_case("native")
        {
            CommandSandboxBackend::Native
        } else {
            CommandSandboxBackend::External
        };
        let permission_profile = config.permission_profile.parse::<PermissionProfileId>()?;
        let workspace = canonical_existing_directory(config.workspace.as_path())?;
        let state_root = config
            .state_dir
            .parent()
            .ok_or_else(|| "sandbox state directory has no parent".to_string())
            .and_then(canonical_existing_directory)?;
        let temp = if let Some(temp) = std::env::var_os("TMPDIR").map(PathBuf::from) {
            temp
        } else {
            let temp = state_root.join("tmp");
            std::fs::create_dir_all(temp.as_path()).map_err(|err| {
                format!(
                    "create sandbox fallback temp directory {} failed: {err}",
                    temp.display()
                )
            })?;
            temp
        };
        let temp = canonical_existing_directory(temp.as_path())?;
        let additional_writable_roots = config
            .additional_writable_roots
            .iter()
            .map(|path| existing_directory_preserving_symlinks(path.as_path()))
            .collect::<Result<Vec<_>, _>>()?;
        let host_home = config
            .host_home
            .as_deref()
            .map(canonical_existing_directory)
            .transpose()?;
        let mut runtime_workspace_roots = if let Some(snapshot) = &config.effective_permissions {
            snapshot
                .runtime_workspace_roots
                .iter()
                .map(|path| resolve_configured_path(path, host_home.as_deref()))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            additional_writable_roots.clone()
        };
        runtime_workspace_roots.push(workspace.clone());
        runtime_workspace_roots.sort();
        runtime_workspace_roots.dedup();
        let base_file_system = config
            .effective_permissions
            .as_ref()
            .map(|snapshot| snapshot.file_system.clone())
            .unwrap_or_else(|| {
                legacy_file_system_policy(permission_profile, &additional_writable_roots)
            });
        let (network_unrestricted, network_requirements) = match config
            .effective_permissions
            .as_ref()
            .map(|snapshot| &snapshot.network)
        {
            Some(NetworkPermissionPolicy::Unrestricted) => (true, None),
            Some(NetworkPermissionPolicy::Restricted { requirements }) => {
                (false, Some(requirements.clone()))
            }
            None if permission_profile == PermissionProfileId::FullAccess => (true, None),
            None => (
                false,
                Some(NetworkRequirements {
                    enabled: Some(false),
                    ..Default::default()
                }),
            ),
        };
        let network_proxy = if backend == CommandSandboxBackend::Native {
            match network_requirements.as_ref() {
                Some(requirements) => {
                    NetworkProxyRuntime::start(config.state_dir.as_path(), requirements).await?
                }
                None => None,
            }
        } else {
            None
        };
        Ok(Self {
            backend,
            workspace,
            state_root,
            temp,
            host_home,
            permission_profile,
            runtime_workspace_roots,
            base_file_system,
            network_unrestricted,
            network_proxy,
        })
    }

    pub(crate) fn file_tool_access_policy(&self) -> Result<FileToolAccessPolicy, String> {
        let materialized = materialize_permissions(self, self.workspace.as_path(), None)?;
        let (deny_globs, deny_glob_roots) = compile_file_tool_deny_globs(self)?;
        let mut canonical_entries = BTreeMap::new();
        for entry in materialized.entries {
            let path = canonicalize_preserving_missing(entry.path.as_path())?;
            canonical_entries
                .entry(path)
                .and_modify(|existing: &mut FileSystemAccessMode| {
                    if entry.access.rank() < existing.rank() {
                        *existing = entry.access;
                    }
                })
                .or_insert(entry.access);
        }
        Ok(FileToolAccessPolicy {
            workspace: self.workspace.clone(),
            unrestricted: materialized.unrestricted,
            entries: canonical_entries
                .into_iter()
                .map(|(path, access)| MaterializedEntry { access, path })
                .collect(),
            deny_globs,
            deny_glob_roots,
        })
    }
}

#[derive(Debug)]
pub(crate) struct FileToolAccessPolicy {
    workspace: PathBuf,
    pub(super) unrestricted: bool,
    pub(super) entries: Vec<MaterializedEntry>,
    pub(super) deny_globs: GlobSet,
    pub(super) deny_glob_roots: Vec<PathBuf>,
}

impl FileToolAccessPolicy {
    pub(crate) fn workspace_writes_allowed(&self) -> bool {
        self.unrestricted
            || self.entries.iter().any(|entry| {
                entry.access == FileSystemAccessMode::Write
                    && entry.path.starts_with(self.workspace.as_path())
            })
    }

    pub(crate) fn resolve_workspace_path(&self, value: &str) -> Result<PathBuf, String> {
        let path = Path::new(value);
        let candidate = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace.join(path)
        };
        let candidate = canonicalize_preserving_missing(candidate.as_path())?;
        if !candidate.starts_with(self.workspace.as_path()) {
            return Err(format!(
                "file tool path is outside the configured workspace: {value}"
            ));
        }
        Ok(candidate)
    }

    pub(crate) fn authorize_read(&self, path: &Path) -> Result<(), String> {
        if self.access_for_path(path) == FileSystemAccessMode::Deny {
            return Err(format!(
                "permission profile denies reading {}",
                path.display()
            ));
        }
        Ok(())
    }

    pub(crate) fn authorize_recursive_read(&self, path: &Path) -> Result<(), String> {
        self.authorize_read(path)?;
        if self
            .entries
            .iter()
            .any(|entry| entry.access == FileSystemAccessMode::Deny && entry.path.starts_with(path))
            || self
                .deny_glob_roots
                .iter()
                .any(|root| root.starts_with(path) || path.starts_with(root))
        {
            return Err(format!(
                "permission profile contains denied paths below {}; recursive reads must be narrowed",
                path.display()
            ));
        }
        Ok(())
    }

    pub(crate) fn authorize_write(&self, path: &Path) -> Result<(), String> {
        if self.access_for_path(path) != FileSystemAccessMode::Write {
            return Err(format!(
                "permission profile denies writing {}",
                path.display()
            ));
        }
        Ok(())
    }

    pub(crate) fn authorize_recursive_write(&self, path: &Path) -> Result<(), String> {
        self.authorize_write(path)?;
        if self.entries.iter().any(|entry| {
            entry.access != FileSystemAccessMode::Write
                && entry.path != path
                && entry.path.starts_with(path)
        }) || self
            .deny_glob_roots
            .iter()
            .any(|root| root.starts_with(path) || path.starts_with(root))
        {
            return Err(format!(
                "permission profile protects paths below {}; recursive writes must be narrowed",
                path.display()
            ));
        }
        Ok(())
    }

    pub(super) fn access_for_path(&self, path: &Path) -> FileSystemAccessMode {
        if self.deny_globs.is_match(path) {
            return FileSystemAccessMode::Deny;
        }
        let inherited = self
            .entries
            .iter()
            .filter(|entry| path == entry.path || path.starts_with(entry.path.as_path()))
            .max_by_key(|entry| path_depth_all_platforms(entry.path.as_path()))
            .map(|entry| entry.access);
        inherited.unwrap_or(if self.unrestricted {
            FileSystemAccessMode::Write
        } else {
            FileSystemAccessMode::Deny
        })
    }
}

pub(super) fn compile_file_tool_deny_globs(
    config: &CommandSandboxConfig,
) -> Result<(GlobSet, Vec<PathBuf>), String> {
    let mut builder = GlobSetBuilder::new();
    let mut roots = Vec::new();
    let FileSystemPermissionPolicy::Restricted { entries, .. } = &config.base_file_system else {
        return builder
            .build()
            .map(|set| (set, roots))
            .map_err(|err| format!("build empty deny glob matcher failed: {err}"));
    };
    for entry in entries.iter().filter(|entry| {
        entry.access == FileSystemAccessMode::Deny
            && matches!(entry.path, FileSystemPath::GlobPattern { .. })
    }) {
        let FileSystemPath::GlobPattern { pattern } = &entry.path else {
            continue;
        };
        let patterns = if Path::new(pattern).is_absolute() {
            vec![PathBuf::from(pattern)]
        } else if let Some(relative) = pattern
            .strip_prefix("~/")
            .or_else(|| pattern.strip_prefix("~\\"))
        {
            vec![config
                .host_home
                .as_deref()
                .ok_or_else(|| "host home directory is unavailable".to_string())?
                .join(relative)]
        } else {
            config
                .runtime_workspace_roots
                .iter()
                .map(|root| root.join(pattern))
                .collect()
        };
        for pattern in patterns {
            let text = pattern.to_string_lossy().to_string();
            let mut effective_patterns = vec![text.clone()];
            if let Some(canonical) = canonicalize_glob_static_prefix(text.as_str())? {
                effective_patterns.push(canonical);
            }
            effective_patterns.sort();
            effective_patterns.dedup();
            for effective_pattern in effective_patterns {
                roots.push(deny_glob_static_root(effective_pattern.as_str())?);
                builder.add(
                    GlobBuilder::new(effective_pattern.as_str())
                        .literal_separator(true)
                        .build()
                        .map_err(|err| format!("invalid deny glob {effective_pattern:?}: {err}"))?,
                );
            }
        }
    }
    roots.sort();
    roots.dedup();
    builder
        .build()
        .map(|set| (set, roots))
        .map_err(|err| format!("build deny glob matcher failed: {err}"))
}

pub(super) fn canonicalize_glob_static_prefix(pattern: &str) -> Result<Option<String>, String> {
    let Some(first_meta) = pattern
        .char_indices()
        .find_map(|(index, ch)| matches!(ch, '*' | '?' | '[' | ']').then_some(index))
    else {
        return Ok(None);
    };
    let static_prefix = &pattern[..first_meta];
    if static_prefix.is_empty() {
        return Ok(None);
    }
    let canonical = canonicalize_preserving_missing(Path::new(static_prefix))?;
    let mut canonical = canonical.to_string_lossy().to_string();
    if static_prefix.ends_with(std::path::MAIN_SEPARATOR) && !canonical.ends_with('/') {
        canonical.push('/');
    }
    canonical.push_str(&pattern[first_meta..]);
    Ok((canonical != pattern).then_some(canonical))
}

pub(super) fn deny_glob_static_root(pattern: &str) -> Result<PathBuf, String> {
    let first_meta = pattern
        .char_indices()
        .find_map(|(index, ch)| matches!(ch, '*' | '?' | '[' | ']').then_some(index))
        .ok_or_else(|| format!("deny glob does not contain a glob expression: {pattern}"))?;
    let static_prefix = &pattern[..first_meta];
    let root = if static_prefix.ends_with('/') {
        let trimmed = static_prefix.trim_end_matches('/');
        if trimmed.is_empty() {
            PathBuf::from("/")
        } else {
            PathBuf::from(trimmed)
        }
    } else {
        Path::new(static_prefix)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/"))
    };
    canonicalize_preserving_missing(root.as_path())
}

pub(super) fn path_depth_all_platforms(path: &Path) -> usize {
    path.components().count()
}

pub(super) fn resolve_configured_path(
    value: &str,
    host_home: Option<&Path>,
) -> Result<PathBuf, String> {
    let path = if value == "~" {
        host_home
            .map(Path::to_path_buf)
            .ok_or_else(|| "host home directory is unavailable".to_string())?
    } else if let Some(relative) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        host_home
            .ok_or_else(|| "host home directory is unavailable".to_string())?
            .join(relative)
    } else {
        PathBuf::from(value)
    };
    existing_directory_preserving_symlinks(path.as_path())
}

pub(super) fn legacy_file_system_policy(
    profile: PermissionProfileId,
    additional_writable_roots: &[PathBuf],
) -> FileSystemPermissionPolicy {
    if profile == PermissionProfileId::FullAccess {
        return FileSystemPermissionPolicy::Unrestricted;
    }
    let mut entries = vec![FileSystemSandboxEntry {
        access: FileSystemAccessMode::Read,
        path: FileSystemPath::Special {
            value: FileSystemSpecialPath::Root,
        },
    }];
    if profile == PermissionProfileId::WorkspaceWrite {
        entries.push(FileSystemSandboxEntry {
            access: FileSystemAccessMode::Write,
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::ProjectRoots { subpath: None },
            },
        });
        entries.extend([".git", ".agents", ".codex"].into_iter().map(|subpath| {
            FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::ProjectRoots {
                        subpath: Some(subpath.to_string()),
                    },
                },
            }
        }));
        entries.extend(
            additional_writable_roots
                .iter()
                .map(|path| FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Path {
                        path: path.to_string_lossy().to_string(),
                    },
                }),
        );
    }
    FileSystemPermissionPolicy::Restricted {
        entries,
        glob_scan_max_depth: None,
    }
}
