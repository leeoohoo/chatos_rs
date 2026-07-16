// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::permissions::*;
use super::*;

#[cfg(target_os = "macos")]
pub(in crate::command_sandbox) fn prepare_macos_command(
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
pub(super) fn fail_if_protected_path_crosses_writable_symlink(
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
pub(super) fn append_macos_path_rule(
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
