// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::{FileSystemSandboxEntry, NetworkDomainPermission, NetworkProxyMode};

fn project_profile() -> CustomPermissionProfile {
    CustomPermissionProfile {
        description: Some("Project edit with secret carve-outs".to_string()),
        extends: Some(":workspace".to_string()),
        workspace_roots: BTreeMap::from([("~/code/shared".to_string(), true)]),
        file_system: Some(AdditionalFileSystemPermissions {
            entries: Some(vec![FileSystemSandboxEntry {
                access: FileSystemAccessMode::Deny,
                path: FileSystemPath::GlobPattern {
                    pattern: "**/*.env".to_string(),
                },
            }]),
            glob_scan_max_depth: Some(4),
            ..Default::default()
        }),
        network: Some(NetworkRequirements {
            enabled: Some(true),
            mode: Some(NetworkProxyMode::Limited),
            domains: Some(BTreeMap::from([(
                "api.example.com".to_string(),
                NetworkDomainPermission::Allow,
            )])),
            ..Default::default()
        }),
    }
}

#[test]
fn resolves_custom_profile_extending_workspace() {
    let config = PermissionProfileConfiguration {
        profiles: BTreeMap::from([("project-edit".to_string(), project_profile())]),
        allowed_permission_profiles: Some(BTreeMap::from([
            (":read-only".to_string(), true),
            ("project-edit".to_string(), true),
        ])),
    };
    config.validate().expect("valid profile configuration");
    let resolved = config
        .resolve(
            "project-edit",
            vec!["/workspace".to_string()],
            Some("revision".to_string()),
            PermissionProfileProvenance::Managed,
        )
        .expect("resolve profile");

    assert_eq!(
        resolved.permission_profile_id,
        PermissionProfileId::WorkspaceWrite
    );
    assert_eq!(
        resolved.effective_permissions.active_profile.id,
        "project-edit"
    );
    assert_eq!(
        resolved
            .effective_permissions
            .active_profile
            .extends
            .as_deref(),
        Some(":workspace")
    );
    assert_eq!(
        resolved.effective_permissions.provenance,
        PermissionProfileProvenance::Managed
    );
    assert!(resolved
        .effective_permissions
        .runtime_workspace_roots
        .contains(&"~/code/shared".to_string()));
    let NetworkPermissionPolicy::Restricted { requirements } =
        resolved.effective_permissions.network
    else {
        panic!("network must stay restricted");
    };
    assert_eq!(requirements.enabled, Some(true));
    assert_eq!(requirements.mode, Some(NetworkProxyMode::Limited));
}

#[test]
fn resolves_independent_minimal_profile_without_inheriting_root_read() {
    let config = PermissionProfileConfiguration {
        profiles: BTreeMap::from([(
            "minimal-project".to_string(),
            CustomPermissionProfile {
                description: Some("Minimal runtime plus workspace".to_string()),
                extends: None,
                file_system: Some(AdditionalFileSystemPermissions {
                    entries: Some(vec![
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
                    ]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )]),
        ..Default::default()
    };

    let resolved = config
        .resolve(
            "minimal-project",
            vec!["/workspace".to_string()],
            None,
            PermissionProfileProvenance::User,
        )
        .expect("resolve independent minimal profile");
    assert_eq!(resolved.effective_permissions.active_profile.extends, None);
    let FileSystemPermissionPolicy::Restricted { entries, .. } =
        resolved.effective_permissions.file_system
    else {
        panic!("minimal profile must stay restricted");
    };
    assert!(entries.iter().any(|entry| {
        entry.access == FileSystemAccessMode::Read
            && matches!(
                entry.path,
                FileSystemPath::Special {
                    value: FileSystemSpecialPath::Minimal
                }
            )
    }));
    assert!(!entries.iter().any(|entry| {
        matches!(
            entry.path,
            FileSystemPath::Special {
                value: FileSystemSpecialPath::Root
            }
        )
    }));
}

#[test]
fn rejects_cycles_reserved_names_and_unsafe_bases() {
    let cycle = PermissionProfileConfiguration {
        profiles: BTreeMap::from([
            (
                "a".to_string(),
                CustomPermissionProfile {
                    extends: Some("b".to_string()),
                    ..Default::default()
                },
            ),
            (
                "b".to_string(),
                CustomPermissionProfile {
                    extends: Some("a".to_string()),
                    ..Default::default()
                },
            ),
        ]),
        ..Default::default()
    };
    assert!(cycle.validate().unwrap_err().contains("cycle"));

    let danger = PermissionProfileConfiguration {
        profiles: BTreeMap::from([(
            "unsafe".to_string(),
            CustomPermissionProfile {
                extends: Some(":danger-full-access".to_string()),
                ..Default::default()
            },
        )]),
        ..Default::default()
    };
    assert!(danger.validate().is_err());

    let reserved = PermissionProfileConfiguration {
        profiles: BTreeMap::from([(
            "filesystem".to_string(),
            CustomPermissionProfile {
                extends: Some(":read-only".to_string()),
                ..Default::default()
            },
        )]),
        ..Default::default()
    };
    assert!(reserved.validate().is_err());
}

#[test]
fn allowlist_is_complete_for_custom_and_future_profiles() {
    let config = PermissionProfileConfiguration {
        profiles: BTreeMap::from([("project-edit".to_string(), project_profile())]),
        allowed_permission_profiles: Some(BTreeMap::from([("project-edit".to_string(), true)])),
    };
    assert!(config.profile_allowed("project-edit"));
    assert!(!config.profile_allowed(":workspace"));
    assert!(!config.profile_allowed("future-profile"));
}
