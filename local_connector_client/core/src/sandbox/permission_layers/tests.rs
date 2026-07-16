// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chatos_sandbox_contract::{
    parse_codex_permission_profile_toml, CodexPermissionProfileDocument, CustomPermissionProfile,
    PermissionProfileId, PermissionProfileProvenance,
};
use uuid::Uuid;

use super::loading::{load_permission_document, ConfigPath};
use super::RuntimePermissionProfileLayers;

fn parse(source: &str) -> CodexPermissionProfileDocument {
    parse_codex_permission_profile_toml(source).expect("parse permission document")
}

fn test_config_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "chatos-permission-layers-{name}-{}",
        Uuid::new_v4()
    ))
}

#[test]
fn managed_allowlist_falls_back_from_disallowed_user_default() {
    let layers = RuntimePermissionProfileLayers {
        user: Some(parse("default_permissions = \":danger-full-access\"")),
        managed: Some(parse(
            r#"
default_permissions = ":read-only"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
"#,
        )),
        ..Default::default()
    };

    let effective = layers
        .effective_configuration(
            &BTreeMap::new(),
            None,
            None,
            PermissionProfileId::WorkspaceWrite,
        )
        .expect("managed fallback");

    assert_eq!(effective.default_profile_name, ":read-only");
    assert_eq!(
        effective.default_provenance,
        PermissionProfileProvenance::Managed
    );
    assert!(!effective
        .configuration
        .profile_allowed(":danger-full-access"));
}

#[test]
fn unresolved_managed_requirements_block_all_permission_resolution() {
    let layers = RuntimePermissionProfileLayers::blocked("cloud fetch failed");

    let error = layers
        .effective_configuration(
            &BTreeMap::new(),
            None,
            None,
            PermissionProfileId::WorkspaceWrite,
        )
        .expect_err("unresolved managed requirements must fail closed");

    assert!(error.to_string().contains("permissions are blocked"));
}

#[test]
fn managed_profile_name_collision_fails_closed() {
    let layers = RuntimePermissionProfileLayers {
        user: Some(parse(
            r#"
[permissions.acme]
extends = ":read-only"
"#,
        )),
        managed: Some(parse(
            r#"
default_permissions = "acme"

[allowed_permission_profiles]
acme = true

[permissions.acme]
extends = ":workspace"
"#,
        )),
        ..Default::default()
    };

    let error = layers
        .effective_configuration(
            &BTreeMap::new(),
            None,
            None,
            PermissionProfileId::WorkspaceWrite,
        )
        .expect_err("same-name managed profile must fail");
    assert!(error.to_string().contains("conflicts"));
}

#[test]
fn user_allowlist_can_only_narrow_managed_allowlist() {
    let layers = RuntimePermissionProfileLayers {
        managed: Some(parse(
            r#"
default_permissions = ":workspace"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
"#,
        )),
        ..Default::default()
    };
    let persisted_allowed = BTreeMap::from([
        (":read-only".to_string(), true),
        (":workspace".to_string(), false),
    ]);

    let effective = layers
        .effective_configuration(
            &BTreeMap::new(),
            Some(&persisted_allowed),
            None,
            PermissionProfileId::WorkspaceWrite,
        )
        .expect("narrow managed allowlist");

    assert_eq!(effective.default_profile_name, ":read-only");
    assert!(effective.configuration.profile_allowed(":read-only"));
    assert!(!effective.configuration.profile_allowed(":workspace"));
}

#[test]
fn managed_allowlist_can_reference_a_persisted_user_profile() {
    let layers = RuntimePermissionProfileLayers {
        managed: Some(parse(
            r#"
default_permissions = "team-review"

[allowed_permission_profiles]
team-review = true
"#,
        )),
        ..Default::default()
    };
    let persisted_profiles = BTreeMap::from([(
        "team-review".to_string(),
        CustomPermissionProfile {
            extends: Some(":read-only".to_string()),
            ..Default::default()
        },
    )]);

    let effective = layers
        .effective_configuration(
            &persisted_profiles,
            None,
            None,
            PermissionProfileId::WorkspaceWrite,
        )
        .expect("managed allowlist may reference loaded user profile");

    assert_eq!(effective.default_profile_name, "team-review");
    assert!(effective.configuration.profile_allowed("team-review"));
    assert_eq!(
        effective.default_provenance,
        PermissionProfileProvenance::User
    );
}

#[test]
fn managed_default_without_allowlist_is_invalid() {
    let layers = RuntimePermissionProfileLayers {
        managed: Some(parse("default_permissions = \":read-only\"")),
        ..Default::default()
    };
    let error = layers
        .effective_configuration(
            &BTreeMap::new(),
            None,
            None,
            PermissionProfileId::WorkspaceWrite,
        )
        .expect_err("managed default requires allowlist");
    assert!(error
        .to_string()
        .contains("requires allowed_permission_profiles"));
}

#[test]
fn trusted_project_layer_overrides_user_default_with_project_provenance() {
    let layers = RuntimePermissionProfileLayers {
        user: Some(parse("default_permissions = \":workspace\"")),
        ..Default::default()
    };
    let project = parse(
        r#"
default_permissions = "project-review"

[permissions.project-review]
extends = ":read-only"
"#,
    );

    let effective = layers
        .effective_configuration_with_project(
            &BTreeMap::new(),
            None,
            None,
            PermissionProfileId::WorkspaceWrite,
            Some(&project),
        )
        .expect("project configuration");

    assert_eq!(effective.default_profile_name, "project-review");
    assert_eq!(
        effective.default_provenance,
        PermissionProfileProvenance::Project
    );
    assert_eq!(
        effective.provenance_for("project-review"),
        PermissionProfileProvenance::Project
    );
}

#[test]
fn trusted_project_layer_cannot_bypass_managed_allowlist() {
    let layers = RuntimePermissionProfileLayers {
        managed: Some(parse(
            r#"
default_permissions = ":read-only"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
"#,
        )),
        ..Default::default()
    };
    let project = parse("default_permissions = \":danger-full-access\"");

    let effective = layers
        .effective_configuration_with_project(
            &BTreeMap::new(),
            None,
            None,
            PermissionProfileId::WorkspaceWrite,
            Some(&project),
        )
        .expect("managed project configuration");

    assert_eq!(effective.default_profile_name, ":read-only");
    assert_eq!(
        effective.default_provenance,
        PermissionProfileProvenance::Managed
    );
    assert!(!effective
        .configuration
        .profile_allowed(":danger-full-access"));
}

#[test]
fn trusted_project_layer_cannot_reenable_user_disabled_profile() {
    let layers = RuntimePermissionProfileLayers::default();
    let user_allowed = BTreeMap::from([
        (":read-only".to_string(), true),
        (":danger-full-access".to_string(), false),
    ]);
    let project = parse(
        r#"
default_permissions = ":danger-full-access"

[allowed_permission_profiles]
":read-only" = true
":danger-full-access" = true
"#,
    );

    let effective = layers
        .effective_configuration_with_project(
            &BTreeMap::new(),
            Some(&user_allowed),
            None,
            PermissionProfileId::WorkspaceWrite,
            Some(&project),
        )
        .expect("project allowlist is capped by user allowlist");

    assert_eq!(effective.default_profile_name, ":read-only");
    assert!(effective.configuration.profile_allowed(":read-only"));
    assert!(!effective
        .configuration
        .profile_allowed(":danger-full-access"));
}

#[test]
fn runtime_layers_contribute_to_policy_revision() {
    let layers = RuntimePermissionProfileLayers {
        user: Some(parse("default_permissions = \":read-only\"")),
        ..Default::default()
    };

    let revision = layers
        .effective_policy_revision(Some("local-user-change"))
        .expect("runtime revision");

    assert!(revision.starts_with("local-user-change+runtime-"));
    assert_eq!(revision.len(), "local-user-change+runtime-".len() + 64);
}

#[test]
fn configured_runtime_files_load_in_precedence_order() {
    let system_path = test_config_path("system.toml");
    let user_path = test_config_path("user.toml");
    let managed_path = test_config_path("requirements.toml");
    fs::write(
        &system_path,
        r#"
default_permissions = ":read-only"

[permissions.review]
extends = ":read-only"
"#,
    )
    .expect("write system config");
    fs::write(&user_path, "default_permissions = \":workspace\"").expect("write user config");
    fs::write(
        &managed_path,
        r#"
default_permissions = ":read-only"

[allowed_permission_profiles]
":read-only" = true
":workspace" = true
review = true
"#,
    )
    .expect("write managed requirements");

    let layers = RuntimePermissionProfileLayers::load_from_paths(
        ConfigPath {
            path: Some(system_path.clone()),
            required: true,
            secure_system_file: false,
        },
        ConfigPath {
            path: Some(user_path.clone()),
            required: true,
            secure_system_file: false,
        },
        ConfigPath {
            path: Some(managed_path.clone()),
            required: true,
            secure_system_file: false,
        },
    )
    .expect("load configured runtime layers");
    let effective = layers
        .effective_configuration(
            &BTreeMap::new(),
            None,
            None,
            PermissionProfileId::WorkspaceWrite,
        )
        .expect("resolve configured runtime layers");

    assert_eq!(effective.default_profile_name, ":workspace");
    assert!(effective.configuration.profiles.contains_key("review"));
    assert!(effective.configuration.profile_allowed("review"));

    let _ = fs::remove_file(system_path);
    let _ = fs::remove_file(user_path);
    let _ = fs::remove_file(managed_path);
}

#[test]
fn explicit_missing_or_malformed_runtime_file_fails_closed() {
    let missing = test_config_path("missing.toml");
    let error = RuntimePermissionProfileLayers::load_from_paths(
        ConfigPath {
            path: Some(missing),
            required: true,
            secure_system_file: false,
        },
        ConfigPath {
            path: None,
            required: false,
            secure_system_file: false,
        },
        ConfigPath {
            path: None,
            required: false,
            secure_system_file: false,
        },
    )
    .expect_err("explicit missing config must fail");
    assert!(error.to_string().contains("metadata"));

    let malformed = test_config_path("malformed.toml");
    fs::write(&malformed, "[permissions").expect("write malformed config");
    let error = RuntimePermissionProfileLayers::load_from_paths(
        ConfigPath {
            path: Some(malformed.clone()),
            required: true,
            secure_system_file: false,
        },
        ConfigPath {
            path: None,
            required: false,
            secure_system_file: false,
        },
        ConfigPath {
            path: None,
            required: false,
            secure_system_file: false,
        },
    )
    .expect_err("malformed config must fail");
    assert!(error.to_string().contains("parse system permission config"));
    let _ = fs::remove_file(malformed);
}

#[cfg(unix)]
#[test]
fn managed_system_file_rejects_insecure_policy_file() {
    let managed = test_config_path("user-owned-requirements.toml");
    fs::write(
        &managed,
        r#"
default_permissions = ":read-only"

[allowed_permission_profiles]
":read-only" = true
"#,
    )
    .expect("write user-owned managed config");
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&managed, fs::Permissions::from_mode(0o666))
            .expect("make managed config insecure");
    }

    let error = load_permission_document(
        "managed permission requirements",
        &ConfigPath {
            path: Some(managed.clone()),
            required: true,
            secure_system_file: true,
        },
        true,
    )
    .expect_err("managed policy must be root-owned");

    assert!(
        error.to_string().contains("owned by root")
            || error.to_string().contains("group- or world-writable")
    );
    let _ = fs::remove_file(managed);
}

#[test]
fn managed_runtime_file_rejects_unrelated_top_level_keys() {
    let managed = test_config_path("strict-requirements.toml");
    fs::write(&managed, "model = \"gpt-test\"").expect("write managed config");

    let error = load_permission_document(
        "managed permission requirements",
        &ConfigPath {
            path: Some(managed.clone()),
            required: true,
            secure_system_file: false,
        },
        true,
    )
    .expect_err("unrelated managed keys must fail closed");

    assert!(format!("{error:#}").contains("unsupported managed requirements top-level key"));
    let _ = fs::remove_file(managed);
}
