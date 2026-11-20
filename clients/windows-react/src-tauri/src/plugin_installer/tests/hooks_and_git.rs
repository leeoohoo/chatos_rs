use super::*;

#[test]
fn codex_hook_preflight_returns_only_safe_event_summary() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path();
    fs::create_dir_all(source.join("bin")).unwrap();
    fs::create_dir_all(source.join("hooks")).unwrap();
    fs::write(source.join("bin/local-tool.exe"), b"tool").unwrap();
    fs::write(
        source.join("hooks/lifecycle.json"),
        r#"{
            "hooks": {
                "SessionStart": [{
                    "matcher": "startup|resume",
                    "hooks": [{
                        "type": "command",
                        "command": "node secret-command.js",
                        "timeout": 5,
                        "additionalContextLimit": 65536
                    }]
                }],
                "UserPromptSubmit": [{
                    "hooks": [{"type": "command", "command": "node tracker.js"}]
                }]
            }
        }"#,
    )
    .unwrap();
    fs::write(
        source.join("chatos.plugin.json"),
        r#"{
            "schemaVersion": 3,
            "name": "hook-mixed-plugin",
            "version": "1.0.0",
            "description": "Hook fixture",
            "author": { "name": "Local" },
            "interface": {
                "displayName": "Hook Fixture",
                "shortDescription": "Hook fixture",
                "longDescription": "Hook fixture",
                "developerName": "Local",
                "category": "Productivity"
            },
            "mcpServers": {"local-tool": { "type": "stdio", "bin": "local-tool" }},
            "hooks": [{"componentKey": "lifecycle", "source": "./hooks/lifecycle.json"}],
            "permissions": [
                {"permission": "process.spawn", "required": true, "components": ["local-tool", "lifecycle"]},
                {"permission": "workspace.read", "required": true, "components": ["lifecycle"]}
            ]
        }"#,
    )
    .unwrap();

    let inspection = inspect_local_package(source, None).unwrap();
    assert_eq!(inspection.hooks.len(), 1);
    assert_eq!(inspection.hooks[0].component_key, "lifecycle");
    assert_eq!(inspection.hooks[0].hook_count, 2);
    assert_eq!(
        inspection.hooks[0].events,
        ["SessionStart", "UserPromptSubmit"]
    );
    assert_eq!(inspection.hooks[0].hooks.len(), 2);
    assert_eq!(
        inspection.hooks[0].hooks[0],
        LocalPluginHookDefinition {
            hook_id: "SessionStart-0-0".into(),
            event: "SessionStart".into(),
            matcher: Some("startup|resume".into()),
            command: "node secret-command.js".into(),
            is_async: false,
            timeout_ms: 5_000,
            additional_context_limit: 65_536,
        }
    );
    assert_eq!(inspection.hooks[0].hooks[1].timeout_ms, 5_000);
    assert_eq!(
        inspection.hooks[0].hooks[1].additional_context_limit,
        65_536
    );
    assert_digest(&inspection.hooks[0].content_sha256);
    assert_digest(&inspection.hooks[0].normalized_hook_sha256);
}

#[test]
fn trusted_hook_bundle_binds_complete_installed_material() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let plugin_root = directory.path().join("plugins");
    fs::create_dir_all(source.join("bin")).unwrap();
    fs::create_dir_all(source.join("hooks")).unwrap();
    fs::write(source.join("bin/local-tool.exe"), b"tool").unwrap();
    fs::write(
        source.join("hooks/lifecycle.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"node hook.js"}]}]}}"#,
    )
    .unwrap();
    fs::write(
        source.join("chatos.plugin.json"),
        r#"{
            "schemaVersion": 3,
            "name": "trusted-hook-plugin",
            "version": "1.0.0",
            "description": "Trusted hook fixture",
            "author": { "name": "Local" },
            "interface": {
                "displayName": "Trusted Hook",
                "shortDescription": "Hook",
                "longDescription": "Trusted hook fixture",
                "developerName": "Local",
                "category": "Productivity"
            },
            "mcpServers": {"local-tool": { "type": "stdio", "bin": "local-tool" }},
            "hooks": [{"componentKey": "lifecycle", "source": "./hooks/lifecycle.json"}],
            "permissions": [
                {"permission": "process.spawn", "required": true, "components": ["local-tool"]},
                {"permission": "workspace.read", "required": true, "components": ["lifecycle"]}
            ]
        }"#,
    )
    .unwrap();

    let inspection = inspect_local_package(&source, None).unwrap();
    let package =
        install_local_package(&plugin_root, &source, None, &inspection.fingerprint).unwrap();
    let bundles =
        trusted_local_hook_bundles(&package, "owner-1", "local-plugin-1", "local-release-1")
            .unwrap();

    assert_eq!(bundles.len(), 1);
    let bundle = &bundles[0];
    bundle.validate().unwrap();
    assert_eq!(bundle.owner_user_id, "owner-1");
    assert_eq!(bundle.plugin_id, "local-plugin-1");
    assert_eq!(bundle.release_id, "local-release-1");
    assert_eq!(bundle.component_key, "lifecycle");
    assert_eq!(bundle.hooks.len(), 1);
    assert_eq!(bundle.hooks[0].event, "SessionStart");
    assert_eq!(bundle.hooks[0].command, "node hook.js");
    assert!(bundle.permissions.contains(&"process.spawn".to_owned()));
    assert!(bundle.permissions.contains(&"workspace.read".to_owned()));
    for digest in [
        &bundle.artifact_sha256,
        &bundle.content_sha256,
        &bundle.normalized_hook_sha256,
        &bundle.command_sha256,
        &bundle.permission_snapshot_sha256,
    ] {
        assert_digest(digest);
    }

    let mut different_artifact = CommittedPluginPackage {
        plugin_root: package.plugin_root.clone(),
        installation_root: package.installation_root.clone(),
        manifest: package.manifest.clone(),
        artifact_sha256: "f".repeat(64),
        backup_root: None,
    };
    let changed = trusted_local_hook_bundles(
        &different_artifact,
        "owner-1",
        "local-plugin-1",
        "local-release-1",
    )
    .unwrap();
    assert_ne!(bundle.bundle_id, changed[0].bundle_id);
    assert_ne!(bundle.artifact_sha256, changed[0].artifact_sha256);
    different_artifact.artifact_sha256 = package.artifact_sha256.clone();

    fs::write(
        package.installation_root.join("hooks/lifecycle.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"node changed.js"}]}]}}"#,
    )
    .unwrap();
    let changed = trusted_local_hook_bundles(
        &different_artifact,
        "owner-1",
        "local-plugin-1",
        "local-release-1",
    )
    .unwrap();
    assert_ne!(bundle.bundle_id, changed[0].bundle_id);
    assert_ne!(bundle.command_sha256, changed[0].command_sha256);
}

#[test]
fn codex_hook_preflight_rejects_unsupported_and_unsafe_handlers() {
    let async_hook = r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"node hook.js","async":true}]}]}}"#;
    assert!(parse_codex_hook_bundle(async_hook).is_err());
    let pre_compact =
        r#"{"hooks":{"PreCompact":[{"hooks":[{"type":"command","command":"node hook.js"}]}]}}"#;
    assert!(parse_codex_hook_bundle(pre_compact).is_err());
    let invalid_matcher = r#"{"hooks":{"SessionStart":[{"matcher":"(","hooks":[{"type":"command","command":"node hook.js"}]}]}}"#;
    assert!(parse_codex_hook_bundle(invalid_matcher).is_err());
    let invalid_timeout = r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"node hook.js","timeout":31}]}]}}"#;
    assert!(parse_codex_hook_bundle(invalid_timeout).is_err());
    let mcp_tool =
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"mcp_tool","command":"ignored"}]}]}}"#;
    assert!(parse_codex_hook_bundle(mcp_tool).is_err());
}

#[test]
fn ponytail_compatibility_import_preserves_and_preflights_hooks() {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let source = repository_root.join("clients/fixtures/codex-hooks/ponytail-5.3.2");
    let directory = tempfile::tempdir().unwrap();
    let inspection =
        inspect_local_package_source(&directory.path().join("plugins"), &source, None).unwrap();

    assert_eq!(inspection.manifest.name, "ponytail");
    assert_eq!(inspection.manifest.hooks.len(), 1);
    assert_eq!(inspection.hooks.len(), 1);
    assert_eq!(inspection.hooks[0].hook_count, 3);
    assert_eq!(
        inspection.hooks[0].events,
        ["SessionStart", "SubagentStart", "UserPromptSubmit"]
    );
    assert_eq!(inspection.hooks[0].hooks.len(), 3);
    assert!(inspection.hooks[0]
        .hooks
        .iter()
        .all(|hook| !hook.command.is_empty() && hook.timeout_ms == 5_000));
    assert_eq!(
        inspection.hooks[0]
            .hooks
            .iter()
            .map(|hook| hook.event.as_str())
            .collect::<Vec<_>>(),
        ["SessionStart", "SubagentStart", "UserPromptSubmit"]
    );
    assert!(inspection.manifest.permissions.iter().any(|permission| {
        permission.permission == "workspace.read"
            && permission
                .components
                .contains(&inspection.hooks[0].component_key)
    }));
}

#[test]
fn git_sources_require_credential_free_https_and_safe_refs() {
    assert!(validate_git_repository_url("https://example.com/team/plugin.git").is_ok());
    assert!(validate_git_repository_url("http://example.com/team/plugin.git").is_err());
    assert!(validate_git_repository_url("https://user@example.com/team/plugin.git").is_err());
    assert!(validate_git_repository_url("https://example.com/team/plugin.git?token=x").is_err());
    assert!(validate_git_repository_url("file:///C:/plugin").is_err());

    assert_eq!(validate_git_reference(None).unwrap(), "HEAD");
    assert_eq!(
        validate_git_reference(Some("release/3.0.9")).unwrap(),
        "release/3.0.9"
    );
    assert!(validate_git_reference(Some("--upload-pack=evil")).is_err());
    assert!(validate_git_reference(Some("refs/heads/../secret")).is_err());
    assert!(validate_git_reference(Some("main@{1}")).is_err());
    assert!(validate_git_reference(Some("main branch")).is_err());
}

#[test]
fn git_archive_extraction_rejects_links_and_case_collisions() {
    let directory = tempfile::tempdir().unwrap();
    let safe_archive = directory.path().join("safe.tar");
    {
        let file = File::create(&safe_archive).unwrap();
        let mut archive = tar::Builder::new(file);
        append_test_tar_file(&mut archive, "chatos.plugin.json", b"{}");
        append_test_tar_file(&mut archive, "bin/tool.exe", b"tool");
        archive.finish().unwrap();
    }
    let safe_root = directory.path().join("safe");
    extract_git_archive(&safe_archive, &safe_root).unwrap();
    assert_eq!(fs::read(safe_root.join("bin/tool.exe")).unwrap(), b"tool");

    let link_archive = directory.path().join("link.tar");
    {
        let file = File::create(&link_archive).unwrap();
        let mut archive = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_link_name("outside").unwrap();
        header.set_cksum();
        archive
            .append_data(&mut header, "link", std::io::empty())
            .unwrap();
        archive.finish().unwrap();
    }
    assert!(extract_git_archive(&link_archive, &directory.path().join("link")).is_err());

    let collision_archive = directory.path().join("collision.tar");
    {
        let file = File::create(&collision_archive).unwrap();
        let mut archive = tar::Builder::new(file);
        append_test_tar_file(&mut archive, "Tool", b"one");
        append_test_tar_file(&mut archive, "tool", b"two");
        archive.finish().unwrap();
    }
    assert!(extract_git_archive(&collision_archive, &directory.path().join("collision")).is_err());
}

fn append_test_tar_file(archive: &mut tar::Builder<File>, path: &str, bytes: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive.append_data(&mut header, path, bytes).unwrap();
}

fn assert_digest(value: &str) {
    assert_eq!(value.len(), 64);
    assert!(value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
}

#[tokio::test]
#[ignore = "requires public HTTPS Git access"]
async fn git_https_acquisition_smoke() {
    let directory = tempfile::tempdir().unwrap();
    let acquired = acquire_git_package(
        &directory.path().join("plugins"),
        "https://github.com/octocat/Hello-World.git",
        Some("master"),
    )
    .await
    .unwrap();
    assert!(acquired.package_root.join("README").is_file());
    assert!(!acquired.package_root.join(".git").exists());
}
