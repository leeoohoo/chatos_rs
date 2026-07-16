// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::permissions::*;
use super::*;

#[cfg(target_os = "linux")]
pub(in crate::command_sandbox) fn prepare_linux_command(
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
pub(super) fn append_linux_restricted_read_mounts(
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
pub(super) fn append_linux_file_system_mounts(
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
pub(super) fn append_linux_read_only_mount(
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
pub(super) fn linux_wrapper_executable() -> Result<PathBuf, String> {
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
pub(super) fn append_linux_deny_mask(
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
pub(super) fn append_linux_empty_file_mount(command: &mut Command, path: &Path, permissions: &str) {
    command
        .arg("--perms")
        .arg(permissions)
        .arg("--ro-bind-data")
        .arg("3")
        .arg(path);
}

#[cfg(target_os = "linux")]
pub(super) fn append_linux_mount_target_parent_dirs(
    command: &mut Command,
    target: &Path,
    anchor: &Path,
) {
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
