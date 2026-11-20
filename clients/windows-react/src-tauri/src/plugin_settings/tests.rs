use super::*;
use chatos_plugin_management_sdk::parse_plugin_manifest;

#[derive(Clone, Copy)]
enum HookHostBehavior {
    Success,
    Error,
    WrongActiveReply,
    WrongBundleReply,
}

struct HookHost {
    behavior: HookHostBehavior,
    commands: Mutex<Vec<HostCommand>>,
}

impl HookHost {
    fn new(behavior: HookHostBehavior) -> Self {
        Self {
            behavior,
            commands: Mutex::new(Vec::new()),
        }
    }
}

impl HostCommandClient for HookHost {
    fn execute(&self, command: HostCommand) -> HostCommandFuture<'_> {
        self.commands.lock().unwrap().push(command.clone());
        let behavior = self.behavior;
        Box::pin(async move {
            match (behavior, command) {
                (HookHostBehavior::Error, _) => Err("host unavailable".into()),
                (
                    _,
                    HostCommand::ListPluginHookBundles(_)
                    | HostCommand::ListPluginHookBundleRecords(_),
                ) => Ok(HostResult::PluginHookBundles {
                    bundles: Vec::new(),
                }),
                (
                    HookHostBehavior::WrongActiveReply,
                    HostCommand::SetActivePluginHookBundles(_),
                ) => Ok(HostResult::ActivePluginHookBundlesUpdated {
                    active_bundle_ids: vec!["unexpected-active-bundle".into()],
                }),
                (
                    _,
                    HostCommand::SetActivePluginHookBundles(SetActivePluginHookBundlesCommand {
                        bundle_ids,
                        ..
                    }),
                ) => Ok(HostResult::ActivePluginHookBundlesUpdated {
                    active_bundle_ids: bundle_ids,
                }),
                (HookHostBehavior::WrongBundleReply, HostCommand::PutPluginHookBundle(_)) => {
                    Ok(HostResult::ActivePluginHookBundlesUpdated {
                        active_bundle_ids: Vec::new(),
                    })
                }
                (_, HostCommand::PutPluginHookBundle(command)) => {
                    Ok(HostResult::PluginHookBundle {
                        bundle: chatos_local_agent_protocol::LocalPluginHookBundleRecord {
                            spec: command.bundle,
                            created_at_unix_ms: 1,
                        },
                    })
                }
                _ => panic!("unexpected host command"),
            }
        })
    }
}

fn hook_bundle_fixture() -> LocalPluginHookBundleSpec {
    LocalPluginHookBundleSpec {
        bundle_id: "bundle-1".into(),
        owner_user_id: "owner-1".into(),
        plugin_id: "plugin-1".into(),
        release_id: "release-1".into(),
        component_key: "lifecycle".into(),
        artifact_sha256: "a".repeat(64),
        content_sha256: "b".repeat(64),
        normalized_hook_sha256: "c".repeat(64),
        command_sha256: "d".repeat(64),
        permission_snapshot_sha256: "e".repeat(64),
        runner_policy_version: 1,
        installation_root: r"C:\ChatOS\Plugins\plugin-1".into(),
        permissions: vec!["process.spawn".into()],
        hooks: vec![chatos_local_agent_protocol::LocalPluginHookDefinition {
            hook_id: "SessionStart-0-0".into(),
            event: "SessionStart".into(),
            matcher: None,
            command: "node hook.js".into(),
            is_async: false,
            timeout_ms: 5_000,
            additional_context_limit: 65_536,
        }],
        enabled: true,
        trusted_at_unix_ms: 1,
    }
}

#[tokio::test]
async fn hook_bundles_are_deactivated_before_they_are_published() {
    let host = HookHost::new(HookHostBehavior::Success);
    let bundle = hook_bundle_fixture();

    publish_inactive_hook_bundles(&host, "owner-1", std::slice::from_ref(&bundle))
        .await
        .unwrap();

    let commands = host.commands.lock().unwrap();
    assert_eq!(commands.len(), 2);
    assert!(matches!(
        &commands[0],
        HostCommand::SetActivePluginHookBundles(command)
            if command.owner_user_id == "owner-1" && command.bundle_ids.is_empty()
    ));
    assert!(matches!(
        &commands[1],
        HostCommand::PutPluginHookBundle(command) if command.bundle == bundle
    ));
}

#[tokio::test]
async fn hook_bundle_publication_rejects_host_errors_and_wrong_replies() {
    let bundle = hook_bundle_fixture();
    for behavior in [
        HookHostBehavior::Error,
        HookHostBehavior::WrongActiveReply,
        HookHostBehavior::WrongBundleReply,
    ] {
        let host = HookHost::new(behavior);
        assert!(
            publish_inactive_hook_bundles(&host, "owner-1", std::slice::from_ref(&bundle))
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn package_without_hooks_still_confirms_an_empty_active_set() {
    let host = HookHost::new(HookHostBehavior::Success);
    publish_inactive_hook_bundles(&host, "owner-1", &[])
        .await
        .unwrap();
    assert!(matches!(
        host.commands.lock().unwrap().as_slice(),
        [HostCommand::SetActivePluginHookBundles(command)] if command.bundle_ids.is_empty()
    ));
}

#[tokio::test]
async fn reconciliation_activates_only_integrity_verified_installations() {
    let directory = tempfile::tempdir().unwrap();
    let plugin_root = directory.path().join("plugins");
    let installation_root = plugin_root.join("local-plugin-1");
    fs::create_dir_all(&installation_root).unwrap();
    let executable = installation_root.join("plugin.exe");
    fs::write(&executable, b"fixture").unwrap();
    let artifact = "a".repeat(64);
    crate::plugin_integrity::write_installation_integrity(
        &plugin_root,
        &installation_root,
        &artifact,
    )
    .unwrap();
    let mut bundle = hook_bundle_fixture();
    bundle.installation_root = installation_root
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into();
    bundle.artifact_sha256 = artifact.clone();
    bundle.release_id = "release-1".into();
    let record = LocalPluginInstallationRecord {
        spec: LocalPluginInstallationSpec {
            installation_id: "installation-1".into(),
            owner_user_id: "owner-1".into(),
            plugin_id: "plugin-1".into(),
            release_id: "release-1".into(),
            release_digest: format!("sha256:{artifact}"),
            component_id: "component-1".into(),
            component_revision: format!("integrity-v1-{artifact}"),
            server_id: "component-1".into(),
            executable_path: executable.to_string_lossy().into(),
            args: Vec::new(),
            working_directory: Some(installation_root.to_string_lossy().into()),
            environment_secret_refs: BTreeMap::new(),
            tool_prefix: None,
            allowed_tools: None,
            enabled: true,
        },
        version: 1,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    };
    let host = HookHost::new(HookHostBehavior::Success);
    reconcile_active_hook_bundles(
        &host,
        "owner-1",
        &plugin_root,
        std::slice::from_ref(&record),
        &[chatos_local_agent_protocol::LocalPluginHookBundleRecord {
            spec: bundle,
            created_at_unix_ms: 1,
        }],
    )
    .await
    .unwrap();
    let commands = host.commands.lock().unwrap();
    assert!(matches!(
        commands.last(),
        Some(HostCommand::SetActivePluginHookBundles(command))
            if command.bundle_ids == ["bundle-1"]
    ));
}

#[tokio::test]
async fn reconciliation_drops_disabled_or_invalid_bundles() {
    let directory = tempfile::tempdir().unwrap();
    let plugin_root = directory.path().join("plugins");
    let installation_root = plugin_root.join("local-plugin-1");
    fs::create_dir_all(&installation_root).unwrap();
    let executable = installation_root.join("plugin.exe");
    fs::write(&executable, b"fixture").unwrap();
    let artifact = "a".repeat(64);
    crate::plugin_integrity::write_installation_integrity(
        &plugin_root,
        &installation_root,
        &artifact,
    )
    .unwrap();
    let mut disabled = hook_bundle_fixture();
    disabled.installation_root = installation_root.to_string_lossy().into();
    disabled.artifact_sha256 = artifact.clone();
    disabled.release_id = "release-1".into();
    disabled.enabled = false;
    let mut malformed = disabled.clone();
    malformed.bundle_id = "bundle-2".into();
    malformed.enabled = true;
    malformed.hooks[0].event = "UnsupportedEvent".into();
    let record = LocalPluginInstallationRecord {
        spec: LocalPluginInstallationSpec {
            installation_id: "installation-1".into(),
            owner_user_id: "owner-1".into(),
            plugin_id: "plugin-1".into(),
            release_id: "release-1".into(),
            release_digest: format!("sha256:{artifact}"),
            component_id: "component-1".into(),
            component_revision: format!("integrity-v1-{artifact}"),
            server_id: "component-1".into(),
            executable_path: executable.to_string_lossy().into(),
            args: Vec::new(),
            working_directory: Some(installation_root.to_string_lossy().into()),
            environment_secret_refs: BTreeMap::new(),
            tool_prefix: None,
            allowed_tools: None,
            enabled: true,
        },
        version: 1,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    };
    let host = HookHost::new(HookHostBehavior::Success);
    reconcile_active_hook_bundles(
        &host,
        "owner-1",
        &plugin_root,
        std::slice::from_ref(&record),
        &[
            LocalPluginHookBundleRecord {
                spec: disabled,
                created_at_unix_ms: 1,
            },
            LocalPluginHookBundleRecord {
                spec: malformed,
                created_at_unix_ms: 1,
            },
        ],
    )
    .await
    .unwrap();
    let commands = host.commands.lock().unwrap();
    assert!(matches!(
        commands.last(),
        Some(HostCommand::SetActivePluginHookBundles(command))
            if command.bundle_ids.is_empty()
    ));
}

fn manual_manager() -> (tempfile::TempDir, PluginSettingsManager) {
    let directory = tempfile::tempdir().unwrap();
    let manager = PluginSettingsManager::new(directory.path().join("app-data"));
    (directory, manager)
}

fn stage_manual(
    manager: &PluginSettingsManager,
    source_root: &Path,
) -> ManualPluginDirectorySelection {
    manager
        .stage_manual_selection(
            "owner-1",
            "presentation-1",
            7,
            source_root.to_path_buf(),
            "source-plugin".into(),
        )
        .unwrap()
}

fn manual_input(selection_id: &str) -> ManualPluginSelectionInput {
    ManualPluginSelectionInput {
        owner_user_id: "owner-1".into(),
        presentation_id: "presentation-1".into(),
        selection_id: selection_id.into(),
    }
}

#[test]
fn manual_selection_is_bound_to_owner_presentation_and_session_generation() {
    let (directory, manager) = manual_manager();
    let selection = stage_manual(&manager, directory.path());
    let mut input = manual_input(&selection.selection_id);

    input.owner_user_id = "owner-2".into();
    assert!(manager.begin_manual_operation(&input, 7, None).is_err());
    input.owner_user_id = "owner-1".into();
    input.presentation_id = "presentation-2".into();
    assert!(manager.begin_manual_operation(&input, 7, None).is_err());
    input.presentation_id = "presentation-1".into();
    assert!(manager.begin_manual_operation(&input, 8, None).is_err());

    let (_, operation_id) = manager.begin_manual_operation(&input, 7, None).unwrap();
    manager.finish_manual_inspection(&selection.selection_id, &operation_id, None);
}

#[test]
fn manual_install_requires_the_exact_preflight_fingerprint() {
    let (directory, manager) = manual_manager();
    let selection = stage_manual(&manager, directory.path());
    let input = manual_input(&selection.selection_id);
    let (_, operation_id) = manager.begin_manual_operation(&input, 7, None).unwrap();
    manager.finish_manual_inspection(&selection.selection_id, &operation_id, Some("a".repeat(64)));

    assert!(manager
        .begin_manual_operation(&input, 7, Some(&"b".repeat(64)))
        .is_err());
    assert!(manager
        .begin_manual_operation(&input, 7, Some(&"a".repeat(64)))
        .is_ok());
}

#[test]
fn manual_selection_rejects_concurrent_operations() {
    let (directory, manager) = manual_manager();
    let selection = stage_manual(&manager, directory.path());
    let input = manual_input(&selection.selection_id);
    let (_, operation_id) = manager.begin_manual_operation(&input, 7, None).unwrap();

    assert_eq!(
        manager.begin_manual_operation(&input, 7, None).unwrap_err(),
        "Plugin 来源正在处理，请等待当前操作完成。"
    );
    manager.finish_manual_inspection(&selection.selection_id, &operation_id, None);
}

#[test]
fn successful_manual_install_consumes_the_selection() {
    let (directory, manager) = manual_manager();
    let selection = stage_manual(&manager, directory.path());
    let input = manual_input(&selection.selection_id);
    let (_, operation_id) = manager.begin_manual_operation(&input, 7, None).unwrap();
    manager.finish_manual_install(&selection.selection_id, &operation_id, true);

    assert!(manager.begin_manual_operation(&input, 7, None).is_err());
}

#[test]
fn discard_does_not_remove_a_leased_manual_operation() {
    let (directory, manager) = manual_manager();
    let selection = stage_manual(&manager, directory.path());
    let input = manual_input(&selection.selection_id);
    let (_, operation_id) = manager.begin_manual_operation(&input, 7, None).unwrap();
    manager.discard_manual_selection("owner-1", "presentation-1");
    let replacement = stage_manual(&manager, directory.path());

    assert!(manager
        .manual_selections
        .lock()
        .unwrap()
        .contains_key(&selection.selection_id));
    assert_ne!(replacement.selection_id, selection.selection_id);
    manager.finish_manual_install(&selection.selection_id, &operation_id, false);
    assert!(!manager
        .manual_selections
        .lock()
        .unwrap()
        .contains_key(&selection.selection_id));
}

#[test]
fn manual_directory_preflight_rejects_non_directories_and_links() {
    let directory = tempfile::tempdir().unwrap();
    let ordinary = directory.path().join("ordinary-plugin");
    fs::create_dir(&ordinary).unwrap();
    let (canonical, name) = prepare_manual_plugin_directory(&ordinary).unwrap();
    assert_eq!(canonical, ordinary.canonicalize().unwrap());
    assert_eq!(name, "ordinary-plugin");

    let file = directory.path().join("not-a-directory");
    fs::write(&file, b"plugin").unwrap();
    assert!(prepare_manual_plugin_directory(&file).is_err());
    assert!(prepare_manual_plugin_directory(Path::new("relative-plugin")).is_err());

    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_dir;
        let link = directory.path().join("linked-plugin");
        if symlink_dir(&ordinary, &link).is_ok() {
            assert!(prepare_manual_plugin_directory(&link).is_err());
            assert!(metadata_is_reparse(&fs::symlink_metadata(&link).unwrap()));
        }
    }
}

#[test]
fn plugin_secret_references_are_opaque_owner_and_plugin_scoped() {
    let environment = BTreeMap::from([
        ("API_TOKEN".into(), "${credential:primary-token}".into()),
        (
            "SECONDARY_TOKEN".into(),
            "${credential:primary-token}".into(),
        ),
    ]);
    let first = environment_secret_references("owner-1", "plugin-1", &environment).unwrap();
    let same = environment_secret_references("owner-1", "plugin-1", &environment).unwrap();
    let other_owner = environment_secret_references("owner-2", "plugin-1", &environment).unwrap();
    let other_plugin = environment_secret_references("owner-1", "plugin-2", &environment).unwrap();

    assert_eq!(first, same);
    assert_eq!(first["API_TOKEN"], first["SECONDARY_TOKEN"]);
    assert_ne!(first["API_TOKEN"], other_owner["API_TOKEN"]);
    assert_ne!(first["API_TOKEN"], other_plugin["API_TOKEN"]);
    assert!(!first["API_TOKEN"].contains("owner-1"));
    assert!(!first["API_TOKEN"].contains("plugin-1"));
    assert!(!first["API_TOKEN"].contains("primary-token"));
    assert!(environment_secret_references(
        "owner-1",
        "plugin-1",
        &BTreeMap::from([("API_TOKEN".into(), "plaintext".into())]),
    )
    .is_err());
}

#[test]
fn platform_gate_only_accepts_windows_or_universal_plugins() {
    assert!(windows_compatible(&[]));
    assert!(windows_compatible(&["windows".into()]));
    assert!(windows_compatible(&["windows-x86_64".into()]));
    assert!(!windows_compatible(&["macos-arm64".into(), "linux".into()]));
}

#[test]
fn presented_text_and_skill_names_do_not_expose_paths() {
    assert_eq!(safe_text("  Diagram Studio  ", "Plugin"), "Diagram Studio");
    assert_eq!(safe_text("bad\0text", "Plugin"), "Plugin");
    let manifest = parse_plugin_manifest(
        r#"{
            "schemaVersion": 3,
            "name": "safe-plugin",
            "version": "1.0.0",
            "description": "Safe",
            "author": { "name": "ChatOS" },
            "skills": ["./skills/diagram-architecture", "./private/SKILL.md"],
            "mcpServers": {},
            "interface": {
                "displayName": "Safe Plugin",
                "shortDescription": "Safe",
                "longDescription": "Safe plugin",
                "developerName": "ChatOS",
                "category": "Productivity"
            },
            "permissions": []
        }"#,
    )
    .unwrap();
    assert_eq!(
        presented_skills(&manifest),
        vec!["diagram-architecture".to_owned(), "SKILL".to_owned()]
    );
}

#[test]
fn runtime_projection_never_claims_an_unchecked_plugin_is_ready() {
    assert_eq!(runtime_status(false, false), "not_installed");
    assert_eq!(runtime_status(true, false), "disabled");
    assert_eq!(runtime_status(true, true), "not_checked");
}

#[test]
fn computer_use_health_updates_runtime_and_declared_permission_truthfully() {
    let mut snapshot = PluginSettingsSnapshot {
        plugins: vec![PresentedPlugin {
            id: "computer-use".into(),
            package_name: Some("open-computer-use".into()),
            plugin_key: None,
            display_name: "Visual Computer Use".into(),
            description: String::new(),
            category: "Developer Tools".into(),
            publisher: "Open Computer Use".into(),
            latest_version: "0.9.0".into(),
            installed_version: Some("0.9.0".into()),
            installed: true,
            update_available: false,
            enabled: true,
            has_ui: false,
            source: "local",
            runtime_status: "not_checked",
            runtime_tool_count: None,
            runtime_last_checked_at: None,
            runtime_error: None,
            mcp_server_count: 1,
            hook_count: 0,
            hook_runtime_status: None,
            skills: Vec::new(),
            permissions: vec![
                PresentedPluginPermission {
                    id: "process.spawn".into(),
                    required: true,
                    reason: None,
                    status: "declared_not_checked",
                },
                PresentedPluginPermission {
                    id: "computer.control".into(),
                    required: true,
                    reason: None,
                    status: "declared_not_checked",
                },
            ],
            installation_issue: None,
            windows_compatible: true,
            install_action_available: false,
            credentials: Vec::new(),
        }],
        marketplace_available: true,
        local_catalog_available: true,
        marketplace_error: None,
        local_catalog_error: None,
        secure_marketplace_install_available: true,
        manual_install_available: true,
    };
    apply_computer_use_runtime_health(
        &mut snapshot,
        Some(PluginRuntimeHealthProjection {
            status: "ready",
            checked_at: "2026-11-19T00:00:00.000Z".into(),
            error: None,
            process_spawn_status: "granted",
            computer_control_status: "needs_attention",
        }),
    );
    let plugin = &snapshot.plugins[0];
    assert_eq!(plugin.runtime_status, "ready");
    assert_eq!(
        plugin.runtime_last_checked_at.as_deref(),
        Some("2026-11-19T00:00:00.000Z")
    );
    assert_eq!(plugin.permissions[0].status, "granted");
    assert_eq!(plugin.permissions[1].status, "needs_attention");
}

#[test]
fn local_installations_do_not_depend_on_marketplace_source_availability() {
    let local = LocalPluginInstallationRecord {
        spec: LocalPluginInstallationSpec {
            installation_id: "installation-1".into(),
            owner_user_id: "owner-1".into(),
            plugin_id: "local-plugin-a".into(),
            release_id: "local-release-a".into(),
            release_digest: "sha256:a".into(),
            component_id: "component-1".into(),
            component_revision: "integrity-v1-a".into(),
            server_id: "component-1".into(),
            executable_path: "C:\\plugins\\local\\plugin.exe".into(),
            args: Vec::new(),
            working_directory: Some("C:\\plugins\\local".into()),
            environment_secret_refs: BTreeMap::new(),
            tool_prefix: None,
            allowed_tools: None,
            enabled: true,
        },
        version: 1,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    };
    assert!(is_local_plugin_installation(&local));

    let mut marketplace = local.clone();
    marketplace.spec.plugin_id = "marketplace-plugin".into();
    marketplace.spec.release_id = "release-1".into();
    assert!(!is_local_plugin_installation(&marketplace));
}

#[test]
fn skill_only_package_receives_a_non_executable_host_record() {
    let directory = tempfile::tempdir().unwrap();
    let installation_root = directory.path().join("installed");
    fs::create_dir_all(&installation_root).unwrap();
    fs::write(installation_root.join("chatos.plugin.json"), "{}").unwrap();
    let manifest = parse_plugin_manifest(
        r#"{
            "schemaVersion": 3,
            "name": "skill-only",
            "version": "1.0.0",
            "description": "Skill only",
            "author": { "name": "Local" },
            "skills": ["./skills/helper"],
            "mcpServers": {},
            "interface": {
                "displayName": "Skill Only",
                "shortDescription": "Skill only",
                "longDescription": "Skill only",
                "developerName": "Local",
                "category": "Developer Tools"
            },
            "permissions": []
        }"#,
    )
    .unwrap();
    let package = crate::plugin_installer::CommittedPluginPackage {
        plugin_root: directory.path().to_path_buf(),
        installation_root: installation_root.clone(),
        manifest,
        artifact_sha256: "a".repeat(64),
        backup_root: None,
    };
    let specs =
        local_installation_specs("owner-1", "plugin-1", "release-1", &package, &[]).unwrap();

    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].server_id, "skills-only");
    assert_eq!(specs[0].component_id, "skills-only");
    assert!(specs[0].args.is_empty());
    assert!(specs[0].allowed_tools.is_none());
    assert_eq!(
        Path::new(&specs[0].executable_path).file_name(),
        Some(std::ffi::OsStr::new("chatos.plugin.json"))
    );
}

#[test]
fn hook_only_package_receives_a_non_executable_host_record() {
    let directory = tempfile::tempdir().unwrap();
    let installation_root = directory.path().join("installed");
    fs::create_dir_all(&installation_root).unwrap();
    fs::write(installation_root.join("chatos.plugin.json"), "{}").unwrap();
    let manifest = parse_plugin_manifest(
        r#"{
            "schemaVersion": 3,
            "name": "hook-only",
            "version": "1.0.0",
            "description": "Hook only",
            "author": { "name": "Local" },
            "hooks": [{ "componentKey": "lifecycle", "source": "hooks/lifecycle.json" }],
            "mcpServers": {},
            "interface": {
                "displayName": "Hook Only",
                "shortDescription": "Hook only",
                "longDescription": "Hook only",
                "developerName": "Local",
                "category": "Developer Tools"
            },
            "permissions": []
        }"#,
    )
    .unwrap();
    let package = crate::plugin_installer::CommittedPluginPackage {
        plugin_root: directory.path().to_path_buf(),
        installation_root: installation_root.clone(),
        manifest,
        artifact_sha256: "a".repeat(64),
        backup_root: None,
    };
    let specs =
        local_installation_specs("owner-1", "plugin-1", "release-1", &package, &[]).unwrap();

    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].server_id, "hooks-only");
    assert_eq!(specs[0].component_id, "hooks-only");
    assert!(specs[0].args.is_empty());
    assert!(specs[0].allowed_tools.is_none());
    assert_eq!(
        Path::new(&specs[0].executable_path).file_name(),
        Some(std::ffi::OsStr::new("chatos.plugin.json"))
    );
}

#[test]
fn computer_use_worker_requires_spawn_and_control_for_the_exact_component() {
    let complete = parse_plugin_manifest(
        r#"{
            "schemaVersion": 3,
            "name": "open-computer-use",
            "version": "1.0.0",
            "description": "Computer Use",
            "author": { "name": "ChatOS" },
            "interface": {
                "displayName": "Computer Use",
                "shortDescription": "Control Windows",
                "longDescription": "Control Windows from screenshots",
                "developerName": "ChatOS",
                "category": "Productivity"
            },
            "mcpServers": {
                "computer-use": { "type": "stdio", "bin": "computer-use" }
            },
            "permissions": [
                { "permission": "process.spawn", "required": true, "components": ["computer-use"] },
                { "permission": "computer.control", "required": true, "components": ["computer-use"] }
            ]
        }"#,
    )
    .unwrap();
    assert!(required_component_permission(
        &complete,
        "process.spawn",
        "computer-use"
    ));
    assert!(required_component_permission(
        &complete,
        "computer.control",
        "computer-use"
    ));
    assert!(!required_component_permission(
        &complete,
        "computer.control",
        "other-component"
    ));
}
