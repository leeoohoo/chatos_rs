// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use serde_json::json;

#[test]
fn codex_permission_request_shape_round_trips() {
    let request: RequestPermissionProfile = serde_json::from_value(json!({
        "fileSystem": {
            "entries": [
                {
                    "access": "write",
                    "path": { "type": "path", "path": "/tmp/output" }
                },
                {
                    "access": "deny",
                    "path": {
                        "type": "special",
                        "value": { "kind": "project_roots", "subpath": ".git" }
                    }
                }
            ],
            "globScanMaxDepth": 3
        },
        "network": { "enabled": true }
    }))
    .expect("deserialize request");

    request.validate().expect("valid request");
    assert_eq!(
        serde_json::to_value(request).expect("serialize request"),
        json!({
            "fileSystem": {
                "entries": [
                    {
                        "access": "write",
                        "path": { "type": "path", "path": "/tmp/output" }
                    },
                    {
                        "access": "deny",
                        "path": {
                            "type": "special",
                            "value": { "kind": "project_roots", "subpath": ".git" }
                        }
                    }
                ],
                "globScanMaxDepth": 3
            },
            "network": { "enabled": true }
        })
    );
}

#[test]
fn empty_permission_request_fails_closed() {
    let request = RequestPermissionProfile::default();
    assert!(request.validate().is_err());
}

#[test]
fn grant_cannot_exceed_request() {
    let request = RequestPermissionProfile {
        file_system: Some(AdditionalFileSystemPermissions {
            entries: Some(vec![FileSystemSandboxEntry {
                access: FileSystemAccessMode::Read,
                path: FileSystemPath::Path {
                    path: "/tmp/input".to_string(),
                },
            }]),
            ..AdditionalFileSystemPermissions::default()
        }),
        network: None,
    };
    let exact = GrantedPermissionProfile {
        file_system: request.file_system.clone(),
        network: None,
    };
    assert!(request.allows_grant(&exact));

    let broader = GrantedPermissionProfile {
        file_system: Some(AdditionalFileSystemPermissions {
            entries: Some(vec![FileSystemSandboxEntry {
                access: FileSystemAccessMode::Write,
                path: FileSystemPath::Path {
                    path: "/tmp/input".to_string(),
                },
            }]),
            ..AdditionalFileSystemPermissions::default()
        }),
        network: Some(AdditionalNetworkPermissions {
            enabled: Some(true),
        }),
    };
    assert!(!request.allows_grant(&broader));
}

#[test]
fn grant_must_retain_requested_deny_carveouts() {
    let request = RequestPermissionProfile {
        file_system: Some(AdditionalFileSystemPermissions {
            entries: Some(vec![
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Write,
                    path: FileSystemPath::Path {
                        path: "/tmp/output".to_string(),
                    },
                },
                FileSystemSandboxEntry {
                    access: FileSystemAccessMode::Deny,
                    path: FileSystemPath::GlobPattern {
                        pattern: "/tmp/output/**/*.env".to_string(),
                    },
                },
            ]),
            glob_scan_max_depth: Some(3),
            ..Default::default()
        }),
        network: None,
    };
    let missing_deny = GrantedPermissionProfile {
        file_system: Some(AdditionalFileSystemPermissions {
            entries: Some(vec![FileSystemSandboxEntry {
                access: FileSystemAccessMode::Write,
                path: FileSystemPath::Path {
                    path: "/tmp/output".to_string(),
                },
            }]),
            ..Default::default()
        }),
        network: None,
    };
    assert!(!request.allows_grant(&missing_deny));

    let shallower_deny = GrantedPermissionProfile {
        file_system: Some(AdditionalFileSystemPermissions {
            entries: request
                .file_system
                .as_ref()
                .and_then(|file_system| file_system.entries.clone()),
            glob_scan_max_depth: Some(1),
            ..Default::default()
        }),
        network: None,
    };
    assert!(!request.allows_grant(&shallower_deny));
    assert!(request.allows_grant(&request.clone().into()));
}

#[test]
fn command_decisions_match_codex_union_shape() {
    let session: CommandExecutionApprovalDecision =
        serde_json::from_value(json!("acceptForSession")).expect("session decision");
    assert_eq!(
        session,
        CommandExecutionApprovalDecision::Simple(
            SimpleCommandExecutionApprovalDecision::AcceptForSession
        )
    );

    let amendment: CommandExecutionApprovalDecision = serde_json::from_value(json!({
        "acceptWithExecpolicyAmendment": {
            "execpolicy_amendment": ["git", "status"]
        }
    }))
    .expect("amendment decision");
    assert!(matches!(
        amendment,
        CommandExecutionApprovalDecision::AcceptWithExecpolicyAmendment { .. }
    ));
}

#[test]
fn workspace_legacy_policy_exposes_active_profile_and_protected_paths() {
    let policy = EffectiveSandboxPolicy {
        permission_profile_id: PermissionProfileId::WorkspaceWrite,
        additional_writable_roots: vec!["/tmp/shared".to_string()],
        ..EffectiveSandboxPolicy::default()
    };
    let snapshot = legacy_policy_permission_snapshot(&policy, vec!["/workspace".to_string()]);

    assert_eq!(snapshot.active_profile.id, ":workspace");
    assert_eq!(snapshot.provenance, PermissionProfileProvenance::BuiltIn);
    let FileSystemPermissionPolicy::Restricted { entries, .. } = snapshot.file_system else {
        panic!("workspace profile must stay restricted");
    };
    assert!(entries.iter().any(|entry| {
        entry.access == FileSystemAccessMode::Read
            && entry.path
                == FileSystemPath::Special {
                    value: FileSystemSpecialPath::ProjectRoots {
                        subpath: Some(".git".to_string()),
                    },
                }
    }));
    assert_eq!(snapshot.runtime_workspace_roots, vec!["/workspace"]);
}
