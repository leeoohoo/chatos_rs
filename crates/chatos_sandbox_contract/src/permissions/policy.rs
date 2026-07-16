// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub fn legacy_policy_permission_snapshot(
    policy: &EffectiveSandboxPolicy,
    runtime_workspace_roots: Vec<String>,
) -> EffectivePermissionSnapshot {
    let active_profile = ActivePermissionProfile {
        id: match policy.permission_profile_id {
            PermissionProfileId::ReadOnly => ":read-only",
            PermissionProfileId::WorkspaceWrite => ":workspace",
            PermissionProfileId::FullAccess => ":danger-full-access",
        }
        .to_string(),
        extends: None,
    };
    let (file_system, network) = match policy.permission_profile_id {
        PermissionProfileId::ReadOnly => (
            FileSystemPermissionPolicy::Restricted {
                entries: vec![FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Read,
                    path: FileSystemPath::Special {
                        value: FileSystemSpecialPath::Root,
                    },
                }],
                glob_scan_max_depth: None,
            },
            NetworkPermissionPolicy::Restricted {
                requirements: NetworkRequirements {
                    enabled: Some(false),
                    ..NetworkRequirements::default()
                },
            },
        ),
        PermissionProfileId::WorkspaceWrite => {
            let mut entries = vec![
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
            ];
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
                policy
                    .additional_writable_roots
                    .iter()
                    .cloned()
                    .map(|path| FileSystemSandboxEntry {
                        access: FileSystemAccessMode::Write,
                        path: FileSystemPath::Path { path },
                    }),
            );
            (
                FileSystemPermissionPolicy::Restricted {
                    entries,
                    glob_scan_max_depth: None,
                },
                NetworkPermissionPolicy::Restricted {
                    requirements: NetworkRequirements {
                        enabled: Some(false),
                        ..NetworkRequirements::default()
                    },
                },
            )
        }
        PermissionProfileId::FullAccess => (
            FileSystemPermissionPolicy::Unrestricted,
            NetworkPermissionPolicy::Unrestricted,
        ),
    };
    EffectivePermissionSnapshot {
        active_profile,
        provenance: PermissionProfileProvenance::BuiltIn,
        file_system,
        network,
        runtime_workspace_roots,
        policy_revision: policy.policy_revision.clone(),
    }
}

pub(super) fn validate_filesystem_path(path: &FileSystemPath) -> Result<(), String> {
    match path {
        FileSystemPath::Path { path } => validate_non_empty_path(path),
        FileSystemPath::GlobPattern { pattern } => validate_non_empty_path(pattern),
        FileSystemPath::Special { value } => match value {
            FileSystemSpecialPath::ProjectRoots {
                subpath: Some(path),
            }
            | FileSystemSpecialPath::Unknown {
                path,
                subpath: None,
            } => validate_non_empty_path(path),
            FileSystemSpecialPath::Unknown {
                path,
                subpath: Some(subpath),
            } => {
                validate_non_empty_path(path)?;
                validate_non_empty_path(subpath)
            }
            _ => Ok(()),
        },
    }
}

pub(super) fn validate_non_empty_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("filesystem permission path must not be empty".to_string());
    }
    if path.contains('\0') {
        return Err("filesystem permission path must not contain NUL".to_string());
    }
    Ok(())
}
