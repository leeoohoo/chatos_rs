// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use chatos_sandbox_contract::{
    AdditionalFileSystemPermissions, ApprovalPolicy, CustomPermissionProfile, FileSystemAccessMode,
    FileSystemPath, FileSystemSandboxEntry, FileSystemSpecialPath, NetworkProxyMode,
};

fn update_request() -> UpdateSandboxSettingsRequest {
    UpdateSandboxSettingsRequest {
        enabled: None,
        default_backend: None,
        default_permission_profile_id: None,
        default_permission_profile_name: None,
        permission_profiles: None,
        permission_profiles_toml: None,
        default_approval_policy: None,
        default_approval_reviewer: None,
        default_network_requirements: None,
        allowed_permission_profiles: None,
        risk_acknowledged: false,
    }
}

#[test]
fn sandbox_settings_rejects_elevated_permission_without_risk_ack() {
    let mut req = update_request();
    req.default_permission_profile_id = Some(PermissionProfileId::FullAccess);
    let state = crate::sandbox::types::LocalSandboxState::default();

    let err = validate_sandbox_settings_update(&req, &state)
        .expect_err("full access requires acknowledgement");

    assert_eq!(err.message(), "switching sandbox permission profile to full access requires explicit risk acknowledgement");
}

#[test]
fn sandbox_settings_treats_never_as_fail_closed_but_auto_review_as_elevated() {
    let state = crate::sandbox::types::LocalSandboxState::default();

    let mut never = update_request();
    never.default_approval_policy = Some(ApprovalPolicy::Never);
    validate_sandbox_settings_update(&never, &state)
        .expect("never denies escalation instead of auto-approving it");

    let mut auto = update_request();
    auto.default_approval_reviewer = Some(ApprovalReviewer::AutoReview);
    assert!(validate_sandbox_settings_update(&auto, &state).is_err());
}

#[test]
fn restricted_network_requires_native_backend_and_risk_acknowledgement() {
    let state = crate::sandbox::types::LocalSandboxState {
        default_backend: SandboxBackendKind::LocalProcess,
        ..Default::default()
    };
    let mut req = update_request();
    req.default_network_requirements = Some(NetworkRequirements {
        enabled: Some(true),
        domains: Some(std::collections::BTreeMap::from([(
            "api.openai.com".to_string(),
            NetworkDomainPermission::Allow,
        )])),
        ..Default::default()
    });
    assert!(validate_sandbox_settings_update(&req, &state).is_err());

    req.risk_acknowledged = true;
    validate_sandbox_settings_update(&req, &state)
        .expect("native proxy network with acknowledgement");

    req.default_backend = Some(SandboxBackendKind::Docker);
    let err = validate_sandbox_settings_update(&req, &state)
        .expect_err("Docker cannot enforce restricted egress");
    assert_eq!(
        err.message(),
        "restricted domain networking requires the native local-process sandbox backend"
    );
}

#[test]
fn sandbox_settings_allows_elevated_update_with_risk_ack() {
    let mut req = update_request();
    req.default_permission_profile_id = Some(PermissionProfileId::FullAccess);
    req.default_approval_policy = Some(ApprovalPolicy::Never);
    req.default_approval_reviewer = Some(ApprovalReviewer::AutoReview);
    req.risk_acknowledged = true;
    let state = crate::sandbox::types::LocalSandboxState::default();

    validate_sandbox_settings_update(&req, &state).expect("risk acknowledged");
}

#[test]
fn permission_profile_allowlist_is_complete_and_enforced() {
    let state = crate::sandbox::types::LocalSandboxState::default();
    let mut req = update_request();
    req.default_permission_profile_id = Some(PermissionProfileId::ReadOnly);
    req.allowed_permission_profiles = Some(std::collections::BTreeMap::from([
        (":read-only".to_string(), true),
        (":workspace".to_string(), false),
    ]));
    validate_sandbox_settings_update(&req, &state).expect("read-only allowed");

    req.default_permission_profile_id = Some(PermissionProfileId::WorkspaceWrite);
    let err = validate_sandbox_settings_update(&req, &state)
        .expect_err("workspace profile must be rejected");
    assert!(err.message().contains("not enabled"));

    req.allowed_permission_profiles = Some(std::collections::BTreeMap::from([(
        "custom-profile".to_string(),
        true,
    )]));
    assert!(validate_sandbox_settings_update(&req, &state).is_err());
}

#[test]
fn managed_allowlist_cannot_be_bypassed_by_api_selection() {
    let mut state = crate::sandbox::types::LocalSandboxState::default();
    state.runtime_permission_profile_layers =
        crate::sandbox::permission_layers::RuntimePermissionProfileLayers::for_tests(
            None,
            None,
            Some(
                parse_codex_permission_profile_toml(
                    r#"
default_permissions = ":read-only"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
"#,
                )
                .expect("parse managed requirements"),
            ),
        );
    let mut req = update_request();
    req.default_permission_profile_id = Some(PermissionProfileId::FullAccess);
    req.risk_acknowledged = true;

    let error = validate_sandbox_settings_update(&req, &state)
        .expect_err("managed allowlist must reject full access");

    assert!(error.message().contains("not enabled"));
}

#[test]
fn api_cannot_shadow_a_managed_custom_profile() {
    let mut state = crate::sandbox::types::LocalSandboxState::default();
    state.runtime_permission_profile_layers =
        crate::sandbox::permission_layers::RuntimePermissionProfileLayers::for_tests(
            None,
            None,
            Some(
                parse_codex_permission_profile_toml(
                    r#"
default_permissions = "acme-review"

[allowed_permission_profiles]
acme-review = true

[permissions.acme-review]
extends = ":read-only"
"#,
                )
                .expect("parse managed profile"),
            ),
        );
    let mut req = update_request();
    req.permission_profiles = Some(std::collections::BTreeMap::from([(
        "acme-review".to_string(),
        CustomPermissionProfile {
            extends: Some(":workspace".to_string()),
            ..Default::default()
        },
    )]));

    let error = validate_sandbox_settings_update(&req, &state)
        .expect_err("managed profile collision must fail closed");

    assert!(error.message().contains("conflicts"));
}

#[test]
fn api_cannot_widen_a_parent_inherited_by_managed_profile() {
    let mut state = crate::sandbox::types::LocalSandboxState {
        default_backend: SandboxBackendKind::LocalProcess,
        permission_profiles: std::collections::BTreeMap::from([(
            "shared-base".to_string(),
            CustomPermissionProfile {
                extends: Some(":read-only".to_string()),
                ..Default::default()
            },
        )]),
        ..Default::default()
    };
    state.runtime_permission_profile_layers =
        crate::sandbox::permission_layers::RuntimePermissionProfileLayers::for_tests(
            None,
            None,
            Some(
                parse_codex_permission_profile_toml(
                    r#"
default_permissions = "acme-managed"

[allowed_permission_profiles]
acme-managed = true

[permissions.acme-managed]
extends = "shared-base"
"#,
                )
                .expect("parse managed inherited profile"),
            ),
        );
    let mut req = update_request();
    req.permission_profiles = Some(std::collections::BTreeMap::from([(
        "shared-base".to_string(),
        CustomPermissionProfile {
            extends: Some(":workspace".to_string()),
            ..Default::default()
        },
    )]));

    let error = validate_sandbox_settings_update(&req, &state)
        .expect_err("managed profile ancestry must be immutable through API");

    assert!(error.message().contains("inherited by a managed profile"));
}

#[test]
fn custom_permission_profile_requires_native_backend_and_can_be_selected() {
    let state = crate::sandbox::types::LocalSandboxState {
        default_backend: SandboxBackendKind::LocalProcess,
        ..Default::default()
    };
    let mut req = update_request();
    req.permission_profiles = Some(std::collections::BTreeMap::from([(
        "project-edit".to_string(),
        CustomPermissionProfile {
            description: Some("Project edit".to_string()),
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
    )]));
    req.default_permission_profile_name = Some("project-edit".to_string());
    validate_sandbox_settings_update(&req, &state).expect("valid native custom profile");

    req.default_backend = Some(SandboxBackendKind::Docker);
    let err = validate_sandbox_settings_update(&req, &state)
        .expect_err("Docker cannot execute custom filesystem profiles");
    assert!(err.message().contains("native local-process"));
}

#[test]
fn codex_permission_toml_import_normalizes_into_persisted_profile_fields() {
    let state = crate::sandbox::types::LocalSandboxState {
        default_backend: SandboxBackendKind::LocalProcess,
        ..Default::default()
    };
    let mut req = update_request();
    req.permission_profiles_toml = Some(
        r#"
default_permissions = "minimal-project"

[allowed_permission_profiles]
"minimal-project" = true

[permissions.minimal-project.filesystem]
glob_scan_max_depth = 3
":minimal" = "read"

[permissions.minimal-project.filesystem.":workspace_roots"]
"." = "write"
"**/*.env" = "deny"
"#
        .to_string(),
    );
    let req = normalize_sandbox_settings_update(req).expect("normalize TOML import");
    assert_eq!(
        req.default_permission_profile_name.as_deref(),
        Some("minimal-project")
    );
    assert!(req
        .permission_profiles
        .as_ref()
        .is_some_and(|profiles| profiles.contains_key("minimal-project")));
    validate_sandbox_settings_update(&req, &state).expect("validate imported TOML profile");
}

#[test]
fn codex_permission_toml_import_rejects_ambiguous_explicit_fields() {
    let mut req = update_request();
    req.permission_profiles_toml = Some("default_permissions = \":read-only\"".to_string());
    req.default_permission_profile_id = Some(PermissionProfileId::ReadOnly);
    let error = normalize_sandbox_settings_update(req)
        .expect_err("ambiguous profile sources must fail closed");
    assert!(error.message().contains("cannot be combined"));
}

#[test]
fn custom_permission_profile_cycles_fail_closed_at_the_api_boundary() {
    let state = crate::sandbox::types::LocalSandboxState::default();
    let mut req = update_request();
    req.permission_profiles = Some(std::collections::BTreeMap::from([
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
    ]));

    let err = validate_sandbox_settings_update(&req, &state)
        .expect_err("inheritance cycle must be rejected");
    assert!(err.message().contains("cycle"));
}

#[test]
fn custom_limited_network_profile_requires_risk_acknowledgement() {
    let state = crate::sandbox::types::LocalSandboxState {
        default_backend: SandboxBackendKind::LocalProcess,
        ..Default::default()
    };
    let mut req = update_request();
    req.permission_profiles = Some(std::collections::BTreeMap::from([(
        "api-read".to_string(),
        CustomPermissionProfile {
            extends: Some(":read-only".to_string()),
            network: Some(NetworkRequirements {
                enabled: Some(true),
                mode: Some(NetworkProxyMode::Limited),
                domains: Some(std::collections::BTreeMap::from([(
                    "api.openai.com".to_string(),
                    NetworkDomainPermission::Allow,
                )])),
                ..Default::default()
            }),
            ..Default::default()
        },
    )]));
    req.default_permission_profile_name = Some("api-read".to_string());

    let err = validate_sandbox_settings_update(&req, &state)
        .expect_err("enabling custom profile network requires acknowledgement");
    assert!(err.message().contains("network access"));

    req.risk_acknowledged = true;
    validate_sandbox_settings_update(&req, &state)
        .expect("acknowledged limited custom network profile");
}

#[test]
fn sandbox_policy_revision_changes_only_for_policy_fields() {
    let state = crate::sandbox::types::LocalSandboxState::default();

    let mut enabled_only = update_request();
    enabled_only.enabled = Some(true);
    assert!(!sandbox_policy_fields_changed(&enabled_only, &state));

    let mut same_backend = update_request();
    same_backend.default_backend = Some(state.default_backend);
    assert!(!sandbox_policy_fields_changed(&same_backend, &state));

    let mut profile = update_request();
    profile.default_permission_profile_id = Some(PermissionProfileId::ReadOnly);
    assert!(sandbox_policy_fields_changed(&profile, &state));

    let mut reviewer = update_request();
    reviewer.default_approval_reviewer = Some(ApprovalReviewer::AutoReview);
    assert!(sandbox_policy_fields_changed(&reviewer, &state));
}

#[test]
fn docker_backend_capability_reports_restricted_network_support() {
    let capability = docker_backend_capability_from_status(&json!({
        "installed": true,
        "running": true,
        "version": "Docker 27"
    }));

    assert_eq!(capability.backend, SandboxBackendKind::Docker);
    assert_eq!(capability.status, SandboxBackendReadinessStatus::Ready);
    assert!(capability.selectable);
    assert!(capability.filesystem_isolation);
    assert!(capability.network_isolation);
    assert!(capability.process_tree_control);
    assert!(capability.message.contains("bridge networking"));
}
