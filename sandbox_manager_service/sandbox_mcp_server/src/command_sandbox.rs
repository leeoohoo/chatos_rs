// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
#[cfg(target_os = "linux")]
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use chatos_builtin_tools::TerminalCommandPermissions;
use chatos_sandbox_contract::{
    FileSystemAccessMode, FileSystemPath, FileSystemPermissionPolicy, FileSystemSandboxEntry,
    FileSystemSpecialPath, GrantedPermissionProfile, NetworkPermissionPolicy, NetworkRequirements,
    PermissionProfileId,
};
use globset::GlobSet;
use globset::{GlobBuilder, GlobSetBuilder};
use tokio::process::{Child, Command};
use walkdir::WalkDir;

use crate::config::ServerConfig;
use crate::network_proxy::{NetworkProxyEndpoints, NetworkProxyRuntime};

const MAX_GLOB_MATCHES: usize = 8_192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandSandboxBackend {
    Native,
    External,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandSandboxConfig {
    backend: CommandSandboxBackend,
    #[cfg_attr(not(test), allow(dead_code))]
    workspace: PathBuf,
    state_root: PathBuf,
    temp: PathBuf,
    host_home: Option<PathBuf>,
    permission_profile: PermissionProfileId,
    runtime_workspace_roots: Vec<PathBuf>,
    base_file_system: FileSystemPermissionPolicy,
    network_unrestricted: bool,
    network_proxy: Option<NetworkProxyRuntime>,
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
    unrestricted: bool,
    entries: Vec<MaterializedEntry>,
    deny_globs: GlobSet,
    deny_glob_roots: Vec<PathBuf>,
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

    fn access_for_path(&self, path: &Path) -> FileSystemAccessMode {
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

fn compile_file_tool_deny_globs(
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

fn canonicalize_glob_static_prefix(pattern: &str) -> Result<Option<String>, String> {
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

fn deny_glob_static_root(pattern: &str) -> Result<PathBuf, String> {
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

fn path_depth_all_platforms(path: &Path) -> usize {
    path.components().count()
}

fn resolve_configured_path(value: &str, host_home: Option<&Path>) -> Result<PathBuf, String> {
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

fn legacy_file_system_policy(
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

pub(crate) struct PreparedSandboxCommand {
    command: Command,
    cleanup: Vec<TransientPath>,
}

pub(crate) struct SpawnedSandboxCommand {
    pub(crate) child: Child,
    pub(crate) cleanup: CommandSandboxCleanup,
}

pub(crate) struct CommandSandboxCleanup {
    paths: Vec<TransientPath>,
}

impl CommandSandboxCleanup {
    pub(crate) fn run(self) {
        for path in self.paths.into_iter().rev() {
            path.remove_if_unchanged();
        }
    }
}

impl PreparedSandboxCommand {
    pub(crate) fn new(
        config: &CommandSandboxConfig,
        shell: &str,
        command: &str,
        cwd: &Path,
        permissions: &TerminalCommandPermissions,
    ) -> Result<Self, String> {
        let granted = validate_permission_context(permissions)?;
        if config.backend == CommandSandboxBackend::External {
            if granted.is_some() || permissions.requested.is_some() {
                return Err(
                    "temporary permission overlays are unavailable in an externally sandboxed runtime"
                        .to_string(),
                );
            }
            return Ok(Self {
                command: direct_shell_command(shell, command),
                cleanup: Vec::new(),
            });
        }

        let network_access = command_network_access(config, granted);
        if config.permission_profile == PermissionProfileId::FullAccess
            && matches!(network_access, CommandNetworkAccess::Full)
        {
            return Ok(Self {
                command: direct_shell_command(shell, command),
                cleanup: Vec::new(),
            });
        }

        #[cfg(target_os = "macos")]
        {
            prepare_macos_command(config, shell, command, cwd, granted, network_access)
        }
        #[cfg(target_os = "linux")]
        {
            prepare_linux_command(config, shell, command, cwd, granted, network_access)
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = (shell, command, cwd, granted, network_access);
            Err("native command sandbox is unsupported on this operating system".to_string())
        }
    }

    pub(crate) fn command_mut(&mut self) -> &mut Command {
        &mut self.command
    }

    pub(crate) fn spawn(mut self) -> Result<SpawnedSandboxCommand, String> {
        match self.command.spawn() {
            Ok(child) => Ok(SpawnedSandboxCommand {
                child,
                cleanup: CommandSandboxCleanup {
                    paths: self.cleanup,
                },
            }),
            Err(err) => {
                for path in self.cleanup.drain(..).rev() {
                    path.remove_if_unchanged();
                }
                Err(err.to_string())
            }
        }
    }
}

#[derive(Debug, Clone)]
enum CommandNetworkAccess {
    Disabled,
    Proxy(NetworkProxyEndpoints),
    Full,
}

fn command_network_access(
    config: &CommandSandboxConfig,
    granted: Option<&GrantedPermissionProfile>,
) -> CommandNetworkAccess {
    if granted
        .and_then(|grant| grant.network.as_ref())
        .and_then(|network| network.enabled)
        == Some(true)
        || config.network_unrestricted
    {
        return CommandNetworkAccess::Full;
    }
    config
        .network_proxy
        .as_ref()
        .map(|proxy| CommandNetworkAccess::Proxy(proxy.endpoints().clone()))
        .unwrap_or(CommandNetworkAccess::Disabled)
}

fn direct_shell_command(shell: &str, command: &str) -> Command {
    let mut process = Command::new(shell);
    process.arg("-lc").arg(command);
    process
}

fn validate_permission_context(
    permissions: &TerminalCommandPermissions,
) -> Result<Option<&GrantedPermissionProfile>, String> {
    match (&permissions.requested, &permissions.granted) {
        (None, None) => Ok(None),
        (Some(_), None) => Err("requested permission overlay was not granted".to_string()),
        (None, Some(_)) => Err("granted permission overlay has no matching request".to_string()),
        (Some(requested), Some(granted)) => {
            requested.validate()?;
            if let Some(file_system) = &granted.file_system {
                file_system.validate()?;
            }
            if !requested.allows_grant(granted) {
                return Err("granted permission overlay exceeds the request".to_string());
            }
            Ok(Some(granted))
        }
    }
}

#[cfg(target_os = "macos")]
fn prepare_macos_command(
    config: &CommandSandboxConfig,
    shell: &str,
    command: &str,
    cwd: &Path,
    granted: Option<&GrantedPermissionProfile>,
    network_access: CommandNetworkAccess,
) -> Result<PreparedSandboxCommand, String> {
    let materialized = materialize_permissions(config, cwd, granted)?;
    let mut profile = String::from(include_str!(
        "../../../local_connector_client/core/src/sandbox/process/seatbelt_base_policy.sbpl"
    ));
    profile.push_str("\n; ChatOS command-scoped permission profile\n");
    let mut params = Vec::new();
    append_macos_path_rule(
        &mut profile,
        &mut params,
        "STATE_ROOT",
        config.state_root.as_path(),
        FileSystemAccessMode::Write,
        &[],
    );
    append_macos_path_rule(
        &mut profile,
        &mut params,
        "TEMP_ROOT",
        config.temp.as_path(),
        FileSystemAccessMode::Write,
        &[],
    );
    if materialized.include_platform_defaults && !materialized.full_disk_read {
        profile.push_str("\n; restricted-read platform defaults\n");
        profile.push_str(include_str!("restricted_read_only_platform_defaults.sbpl"));
        profile.push('\n');
    }
    if materialized.unrestricted {
        profile.push_str("(allow file-read* file-write*)\n");
    }
    let writable_roots = materialized_writable_roots(&materialized);
    let allowed_write_paths = allowed_write_paths(writable_roots.as_slice());
    for (index, entry) in materialized.entries.iter().enumerate() {
        let path = remap_path_for_writable_root(entry.path.as_path(), writable_roots.as_slice());
        if entry.access != FileSystemAccessMode::Write
            && is_within_allowed_write_paths(path.as_path(), allowed_write_paths.as_slice())
        {
            fail_if_protected_path_crosses_writable_symlink(
                path.as_path(),
                entry.access,
                allowed_write_paths.as_slice(),
            )?;
        }
        let mut exclusion_entries = materialized
            .entries
            .iter()
            .filter(|candidate| {
                candidate.path != entry.path
                    && candidate.path.starts_with(entry.path.as_path())
                    && match entry.access {
                        FileSystemAccessMode::Write => {
                            candidate.access != FileSystemAccessMode::Write
                        }
                        FileSystemAccessMode::Deny => {
                            candidate.access == FileSystemAccessMode::Write
                        }
                        FileSystemAccessMode::Read => false,
                    }
            })
            .map(|candidate| {
                (
                    remap_path_for_writable_root(
                        candidate.path.as_path(),
                        writable_roots.as_slice(),
                    ),
                    candidate.access,
                )
            })
            .collect::<Vec<_>>();
        exclusion_entries.sort_by(|left, right| left.0.cmp(&right.0));
        exclusion_entries.dedup_by(|left, right| left.0 == right.0);
        for (excluded, excluded_access) in &exclusion_entries {
            if entry.access == FileSystemAccessMode::Write {
                fail_if_protected_path_crosses_writable_symlink(
                    excluded.as_path(),
                    *excluded_access,
                    allowed_write_paths.as_slice(),
                )?;
            }
        }
        let exclusions = exclusion_entries
            .into_iter()
            .map(|(path, _)| path)
            .collect::<Vec<_>>();
        append_macos_path_rule(
            &mut profile,
            &mut params,
            format!("FILESYSTEM_{index}").as_str(),
            path.as_path(),
            entry.access,
            exclusions.as_slice(),
        );
    }
    match &network_access {
        CommandNetworkAccess::Disabled => {}
        CommandNetworkAccess::Full => profile.push_str("(allow network*)\n"),
        CommandNetworkAccess::Proxy(endpoints) => {
            profile.push_str("\n; proxy-only command network access\n");
            for port in endpoints.loopback_ports() {
                profile.push_str(
                    format!("(allow network-outbound (remote ip \"localhost:{port}\"))\n").as_str(),
                );
            }
            profile.push_str(include_str!("seatbelt_network_policy.sbpl"));
        }
    }

    let mut process = Command::new("/usr/bin/sandbox-exec");
    process.arg("-p").arg(profile);
    for (key, value) in params {
        process.arg(format!("-D{key}={}", value.to_string_lossy()));
    }
    process.arg("--").arg(shell).arg("-lc").arg(command);
    process.current_dir(cwd);
    process.env("TMPDIR", config.temp.as_os_str());
    if let CommandNetworkAccess::Proxy(endpoints) = &network_access {
        endpoints.apply_to_command(&mut process);
    }
    Ok(PreparedSandboxCommand {
        command: process,
        cleanup: Vec::new(),
    })
}

#[cfg(target_os = "macos")]
fn fail_if_protected_path_crosses_writable_symlink(
    path: &Path,
    access: FileSystemAccessMode,
    allowed_write_paths: &[PathBuf],
) -> Result<(), String> {
    let Some(symlink) = first_writable_symlink_component_in_path(path, allowed_write_paths) else {
        return Ok(());
    };
    let protection = match access {
        FileSystemAccessMode::Read => "read-only",
        FileSystemAccessMode::Deny => "deny-read",
        FileSystemAccessMode::Write => return Ok(()),
    };
    Err(format!(
        "cannot enforce sandbox {protection} path {} because it crosses writable symlink {}",
        path.display(),
        symlink.display()
    ))
}

#[cfg(target_os = "macos")]
fn append_macos_path_rule(
    profile: &mut String,
    params: &mut Vec<(String, PathBuf)>,
    key: &str,
    root: &Path,
    access: FileSystemAccessMode,
    exclusions: &[PathBuf],
) {
    params.push((key.to_string(), root.to_path_buf()));
    let operation = match access {
        FileSystemAccessMode::Read => "allow file-read*",
        FileSystemAccessMode::Write => "allow file-read* file-write*",
        FileSystemAccessMode::Deny => "deny file-read* file-write*",
    };
    profile.push_str(format!("({operation}\n  (require-all\n    (require-any (literal (param \"{key}\")) (subpath (param \"{key}\")))\n").as_str());
    for (index, excluded) in exclusions.iter().enumerate() {
        let excluded_key = format!("{key}_EXCLUDED_{index}");
        params.push((excluded_key.clone(), excluded.clone()));
        profile.push_str(
            format!(
                "    (require-not (literal (param \"{excluded_key}\")))\n    (require-not (subpath (param \"{excluded_key}\")))\n"
            )
            .as_str(),
        );
    }
    profile.push_str("  ))\n");
    if access != FileSystemAccessMode::Deny && root != Path::new("/") {
        profile.push_str(
            format!(
                "(allow file-read-metadata file-test-existence (path-ancestors (param \"{key}\")))\n"
            )
            .as_str(),
        );
    }
}

#[cfg(target_os = "linux")]
fn prepare_linux_command(
    config: &CommandSandboxConfig,
    shell: &str,
    command: &str,
    cwd: &Path,
    granted: Option<&GrantedPermissionProfile>,
    network_access: CommandNetworkAccess,
) -> Result<PreparedSandboxCommand, String> {
    let materialized = materialize_permissions(config, cwd, granted)?;
    let bwrap = find_executable("bwrap")
        .ok_or_else(|| "Bubblewrap is not available on PATH".to_string())?;
    let wrapper_executable = linux_wrapper_executable()?;
    let mut cleanup = Vec::new();
    let wrapper_directory_name = format!(".chatos-sandbox-wrapper-{}", uuid::Uuid::new_v4());
    let wrapper_host_directory = config.temp.join(wrapper_directory_name.as_str());
    cleanup.push(TransientPath::create_directory(
        wrapper_host_directory.as_path(),
    )?);
    let wrapper_host_path = wrapper_host_directory.join("agent");
    cleanup.push(TransientPath::create_file(wrapper_host_path.as_path())?);
    let wrapper_sandbox_path = PathBuf::from("/tmp")
        .join(wrapper_directory_name)
        .join("agent");
    // Open descriptor 3 in a fixed launcher script so Bubblewrap's --ro-bind-data can create
    // unreadable or read-only synthetic files without mutating the host filesystem. Passing an
    // already-open descriptor through tokio::process is not portable because some spawn paths
    // close all non-standard descriptors before exec.
    let mut process = Command::new("/bin/sh");
    process
        .arg("-c")
        .arg("exec 3</dev/null\nexec \"$@\"")
        .arg("chatos-bwrap-launcher")
        .arg(bwrap);
    process.args([
        "--new-session",
        "--die-with-parent",
        "--unshare-user",
        "--unshare-ipc",
        "--unshare-pid",
        "--unshare-uts",
        "--unshare-cgroup-try",
        "--cap-drop",
        "ALL",
    ]);
    if !matches!(network_access, CommandNetworkAccess::Full) {
        process.arg("--unshare-net");
    }
    if materialized.unrestricted {
        process.args(["--bind", "/", "/"]);
    } else if materialized.full_disk_read {
        process.args(["--ro-bind", "/", "/"]);
    } else {
        process.args(["--tmpfs", "/"]);
    }
    process.args(["--proc", "/proc", "--dev", "/dev"]);
    process.arg("--bind").arg(config.temp.as_path()).arg("/tmp");
    process
        .arg("--bind")
        .arg(config.state_root.as_path())
        .arg(config.state_root.as_path());
    if !materialized.unrestricted && !materialized.full_disk_read {
        append_linux_restricted_read_mounts(&mut process, &materialized);
    }
    let sandbox_cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    if sandbox_cwd.starts_with(Path::new("/tmp"))
        && !sandbox_cwd.starts_with(config.temp.as_path())
        && materialized.access_for_path(sandbox_cwd.as_path()) != FileSystemAccessMode::Deny
    {
        process
            .arg("--ro-bind")
            .arg(sandbox_cwd.as_path())
            .arg(sandbox_cwd.as_path());
    }
    // The broker executable may itself live below the host TMPDIR. Mount it back at a private,
    // randomized read-only path after replacing `/tmp`, so the in-namespace wrapper is reachable
    // without trusting an executable stored in the writable workspace.
    process
        .arg("--ro-bind")
        .arg(wrapper_executable.as_path())
        .arg(wrapper_sandbox_path.as_path());
    if let CommandNetworkAccess::Proxy(endpoints) = &network_access {
        for directory in endpoints.linux_bridge_directories() {
            process.arg("--ro-bind").arg(directory).arg(directory);
        }
    }

    append_linux_file_system_mounts(&mut process, &materialized, &mut cleanup)?;
    process.arg("--chdir").arg(sandbox_cwd).arg("--");
    if let CommandNetworkAccess::Proxy(endpoints) = &network_access {
        process.arg(wrapper_sandbox_path.as_path());
        process.args(endpoints.linux_wrapper_arguments());
        process.arg(shell).arg("-lc").arg(command);
        endpoints.apply_to_command(&mut process);
    } else {
        process
            .arg(wrapper_sandbox_path.as_path())
            .arg("--internal-command-wrapper")
            .arg("--")
            .arg(shell)
            .arg("-lc")
            .arg(command);
    }
    process.env("TMPDIR", "/tmp");
    Ok(PreparedSandboxCommand {
        command: process,
        cleanup,
    })
}

#[cfg(target_os = "linux")]
fn append_linux_restricted_read_mounts(
    command: &mut Command,
    materialized: &MaterializedPermissions,
) {
    let writable_roots = materialized_writable_roots(materialized);
    let mut readable_roots = materialized
        .entries
        .iter()
        .filter(|entry| entry.access != FileSystemAccessMode::Deny && entry.path.exists())
        .map(|entry| remap_path_for_writable_root(entry.path.as_path(), writable_roots.as_slice()))
        .filter(|path| path != Path::new("/") && path != Path::new("/dev"))
        .collect::<BTreeSet<_>>();
    if materialized.include_platform_defaults {
        readable_roots.extend(minimal_platform_paths());
    }
    for root in readable_roots {
        command.arg("--ro-bind").arg(&root).arg(&root);
    }
}

#[cfg(target_os = "linux")]
fn append_linux_file_system_mounts(
    command: &mut Command,
    materialized: &MaterializedPermissions,
    cleanup: &mut Vec<TransientPath>,
) -> Result<(), String> {
    let mut writable_roots = materialized_writable_roots(materialized);
    writable_roots.retain(|root| root.logical.exists());
    if materialized.unrestricted && !materialized.entries.is_empty() {
        writable_roots.push(MaterializedWritableRoot {
            logical: PathBuf::from("/"),
            mount: PathBuf::from("/"),
        });
    }
    writable_roots.sort_by_key(|root| path_depth(root.logical.as_path()));
    writable_roots.dedup_by(|left, right| left.logical == right.logical);
    let allowed_write_paths = allowed_write_paths(writable_roots.as_slice());
    let writable_mount_roots = writable_roots
        .iter()
        .map(|root| root.mount.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let mut read_only_paths = materialized
        .entries
        .iter()
        .filter(|entry| entry.access == FileSystemAccessMode::Read)
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    read_only_paths.sort_by_key(|path| path_depth(path.as_path()));
    let mut denied_paths = materialized
        .entries
        .iter()
        .filter(|entry| entry.access == FileSystemAccessMode::Deny)
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    denied_paths.sort_by_key(|path| path_depth(path.as_path()));

    // A deny ancestor outside all writable roots must be installed first. A more-specific
    // writable entry can then recreate its mount target and deliberately reopen that child.
    for denied in &denied_paths {
        let denied = remap_path_for_writable_root(denied.as_path(), writable_roots.as_slice());
        if !allowed_write_paths
            .iter()
            .any(|root| denied.starts_with(root))
            && writable_mount_roots
                .iter()
                .any(|root| root.starts_with(denied.as_path()))
        {
            append_linux_deny_mask(
                command,
                denied.as_path(),
                allowed_write_paths.as_slice(),
                writable_mount_roots.as_slice(),
                cleanup,
            )?;
        }
    }

    // Process writable roots from broad to narrow. Reapplying read/deny carve-outs after each
    // bind lets a nested writable root reopen only the child explicitly named by the policy.
    for root in &writable_roots {
        command.arg("--bind").arg(&root.mount).arg(&root.mount);

        let mut nested_read_only = read_only_paths
            .iter()
            .filter(|path| {
                path.as_path() != root.logical.as_path()
                    && (path.starts_with(root.logical.as_path())
                        || path.starts_with(root.mount.as_path()))
            })
            .map(|path| remap_path_for_writable_root(path, writable_roots.as_slice()))
            .collect::<BTreeSet<_>>();
        for read_only in &nested_read_only {
            append_linux_read_only_mount(
                command,
                read_only,
                allowed_write_paths.as_slice(),
                cleanup,
            )?;
        }
        nested_read_only.clear();

        let nested_denied = denied_paths
            .iter()
            .filter(|path| {
                path.starts_with(root.logical.as_path()) || path.starts_with(root.mount.as_path())
            })
            .map(|path| remap_path_for_writable_root(path, writable_roots.as_slice()))
            .collect::<BTreeSet<_>>();
        for denied in nested_denied {
            append_linux_deny_mask(
                command,
                denied.as_path(),
                allowed_write_paths.as_slice(),
                writable_mount_roots.as_slice(),
                cleanup,
            )?;
        }
    }

    // Denies unrelated to writable roots still need masking on top of the read-only root view.
    for denied in &denied_paths {
        let denied = remap_path_for_writable_root(denied.as_path(), writable_roots.as_slice());
        if !allowed_write_paths
            .iter()
            .any(|root| denied.starts_with(root) || root.starts_with(denied.as_path()))
        {
            append_linux_deny_mask(
                command,
                denied.as_path(),
                allowed_write_paths.as_slice(),
                writable_mount_roots.as_slice(),
                cleanup,
            )?;
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn append_linux_read_only_mount(
    command: &mut Command,
    path: &Path,
    allowed_write_paths: &[PathBuf],
    _cleanup: &mut Vec<TransientPath>,
) -> Result<(), String> {
    if path == Path::new("/") || !is_within_allowed_write_paths(path, allowed_write_paths) {
        return Ok(());
    }
    if let Some(symlink) = first_writable_symlink_component_in_path(path, allowed_write_paths) {
        return Err(format!(
            "cannot enforce sandbox read-only path {} because it crosses writable symlink {}",
            path.display(),
            symlink.display()
        ));
    }
    if !path.exists() {
        let missing = first_missing_component(path)
            .ok_or_else(|| format!("cannot materialize read-only path {}", path.display()))?;
        if missing
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, ".git" | ".agents" | ".codex"))
        {
            command
                .arg("--perms")
                .arg("555")
                .arg("--tmpfs")
                .arg(missing.as_path())
                .arg("--remount-ro")
                .arg(missing.as_path());
        } else {
            append_linux_empty_file_mount(command, missing.as_path(), "444");
        }
        return Ok(());
    }
    command.arg("--ro-bind").arg(path).arg(path);
    Ok(())
}

#[cfg(target_os = "linux")]
fn linux_wrapper_executable() -> Result<PathBuf, String> {
    #[cfg(test)]
    if let Some(path) = std::env::var_os("CHATOS_SANDBOX_TEST_WRAPPER_EXECUTABLE") {
        let path = PathBuf::from(path);
        let path = path.canonicalize().map_err(|err| {
            format!(
                "canonicalize sandbox test wrapper {} failed: {err}",
                path.display()
            )
        })?;
        if !path.is_file() {
            return Err(format!(
                "sandbox test wrapper is not a file: {}",
                path.display()
            ));
        }
        return Ok(path);
    }

    std::env::current_exe().map_err(|err| format!("resolve sandbox agent executable failed: {err}"))
}

#[cfg(target_os = "linux")]
fn append_linux_deny_mask(
    command: &mut Command,
    path: &Path,
    allowed_write_paths: &[PathBuf],
    writable_mount_roots: &[PathBuf],
    _cleanup: &mut Vec<TransientPath>,
) -> Result<(), String> {
    if path == Path::new("/") {
        return Err("denying the filesystem root is not a runnable command policy".to_string());
    }
    if let Some(symlink) = first_writable_symlink_component_in_path(path, allowed_write_paths) {
        return Err(format!(
            "cannot enforce sandbox deny-read path {} because it crosses writable symlink {}",
            path.display(),
            symlink.display()
        ));
    }
    if !path.exists() {
        let missing = first_missing_component(path)
            .ok_or_else(|| format!("cannot materialize denied path {}", path.display()))?;
        if is_within_allowed_write_paths(missing.as_path(), allowed_write_paths) {
            append_linux_empty_file_mount(command, missing.as_path(), "000");
        }
    } else if path.is_dir() {
        let mut writable_descendants = writable_mount_roots
            .iter()
            .filter(|root| root.as_path() != path && root.starts_with(path))
            .collect::<Vec<_>>();
        writable_descendants.sort_by_key(|root| path_depth(root.as_path()));
        command
            .arg("--perms")
            .arg(if writable_descendants.is_empty() {
                "000"
            } else {
                "111"
            })
            .arg("--tmpfs")
            .arg(path);
        for descendant in writable_descendants {
            append_linux_mount_target_parent_dirs(command, descendant, path);
        }
        command.arg("--remount-ro").arg(path);
    } else {
        append_linux_empty_file_mount(command, path, "000");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn append_linux_empty_file_mount(command: &mut Command, path: &Path, permissions: &str) {
    command
        .arg("--perms")
        .arg(permissions)
        .arg("--ro-bind-data")
        .arg("3")
        .arg(path);
}

#[cfg(target_os = "linux")]
fn append_linux_mount_target_parent_dirs(command: &mut Command, target: &Path, anchor: &Path) {
    let target_directory = if target.is_dir() {
        target
    } else if let Some(parent) = target.parent() {
        parent
    } else {
        return;
    };
    let mut directories = target_directory
        .ancestors()
        .take_while(|path| *path != anchor)
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    directories.reverse();
    for directory in directories {
        command.arg("--dir").arg(directory);
    }
}

#[derive(Debug)]
struct MaterializedPermissions {
    unrestricted: bool,
    full_disk_read: bool,
    include_platform_defaults: bool,
    entries: Vec<MaterializedEntry>,
}

impl MaterializedPermissions {
    #[cfg(target_os = "linux")]
    fn access_for_path(&self, path: &Path) -> FileSystemAccessMode {
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
struct MaterializedEntry {
    access: FileSystemAccessMode,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MaterializedWritableRoot {
    logical: PathBuf,
    mount: PathBuf,
}

fn materialized_writable_roots(
    materialized: &MaterializedPermissions,
) -> Vec<MaterializedWritableRoot> {
    let mut roots = materialized
        .entries
        .iter()
        .filter(|entry| entry.access == FileSystemAccessMode::Write)
        .map(|entry| MaterializedWritableRoot {
            logical: entry.path.clone(),
            mount: canonical_target_if_symlinked_path(entry.path.as_path())
                .unwrap_or_else(|| entry.path.clone()),
        })
        .collect::<Vec<_>>();
    roots.sort_by(|left, right| {
        path_depth_all_platforms(left.logical.as_path())
            .cmp(&path_depth_all_platforms(right.logical.as_path()))
            .then_with(|| left.logical.cmp(&right.logical))
    });
    roots.dedup_by(|left, right| left.logical == right.logical);
    roots
}

fn allowed_write_paths(writable_roots: &[MaterializedWritableRoot]) -> Vec<PathBuf> {
    let mut paths = writable_roots
        .iter()
        .flat_map(|root| [root.logical.clone(), root.mount.clone()])
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

fn remap_path_for_writable_root(
    path: &Path,
    writable_roots: &[MaterializedWritableRoot],
) -> PathBuf {
    let Some(root) = writable_roots
        .iter()
        .filter(|root| path.starts_with(root.logical.as_path()))
        .max_by_key(|root| path_depth_all_platforms(root.logical.as_path()))
    else {
        return path.to_path_buf();
    };
    if root.logical == root.mount {
        return path.to_path_buf();
    }
    path.strip_prefix(root.logical.as_path())
        .map(|relative| root.mount.join(relative))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn is_within_allowed_write_paths(path: &Path, allowed_write_paths: &[PathBuf]) -> bool {
    allowed_write_paths
        .iter()
        .any(|root| path.starts_with(root.as_path()))
}

fn first_writable_symlink_component_in_path(
    target_path: &Path,
    allowed_write_paths: &[PathBuf],
) -> Option<PathBuf> {
    let mut current = PathBuf::new();
    for component in target_path.components() {
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

        let metadata = match std::fs::symlink_metadata(current.as_path()) {
            Ok(metadata) => metadata,
            Err(_) => break,
        };
        if metadata.file_type().is_symlink()
            && is_within_allowed_write_paths(current.as_path(), allowed_write_paths)
        {
            return Some(current);
        }
    }
    None
}

fn materialize_permissions(
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
        || entries
            .get(Path::new("/"))
            .is_some_and(|access| *access != FileSystemAccessMode::Deny);
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

fn materialize_file_system_entries(
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

fn resolve_entry_paths(
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
            FileSystemSpecialPath::Root => Ok(vec![PathBuf::from("/")]),
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

fn resolve_permission_path(
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

fn join_beneath(root: &Path, subpath: &str) -> Result<PathBuf, String> {
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

fn normalize_policy_path_preserving_symlinks(path: &Path) -> Result<PathBuf, String> {
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

fn canonicalize_preserving_missing(path: &Path) -> Result<PathBuf, String> {
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

fn expand_deny_glob(
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

fn minimal_platform_paths() -> Vec<PathBuf> {
    [
        "/bin", "/sbin", "/usr", "/etc", "/lib", "/lib64", "/System", "/Library",
    ]
    .into_iter()
    .map(PathBuf::from)
    .filter(|path| path.exists())
    .collect()
}

fn canonical_target_if_symlinked_path(path: &Path) -> Option<PathBuf> {
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
fn path_depth(path: &Path) -> usize {
    path.components().count()
}

fn canonical_existing_directory(path: &Path) -> Result<PathBuf, String> {
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

fn existing_directory_preserving_symlinks(path: &Path) -> Result<PathBuf, String> {
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
fn find_executable(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .map(Path::new)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(target_os = "linux")]
fn first_missing_component(path: &Path) -> Option<PathBuf> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if !current.exists() {
            return Some(current);
        }
    }
    None
}

#[derive(Debug)]
struct TransientPath {
    path: PathBuf,
    kind: TransientPathKind,
    identity: Option<FileIdentity>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
enum TransientPathKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl TransientPath {
    #[cfg(target_os = "linux")]
    fn create_directory(path: &Path) -> Result<Self, String> {
        std::fs::create_dir(path).map_err(|err| {
            format!(
                "create protected mount target {} failed: {err}",
                path.display()
            )
        })?;
        let identity = file_identity(path);
        Ok(Self {
            path: path.to_path_buf(),
            kind: TransientPathKind::Directory,
            identity,
        })
    }

    #[cfg(target_os = "linux")]
    fn create_file(path: &Path) -> Result<Self, String> {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|err| {
                format!(
                    "create denied mount target {} failed: {err}",
                    path.display()
                )
            })?;
        let identity = file_identity(path);
        Ok(Self {
            path: path.to_path_buf(),
            kind: TransientPathKind::File,
            identity,
        })
    }

    fn remove_if_unchanged(self) {
        if self.identity.is_some() && file_identity(self.path.as_path()) != self.identity {
            return;
        }
        match self.kind {
            TransientPathKind::File => {
                let _ = std::fs::remove_file(self.path);
            }
            TransientPathKind::Directory => {
                let _ = std::fs::remove_dir(self.path);
            }
        }
    }
}

#[cfg(unix)]
fn file_identity(path: &Path) -> Option<FileIdentity> {
    use std::os::unix::fs::MetadataExt;

    let metadata = std::fs::symlink_metadata(path).ok()?;
    Some(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(not(unix))]
fn file_identity(_path: &Path) -> Option<FileIdentity> {
    None
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests {
    use super::*;
    use chatos_sandbox_contract::{
        AdditionalFileSystemPermissions, AdditionalNetworkPermissions, NetworkDomainPermission,
        NetworkProxyMode, NetworkRequirements, RequestPermissionProfile,
    };
    use rcgen::{
        BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
        KeyUsagePurpose, SanType, PKCS_ECDSA_P256_SHA256,
    };
    use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
    use rustls::ServerConfig as TlsServerConfig;
    use std::collections::BTreeMap;
    use std::net::{IpAddr, Ipv4Addr};
    use std::process::Stdio;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;

    fn config(root: &Path, profile: PermissionProfileId) -> CommandSandboxConfig {
        let workspace = root.join("workspace");
        let state_root = root.join("state");
        let home = state_root.join("home");
        let temp = state_root.join("tmp");
        for path in [&workspace, &home, &temp] {
            std::fs::create_dir_all(path).expect("create path");
        }
        std::fs::create_dir_all(workspace.join(".git")).expect("git");
        let workspace = workspace.canonicalize().expect("workspace");
        CommandSandboxConfig {
            backend: CommandSandboxBackend::Native,
            workspace: workspace.clone(),
            state_root: state_root.canonicalize().expect("state"),
            temp: temp.canonicalize().expect("temp"),
            host_home: std::env::var_os("HOME").map(PathBuf::from),
            permission_profile: profile,
            runtime_workspace_roots: vec![workspace],
            base_file_system: legacy_file_system_policy(profile, &[]),
            network_unrestricted: profile == PermissionProfileId::FullAccess,
            network_proxy: None,
        }
    }

    #[cfg(unix)]
    #[test]
    fn permission_paths_preserve_symlinks_until_backend_materialization() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "chatos-command-symlink-normalization-test-{}",
            uuid::Uuid::new_v4()
        ));
        let real = root.join("real");
        let link = root.join("link");
        std::fs::create_dir_all(real.as_path()).expect("real root");
        symlink(real.as_path(), link.as_path()).expect("symlink root");

        let logical = link.join("missing/child");
        assert_eq!(
            normalize_policy_path_preserving_symlinks(logical.as_path())
                .expect("normalize logical path"),
            logical
        );
        assert_eq!(
            canonicalize_preserving_missing(logical.as_path()).expect("canonicalize target"),
            real.canonicalize()
                .expect("canonical real root")
                .join("missing/child")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_writable_roots_use_real_targets_and_nested_symlinks_fail_closed() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "chatos-command-symlink-policy-test-{}",
            uuid::Uuid::new_v4()
        ));
        let real = root.join("real");
        let link = root.join("link");
        let outside = root.join("outside");
        std::fs::create_dir_all(real.join("readonly")).expect("real readonly");
        std::fs::create_dir_all(outside.as_path()).expect("outside");
        symlink(real.as_path(), link.as_path()).expect("symlink root");
        symlink(outside.as_path(), real.join("escape")).expect("nested symlink");
        let real_target = real.canonicalize().expect("canonical real root");

        let materialized = MaterializedPermissions {
            unrestricted: false,
            full_disk_read: false,
            include_platform_defaults: false,
            entries: vec![
                MaterializedEntry {
                    access: FileSystemAccessMode::Write,
                    path: link.clone(),
                },
                MaterializedEntry {
                    access: FileSystemAccessMode::Read,
                    path: link.join("readonly"),
                },
            ],
        };
        let writable_roots = materialized_writable_roots(&materialized);
        assert_eq!(
            writable_roots,
            vec![MaterializedWritableRoot {
                logical: link.clone(),
                mount: real_target.clone(),
            }]
        );
        assert_eq!(
            remap_path_for_writable_root(
                link.join("readonly").as_path(),
                writable_roots.as_slice()
            ),
            real_target.join("readonly")
        );

        let allowed = allowed_write_paths(writable_roots.as_slice());
        let escaped = remap_path_for_writable_root(
            link.join("escape/secret.txt").as_path(),
            writable_roots.as_slice(),
        );
        assert_eq!(
            first_writable_symlink_component_in_path(escaped.as_path(), allowed.as_slice()),
            Some(real_target.join("escape"))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn deny_glob_expansion_keeps_logical_matches_and_real_targets() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "chatos-command-symlink-glob-test-{}",
            uuid::Uuid::new_v4()
        ));
        let real = root.join("real");
        let link = root.join("link");
        std::fs::create_dir_all(real.join("nested")).expect("real nested");
        std::fs::write(real.join("nested/secret.env"), "secret").expect("secret file");
        symlink(real.as_path(), link.as_path()).expect("symlink root");

        let matches = expand_deny_glob(
            format!("{}/**/*.env", link.display()).as_str(),
            root.as_path(),
            Some(4),
        )
        .expect("expand deny glob");
        assert!(
            matches.contains(&link.join("nested/secret.env")),
            "{matches:?}"
        );
        assert!(
            matches.contains(
                &real
                    .canonicalize()
                    .expect("canonical real root")
                    .join("nested/secret.env")
            ),
            "{matches:?}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn native_workspace_metadata_symlink_is_rejected_fail_closed() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "chatos-command-seatbelt-symlink-test-{}",
            uuid::Uuid::new_v4()
        ));
        let config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
        let target = root.join("git-target");
        std::fs::create_dir_all(target.as_path()).expect("git target");
        std::fs::remove_dir_all(config.workspace.join(".git")).expect("remove ordinary git dir");
        symlink(target.as_path(), config.workspace.join(".git")).expect("symlink git dir");

        let error = PreparedSandboxCommand::new(
            &config,
            "/bin/sh",
            "true",
            config.workspace.as_path(),
            &TerminalCommandPermissions::default(),
        )
        .err()
        .expect("writable metadata symlink should fail closed");
        assert!(error.contains("crosses writable symlink"), "{error}");
        let _ = std::fs::remove_dir_all(root);
    }

    async fn run(
        config: &CommandSandboxConfig,
        command: String,
        permissions: TerminalCommandPermissions,
    ) -> std::process::Output {
        let mut prepared = PreparedSandboxCommand::new(
            config,
            "/bin/sh",
            command.as_str(),
            config.workspace.as_path(),
            &permissions,
        )
        .expect("prepare command");
        prepared
            .command_mut()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let spawned = prepared.spawn().expect("spawn");
        let output = spawned.child.wait_with_output().await.expect("output");
        spawned.cleanup.run();
        output
    }

    #[tokio::test]
    async fn workspace_policy_and_command_overlay_are_enforced_per_child() {
        let root = std::env::temp_dir().join(format!(
            "chatos-command-sandbox-test-{}",
            uuid::Uuid::new_v4()
        ));
        let config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside).expect("outside");
        let base = run(
            &config,
            format!(
                "touch '{}' && ! touch '{}' && ! touch '{}'",
                config.workspace.join("inside").display(),
                outside.join("blocked").display(),
                config.workspace.join(".git/blocked").display()
            ),
            TerminalCommandPermissions::default(),
        )
        .await;
        assert!(
            base.status.success(),
            "{}",
            String::from_utf8_lossy(&base.stderr)
        );

        let requested = RequestPermissionProfile {
            file_system: Some(AdditionalFileSystemPermissions {
                entries: Some(vec![FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Path {
                        path: outside.to_string_lossy().to_string(),
                    },
                }]),
                ..Default::default()
            }),
            network: None,
        };
        let elevated = run(
            &config,
            format!("touch '{}'", outside.join("allowed").display()),
            TerminalCommandPermissions {
                requested: Some(requested.clone()),
                granted: Some(requested.into()),
            },
        )
        .await;
        assert!(
            elevated.status.success(),
            "{}",
            String::from_utf8_lossy(&elevated.stderr)
        );
        assert!(outside.join("allowed").exists());

        std::fs::write(outside.join("secret.env"), "secret").expect("secret file");
        let constrained_request = RequestPermissionProfile {
            file_system: Some(AdditionalFileSystemPermissions {
                entries: Some(vec![
                    FileSystemSandboxEntry {
                        access: FileSystemAccessMode::Write,
                        path: FileSystemPath::Path {
                            path: outside.to_string_lossy().to_string(),
                        },
                    },
                    FileSystemSandboxEntry {
                        access: FileSystemAccessMode::Deny,
                        path: FileSystemPath::GlobPattern {
                            pattern: format!("{}/**/*.env", outside.display()),
                        },
                    },
                ]),
                glob_scan_max_depth: Some(3),
                ..Default::default()
            }),
            network: None,
        };
        let constrained = run(
            &config,
            format!(
                "touch '{}' && ! cat '{}' && ! rm '{}'",
                outside.join("ordinary.txt").display(),
                outside.join("secret.env").display(),
                outside.join("secret.env").display(),
            ),
            TerminalCommandPermissions {
                requested: Some(constrained_request.clone()),
                granted: Some(constrained_request.into()),
            },
        )
        .await;
        assert!(
            constrained.status.success(),
            "{}",
            String::from_utf8_lossy(&constrained.stderr)
        );
        assert!(outside.join("ordinary.txt").exists());
        assert!(outside.join("secret.env").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[tokio::test]
    async fn symlinked_writable_root_binds_real_target_and_preserves_read_carveout() {
        use std::os::unix::fs::symlink;

        let root = std::env::var_os("HOME")
            .map(PathBuf::from)
            .expect("host home")
            .join(format!(
                ".chatos-command-symlinked-write-root-test-{}",
                uuid::Uuid::new_v4()
            ));
        let mut config = config(root.as_path(), PermissionProfileId::ReadOnly);
        let real = root.join("real-write-root");
        let link = root.join("linked-write-root");
        std::fs::create_dir_all(real.join("readonly")).expect("readonly root");
        symlink(real.as_path(), link.as_path()).expect("symlink writable root");
        config.workspace = real.canonicalize().expect("canonical real workspace");
        config.runtime_workspace_roots = vec![config.workspace.clone()];
        config.base_file_system = FileSystemPermissionPolicy::Restricted {
            entries: vec![
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::Root,
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Path {
                        path: link.to_string_lossy().to_string(),
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Path {
                        path: link.join("readonly").to_string_lossy().to_string(),
                    },
                },
            ],
            glob_scan_max_depth: None,
        };

        let output = run(
            &config,
            format!(
                "touch '{}' && ! touch '{}'",
                link.join("created.txt").display(),
                link.join("readonly/blocked.txt").display(),
            ),
            TerminalCommandPermissions::default(),
        )
        .await;
        assert!(
            output.status.success(),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(real.join("created.txt").exists());
        assert!(!real.join("readonly/blocked.txt").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn custom_profile_enforces_multi_root_carveouts_and_deny_globs() {
        let root = std::env::temp_dir().join(format!(
            "chatos-command-custom-profile-test-{}",
            uuid::Uuid::new_v4()
        ));
        let mut config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
        let shared = root.join("shared");
        std::fs::create_dir_all(shared.join("readonly")).expect("shared readonly");
        std::fs::create_dir_all(config.workspace.join("readonly")).expect("workspace readonly");
        std::fs::write(config.workspace.join("workspace.env"), "secret").expect("workspace env");
        std::fs::write(shared.join("shared.env"), "secret").expect("shared env");
        let shared = shared.canonicalize().expect("shared");
        config.runtime_workspace_roots = vec![config.workspace.clone(), shared.clone()];
        config.base_file_system = FileSystemPermissionPolicy::Restricted {
            entries: vec![
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::Root,
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::ProjectRoots { subpath: None },
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::ProjectRoots {
                            subpath: Some("readonly".to_string()),
                        },
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Deny,
                    path: FileSystemPath::GlobPattern {
                        pattern: "**/*.env".to_string(),
                    },
                },
            ],
            glob_scan_max_depth: Some(4),
        };

        let output = run(
            &config,
            format!(
                "touch '{}' && touch '{}' && ! touch '{}' && ! touch '{}' && ! cat '{}' && ! cat '{}'",
                config.workspace.join("workspace.txt").display(),
                shared.join("shared.txt").display(),
                config.workspace.join("readonly/blocked.txt").display(),
                shared.join("readonly/blocked.txt").display(),
                config.workspace.join("workspace.env").display(),
                shared.join("shared.env").display(),
            ),
            TerminalCommandPermissions::default(),
        )
        .await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(config.workspace.join("workspace.txt").exists());
        assert!(shared.join("shared.txt").exists());
        assert!(!config.workspace.join("readonly/blocked.txt").exists());
        assert!(!shared.join("readonly/blocked.txt").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[tokio::test]
    async fn restricted_minimal_profile_runs_tools_without_reading_unapproved_user_paths() {
        let root = std::env::temp_dir().join(format!(
            "chatos-command-minimal-profile-test-{}",
            uuid::Uuid::new_v4()
        ));
        let mut config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
        config.temp = std::env::temp_dir()
            .canonicalize()
            .expect("canonical system temp");
        let outside = config
            .host_home
            .as_deref()
            .expect("host home")
            .join(format!(
                ".chatos-command-minimal-outside-{}",
                uuid::Uuid::new_v4()
            ));
        std::fs::create_dir_all(outside.as_path()).expect("outside");
        std::fs::write(outside.join("secret.txt"), "secret").expect("outside secret");
        config.base_file_system = FileSystemPermissionPolicy::Restricted {
            entries: vec![
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::Minimal,
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::ProjectRoots { subpath: None },
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::ProjectRoots {
                            subpath: Some(".git".to_string()),
                        },
                    },
                },
            ],
            glob_scan_max_depth: None,
        };

        let materialized = materialize_permissions(&config, config.workspace.as_path(), None)
            .expect("materialize minimal profile");
        assert!(!materialized.full_disk_read);
        assert!(materialized.include_platform_defaults);

        let output = run(
            &config,
            format!(
                "echo \"TMPDIR=$TMPDIR\" && test -x /bin/sh && touch '{}' && test -f \"$(mktemp)\" && ! cat '{}'",
                config.workspace.join("created.txt").display(),
                outside.join("secret.txt").display(),
            ),
            TerminalCommandPermissions::default(),
        )
        .await;
        assert!(
            output.status.success(),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(config.workspace.join("created.txt").exists());
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn restricted_read_workspace_below_private_tmp_shadow_remains_visible() {
        let root = std::env::temp_dir().join(format!(
            "chatos-command-restricted-read-tmp-test-{}",
            uuid::Uuid::new_v4()
        ));
        let mut config = config(root.as_path(), PermissionProfileId::ReadOnly);
        std::fs::write(config.workspace.join("readable.txt"), "visible").expect("workspace file");
        config.base_file_system = FileSystemPermissionPolicy::Restricted {
            entries: vec![
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::Minimal,
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::ProjectRoots { subpath: None },
                    },
                },
            ],
            glob_scan_max_depth: None,
        };

        let output = run(
            &config,
            format!(
                "test \"$(cat '{}')\" = visible && ! touch '{}'",
                config.workspace.join("readable.txt").display(),
                config.workspace.join("blocked.txt").display(),
            ),
            TerminalCommandPermissions::default(),
        )
        .await;
        assert!(
            output.status.success(),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!config.workspace.join("blocked.txt").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn command_write_overlay_cannot_override_a_base_deny() {
        let root = std::env::temp_dir().join(format!(
            "chatos-command-base-deny-test-{}",
            uuid::Uuid::new_v4()
        ));
        let mut config = config(root.as_path(), PermissionProfileId::ReadOnly);
        let secret = config.workspace.join("secret.txt");
        std::fs::write(secret.as_path(), "secret").expect("secret");
        config.base_file_system = FileSystemPermissionPolicy::Restricted {
            entries: vec![
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::Root,
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Deny,
                    path: FileSystemPath::Path {
                        path: secret.to_string_lossy().to_string(),
                    },
                },
            ],
            glob_scan_max_depth: None,
        };
        let requested = RequestPermissionProfile {
            file_system: Some(AdditionalFileSystemPermissions {
                entries: Some(vec![
                    FileSystemSandboxEntry {
                        access: FileSystemAccessMode::Write,
                        path: FileSystemPath::Path {
                            path: config.workspace.to_string_lossy().to_string(),
                        },
                    },
                    FileSystemSandboxEntry {
                        access: FileSystemAccessMode::Write,
                        path: FileSystemPath::Path {
                            path: secret.to_string_lossy().to_string(),
                        },
                    },
                ]),
                ..Default::default()
            }),
            network: None,
        };

        let output = run(
            &config,
            format!(
                "touch '{}' && ! cat '{}' && ! rm '{}'",
                config.workspace.join("ordinary.txt").display(),
                secret.display(),
                secret.display(),
            ),
            TerminalCommandPermissions {
                requested: Some(requested.clone()),
                granted: Some(requested.into()),
            },
        )
        .await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(config.workspace.join("ordinary.txt").exists());
        assert!(secret.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_tool_policy_does_not_infer_workspace_write_from_an_external_write_root() {
        let root = std::env::temp_dir().join(format!(
            "chatos-file-tool-external-write-test-{}",
            uuid::Uuid::new_v4()
        ));
        let mut config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside).expect("outside");
        config.base_file_system = FileSystemPermissionPolicy::Restricted {
            entries: vec![
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::Root,
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Path {
                        path: outside.to_string_lossy().to_string(),
                    },
                },
            ],
            glob_scan_max_depth: None,
        };

        let policy = config.file_tool_access_policy().expect("file tool policy");
        assert!(!policy.workspace_writes_allowed());
        assert!(policy
            .authorize_write(config.workspace.join("blocked.txt").as_path())
            .is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn file_tool_policy_enforces_read_carveouts_and_deny_globs() {
        let root = std::env::temp_dir().join(format!(
            "chatos-file-tool-carveout-test-{}",
            uuid::Uuid::new_v4()
        ));
        let mut config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
        let readonly = config.workspace.join("readonly");
        let secret = config.workspace.join("secret.env");
        std::fs::create_dir_all(&readonly).expect("readonly");
        std::fs::write(&secret, "secret").expect("secret");
        config.base_file_system = FileSystemPermissionPolicy::Restricted {
            entries: vec![
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::Root,
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::ProjectRoots { subpath: None },
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Path {
                        path: readonly.to_string_lossy().to_string(),
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Deny,
                    path: FileSystemPath::GlobPattern {
                        pattern: "**/*.env".to_string(),
                    },
                },
            ],
            glob_scan_max_depth: Some(3),
        };

        let policy = config.file_tool_access_policy().expect("file tool policy");
        assert!(policy.workspace_writes_allowed());
        assert!(policy
            .authorize_write(config.workspace.join("ordinary.txt").as_path())
            .is_ok());
        assert!(policy
            .authorize_write(readonly.join("blocked.txt").as_path())
            .is_err());
        assert!(policy.authorize_read(secret.as_path()).is_err());
        assert!(policy
            .authorize_recursive_read(config.workspace.as_path())
            .is_err());
        assert!(policy
            .authorize_recursive_write(config.workspace.as_path())
            .is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn restricted_network_uses_proxy_and_blocks_direct_bypass() {
        let root = std::env::temp_dir().join(format!(
            "chatos-command-network-proxy-test-{}",
            uuid::Uuid::new_v4()
        ));
        let mut config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("upstream listener");
        let upstream_port = listener.local_addr().expect("upstream address").port();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept upstream");
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request).await.expect("read request");
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nproxied",
                )
                .await
                .expect("write response");
        });
        config.network_proxy = NetworkProxyRuntime::start(
            config.state_root.as_path(),
            &NetworkRequirements {
                enabled: Some(true),
                domains: Some(BTreeMap::from([(
                    "127.0.0.1".to_string(),
                    NetworkDomainPermission::Allow,
                )])),
                enable_socks5: Some(false),
                ..Default::default()
            },
        )
        .await
        .expect("start network proxy");

        let proxied = run(
            &config,
            format!(
                "/usr/bin/curl --silent --show-error --max-time 3 http://127.0.0.1:{upstream_port}/"
            ),
            TerminalCommandPermissions::default(),
        )
        .await;
        assert!(
            proxied.status.success(),
            "{}",
            String::from_utf8_lossy(&proxied.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&proxied.stdout), "proxied");

        let bypass = run(
            &config,
            format!(
                "/usr/bin/curl --noproxy '*' --silent --show-error --max-time 1 http://127.0.0.1:{upstream_port}/"
            ),
            TerminalCommandPermissions::default(),
        )
        .await;
        assert!(!bypass.status.success(), "direct network bypass succeeded");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn restricted_https_uses_managed_ca_and_enforces_inner_method_policy() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let root = std::env::temp_dir().join(format!(
            "chatos-command-https-proxy-test-{}",
            uuid::Uuid::new_v4()
        ));
        let mut config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);

        let mut ca_params = CertificateParams::default();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let mut distinguished_name = DistinguishedName::new();
        distinguished_name.push(DnType::CommonName, "ChatOS command sandbox HTTPS test CA");
        ca_params.distinguished_name = distinguished_name;
        let ca_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("CA key");
        let ca_certificate = ca_params.self_signed(&ca_key).expect("CA certificate");
        let ca_path = config.state_root.join("upstream-test-ca.pem");
        std::fs::write(ca_path.as_path(), ca_certificate.pem()).expect("write test CA");
        let issuer = Issuer::new(ca_params, ca_key);

        let mut leaf_params = CertificateParams::new(Vec::new()).expect("leaf params");
        leaf_params
            .subject_alt_names
            .push(SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        let leaf_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("leaf key");
        let leaf_certificate = leaf_params
            .signed_by(&leaf_key, &issuer)
            .expect("leaf certificate");
        let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
        let mut tls_config = TlsServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![leaf_certificate.der().clone()], private_key)
            .expect("TLS server config");
        tls_config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let acceptor = TlsAcceptor::from(Arc::new(tls_config));

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("HTTPS upstream listener");
        let upstream_port = listener.local_addr().expect("upstream address").port();
        let upstream_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept HTTPS GET");
            let mut stream = acceptor.accept(stream).await.expect("accept upstream TLS");
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).await.expect("read HTTPS GET");
            assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /"));
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\nhttps-proxied",
                )
                .await
                .expect("write HTTPS response");
            stream.shutdown().await.expect("shutdown upstream TLS");

            let (stream, _) = listener.accept().await.expect("accept HTTPS POST route");
            assert!(
                acceptor.accept(stream).await.is_err(),
                "blocked POST must not start upstream TLS"
            );
        });

        let previous_ssl_cert_file = std::env::var_os("SSL_CERT_FILE");
        std::env::set_var("SSL_CERT_FILE", ca_path.as_os_str());
        config.network_proxy = NetworkProxyRuntime::start(
            config.state_root.as_path(),
            &NetworkRequirements {
                enabled: Some(true),
                mode: Some(NetworkProxyMode::Limited),
                domains: Some(BTreeMap::from([(
                    "127.0.0.1".to_string(),
                    NetworkDomainPermission::Allow,
                )])),
                enable_socks5: Some(false),
                ..Default::default()
            },
        )
        .await
        .expect("start HTTPS network proxy");
        if let Some(value) = previous_ssl_cert_file {
            std::env::set_var("SSL_CERT_FILE", value);
        } else {
            std::env::remove_var("SSL_CERT_FILE");
        }

        let get = run(
            &config,
            format!(
                "/usr/bin/curl --fail --silent --show-error --max-time 5 https://127.0.0.1:{upstream_port}/"
            ),
            TerminalCommandPermissions::default(),
        )
        .await;
        assert!(
            get.status.success(),
            "{}",
            String::from_utf8_lossy(&get.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&get.stdout), "https-proxied");

        let post = run(
            &config,
            format!(
                "/usr/bin/curl --fail --silent --show-error --max-time 5 --request POST --data '' https://127.0.0.1:{upstream_port}/"
            ),
            TerminalCommandPermissions::default(),
        )
        .await;
        assert!(
            !post.status.success(),
            "limited HTTPS POST unexpectedly succeeded"
        );
        assert!(
            String::from_utf8_lossy(&post.stderr).contains("403"),
            "{}",
            String::from_utf8_lossy(&post.stderr)
        );
        upstream_task.await.expect("upstream task");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn untrusted_grant_without_request_is_rejected() {
        let root = std::env::temp_dir().join(format!(
            "chatos-command-sandbox-grant-test-{}",
            uuid::Uuid::new_v4()
        ));
        let config = config(root.as_path(), PermissionProfileId::WorkspaceWrite);
        let err = match PreparedSandboxCommand::new(
            &config,
            "/bin/sh",
            "true",
            config.workspace.as_path(),
            &TerminalCommandPermissions {
                requested: None,
                granted: Some(GrantedPermissionProfile {
                    file_system: None,
                    network: Some(AdditionalNetworkPermissions {
                        enabled: Some(true),
                    }),
                }),
            },
        ) {
            Ok(_) => panic!("grant injection must fail"),
            Err(err) => err,
        };
        assert!(err.contains("no matching request"));
        let _ = std::fs::remove_dir_all(root);
    }
}
