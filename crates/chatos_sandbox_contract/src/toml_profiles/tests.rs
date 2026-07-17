// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::{FileSystemPermissionPolicy, PermissionProfileId};

#[test]
fn parses_codex_style_independent_minimal_profile() {
    let document = parse_codex_permission_profile_toml(
        r#"
model = "gpt-5"
default_permissions = "project-edit"

[allowed_permission_profiles]
":read-only" = true
"project-edit" = true

[permissions.project-edit]
description = "Minimal project editing"

[permissions.project-edit.workspace_roots]
"~/code/shared" = true

[permissions.project-edit.filesystem]
glob_scan_max_depth = 4
":minimal" = "read"

[permissions.project-edit.filesystem.":workspace_roots"]
"." = "write"
".devcontainer" = "read"
"**/*.env" = "deny"

[permissions.project-edit.network]
enabled = true
mode = "limited"

[permissions.project-edit.network.domains]
"api.example.com" = "allow"
"blocked.example.com" = "deny"
"#,
    )
    .expect("parse Codex permission TOML");

    assert_eq!(
        document.default_permissions.as_deref(),
        Some("project-edit")
    );
    let resolved = document
        .configuration
        .resolve(
            "project-edit",
            vec!["/workspace".to_string()],
            None,
            PermissionProfileProvenance::User,
        )
        .expect("resolve parsed profile");
    assert_eq!(
        resolved.permission_profile_id,
        PermissionProfileId::WorkspaceWrite
    );
    assert_eq!(resolved.effective_permissions.active_profile.extends, None);
    let FileSystemPermissionPolicy::Restricted { entries, .. } =
        resolved.effective_permissions.file_system
    else {
        panic!("filesystem must remain restricted");
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
fn parses_parent_first_overrides_and_project_roots_alias() {
    let document = parse_codex_permission_profile_toml(
        r#"
default_permissions = "child"

[permissions.base.filesystem]
":root" = "read"
"/tmp/shared" = "read"

[permissions.base.filesystem.":project_roots"]
"docs" = "read"

[permissions.child]
extends = "base"

[permissions.child.filesystem]
"/tmp/shared" = "write"

[permissions.child.filesystem.":project_roots"]
"docs" = "write"
"#,
    )
    .expect("parse inherited profile");
    let resolved = document
        .configuration
        .resolve(
            "child",
            vec!["/workspace".to_string()],
            None,
            PermissionProfileProvenance::User,
        )
        .expect("resolve inherited profile");
    let FileSystemPermissionPolicy::Restricted { entries, .. } =
        resolved.effective_permissions.file_system
    else {
        panic!("restricted filesystem");
    };
    assert!(entries.iter().any(|entry| {
        entry.access == FileSystemAccessMode::Write
            && entry.path
                == FileSystemPath::Path {
                    path: "/tmp/shared".to_string(),
                }
    }));
}

#[test]
fn rejects_unsupported_network_keys_and_parent_traversal() {
    let network_error = parse_codex_permission_profile_toml(
        r#"
[permissions.net.network]
enabled = true
proxy_url = "http://127.0.0.1:3128"
"#,
    )
    .expect_err("unsupported network keys must fail closed");
    assert!(network_error.contains("unsupported network key"));

    let traversal_error = parse_codex_permission_profile_toml(
        r#"
[permissions.escape.filesystem.":workspace_roots"]
"../outside" = "write"
"#,
    )
    .expect_err("parent traversal must fail closed");
    assert!(traversal_error.contains("parent traversal"));
}

#[test]
fn higher_precedence_layers_override_by_key_without_dropping_other_rules() {
    let lower = parse_codex_permission_profile_toml(
        r#"
default_permissions = "dev"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
"dev" = true

[permissions.dev]
description = "User profile"

[permissions.dev.filesystem]
":minimal" = "read"
"/tmp/shared" = "read"

[permissions.dev.filesystem.":workspace_roots"]
"." = "write"
"docs" = "read"

[permissions.dev.network]
enabled = true

[permissions.dev.network.domains]
"user.example.com" = "allow"
"shared.example.com" = "allow"
"#,
    )
    .expect("lower layer");
    let higher = parse_codex_permission_profile_toml(
        r#"
[allowed_permission_profiles]
":workspace" = false

[permissions.dev]
description = "Project profile"

[permissions.dev.filesystem]
"/tmp/shared" = "deny"

[permissions.dev.filesystem.":workspace_roots"]
"docs" = "write"

[permissions.dev.network.domains]
"project.example.com" = "allow"
"shared.example.com" = "deny"
"#,
    )
    .expect("higher layer");

    let merged = merge_codex_permission_profile_documents(lower, higher)
        .expect("merge permission profile layers");
    assert_eq!(merged.default_permissions.as_deref(), Some("dev"));
    assert_eq!(
        merged
            .configuration
            .allowed_permission_profiles
            .as_ref()
            .and_then(|allowed| allowed.get(":workspace")),
        Some(&false)
    );
    let profile = merged
        .configuration
        .profiles
        .get("dev")
        .expect("merged dev profile");
    assert_eq!(profile.description.as_deref(), Some("Project profile"));
    let entries = profile
        .file_system
        .as_ref()
        .expect("merged filesystem")
        .normalized_entries();
    assert!(entries.iter().any(|entry| {
        entry.access == FileSystemAccessMode::Read
            && matches!(
                entry.path,
                FileSystemPath::Special {
                    value: FileSystemSpecialPath::Minimal
                }
            )
    }));
    assert!(entries.iter().any(|entry| {
        entry.access == FileSystemAccessMode::Deny
            && entry.path
                == (FileSystemPath::Path {
                    path: "/tmp/shared".to_string(),
                })
    }));
    let domains = profile
        .network
        .as_ref()
        .and_then(|network| network.domains.as_ref())
        .expect("merged domains");
    assert_eq!(
        domains.get("shared.example.com"),
        Some(&NetworkDomainPermission::Deny)
    );
    assert!(domains.contains_key("user.example.com"));
    assert!(domains.contains_key("project.example.com"));
}

#[test]
fn managed_requirements_reject_unrelated_top_level_keys() {
    assert!(parse_codex_permission_profile_toml("model = \"gpt-test\"").is_ok());
    assert!(parse_managed_requirements_toml("model = \"gpt-test\"").is_err());
    assert!(parse_managed_requirements_toml("default_permissions = \":read-only\"").is_ok());
}
