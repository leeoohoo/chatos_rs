use super::*;

#[test]
fn npm_sources_require_safe_names_exact_versions_and_registry_tarballs() {
    assert!(validate_npm_package_name("safe-package").is_ok());
    assert!(validate_npm_package_name("@scope/safe-package").is_ok());
    assert!(validate_npm_package_name("Unsafe-Package").is_err());
    assert!(validate_npm_package_name("../package").is_err());
    assert!(validate_exact_version("1.2.3").is_ok());
    assert!(validate_exact_version("1.2.3-beta.1+build.5").is_ok());
    assert!(validate_exact_version("latest").is_err());
    assert!(validate_exact_version("^1.2.3").is_err());
    assert!(validate_npm_tarball_url(
        "https://registry.npmjs.org/safe-package/-/safe-package-1.2.3.tgz"
    )
    .is_ok());
    assert!(
        validate_npm_tarball_url("https://example.com/safe-package/-/safe-package-1.2.3.tgz")
            .is_err()
    );
}

#[test]
fn npm_mcp_bin_access_profiles_lockfile_and_fingerprint_are_strict() {
    let manifest = json!({"bin": {"safe": "./dist/cli.js"}});
    assert_eq!(
        npm_binary_path(manifest.get("bin"), "safe-package", "safe").unwrap(),
        "dist/cli.js"
    );
    assert_eq!(
        npm_access_permissions("restricted", "safe").unwrap().len(),
        1
    );
    assert_eq!(
        npm_access_permissions("developer_analysis", "safe")
            .unwrap()
            .len(),
        6
    );
    assert!(npm_access_permissions("unbounded", "safe").is_err());

    let directory = tempfile::tempdir().unwrap();
    let lock = directory.path().join("package-lock.json");
    fs::write(
        &lock,
        serde_json::to_vec(&json!({
            "lockfileVersion": 3,
            "packages": {
                "": {},
                "node_modules/safe": {
                    "resolved": "https://registry.npmjs.org/safe/-/safe-1.0.0.tgz",
                    "integrity": format!("sha512-{}", STANDARD.encode([7_u8; 64]))
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    validate_npm_lock(&lock).unwrap();
    fs::write(
        &lock,
        serde_json::to_vec(&json!({
            "lockfileVersion": 3,
            "packages": {
                "": {},
                "node_modules/unsafe": {
                    "resolved": "https://example.com/unsafe.tgz",
                    "integrity": format!("sha512-{}", STANDARD.encode([7_u8; 64]))
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(validate_npm_lock(&lock).is_err());

    let draft = NpmMcpDraft {
        display_name: "Safe".into(),
        description: "Safe MCP".into(),
        component_key: "safe".into(),
        package_name: "safe-package".into(),
        package_version: "1.0.0".into(),
        binary_name: "safe".into(),
        arguments: vec!["mcp".into()],
        access_profile: "restricted".into(),
    };
    let one = npm_mcp_fingerprint(
        &"1".repeat(64),
        &draft,
        &[json!({"name": "ping", "inputSchema": {"type": "object"}})],
    )
    .unwrap();
    let two = npm_mcp_fingerprint(
        &"1".repeat(64),
        &draft,
        &[json!({"name": "pong", "inputSchema": {"type": "object"}})],
    )
    .unwrap();
    assert_ne!(one, two);
}

#[test]
fn compatible_stdio_mcp_config_is_normalized_in_staging_and_installed() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let plugin_root = directory.path().join("plugins");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("server.exe"), b"fixture executable").unwrap();
    fs::write(
        source.join("mcp.json"),
        serde_json::to_vec(&json!({
            "mcpServers": {
                "Fixture Server": {
                    "type": "stdio",
                    "command": ["./server.exe", "--stdio"],
                    "args": ["--safe"],
                    "env": {}
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let inspection = inspect_local_package_source(&plugin_root, &source, None).unwrap();
    assert!(inspection
        .manifest
        .name
        .starts_with("compat-mcp-fixture-server-"));
    assert_eq!(inspection.manifest.version, "0.0.0-local");
    let PluginMcpServer::Stdio { bin, args, env, .. } = &inspection.manifest.mcp_servers[0] else {
        panic!("expected a normalized stdio MCP server");
    };
    assert_eq!(bin, "compat-fixture-server");
    assert_eq!(args, &["--stdio", "--safe"]);
    assert!(env.is_empty());
    assert!(!source.join("chatos.plugin.json").exists());
    assert!(!source.join("bin").exists());

    let package =
        install_local_package(&plugin_root, &source, None, &inspection.fingerprint).unwrap();
    assert!(package
        .installation_root
        .join("chatos.plugin.json")
        .is_file());
    assert!(package
        .installation_root
        .join("bin/compat-fixture-server.exe")
        .is_file());
    package.finish().unwrap();
}

#[test]
fn portable_skill_only_plugin_is_normalized_in_staging_and_installed() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("portable-skill");
    let plugin_root = directory.path().join("plugins");
    fs::create_dir_all(source.join("references")).unwrap();
    fs::create_dir_all(source.join("scripts")).unwrap();
    fs::create_dir_all(source.join("assets")).unwrap();
    fs::write(
        source.join("plugin.json"),
        serde_json::to_vec(&json!({
            "name": "portable-helper",
            "version": "1.2.3",
            "description": "Portable helper",
            "displayName": "Portable Helper"
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        source.join("SKILL.md"),
        "---\nname: portable-helper\ndescription: Portable rules\n---\nFollow the portable rules.\n",
    )
    .unwrap();
    fs::write(source.join("references/guide.md"), "Guide").unwrap();
    fs::write(source.join("scripts/check.ps1"), "Write-Output safe").unwrap();
    fs::write(source.join("assets/readme.txt"), "Asset").unwrap();

    let inspection = inspect_local_package_source(&plugin_root, &source, None).unwrap();
    assert_eq!(inspection.manifest.name, "portable-helper");
    assert_eq!(inspection.manifest.version, "1.2.3");
    assert_eq!(inspection.manifest.skills.len(), 1);
    assert!(inspection.manifest.mcp_servers.is_empty());
    assert!(!source.join("chatos.plugin.json").exists());
    assert!(!source.join("skills").exists());

    let package =
        install_local_package(&plugin_root, &source, None, &inspection.fingerprint).unwrap();
    let installed_skill = package
        .installation_root
        .join("skills")
        .join("portable-helper");
    assert!(installed_skill.join("SKILL.md").is_file());
    assert_eq!(
        fs::read_to_string(installed_skill.join("references/guide.md")).unwrap(),
        "Guide"
    );
    assert!(installed_skill.join("scripts/check.ps1").is_file());
    assert!(installed_skill.join("assets/readme.txt").is_file());
    package.finish().unwrap();
}

#[test]
fn codex_and_claude_skill_plugins_keep_their_manifest_identity() {
    for (manifest_path, name) in [
        (".codex-plugin/plugin.json", "codex-helper"),
        (".claude-plugin/plugin.json", "claude-helper"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let manifest_path = directory.path().join(manifest_path);
        fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
        fs::write(
            &manifest_path,
            serde_json::to_vec(&json!({
                "name": name,
                "version": "2.0.0",
                "description": "Compatible skill Plugin"
            }))
            .unwrap(),
        )
        .unwrap();
        let skill = directory.path().join("skills").join(name);
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Compatible rules\n---\nUse the rules.\n"),
        )
        .unwrap();

        normalize_compatible_mcp_package(directory.path()).unwrap();
        let manifest = validate_local_package(directory.path()).unwrap();
        assert_eq!(manifest.name, name);
        assert_eq!(manifest.version, "2.0.0");
        assert_eq!(manifest.skills[0].path, format!("./skills/{name}"));
        assert!(manifest.mcp_servers.is_empty());
    }
}

#[test]
fn compatible_skill_import_rejects_conflicts_invalid_frontmatter_and_duplicates() {
    let conflict = tempfile::tempdir().unwrap();
    fs::write(
        conflict.path().join("SKILL.md"),
        "---\nname: helper\ndescription: Root\n---\nRoot rules.\n",
    )
    .unwrap();
    fs::create_dir_all(conflict.path().join("skills/helper")).unwrap();
    fs::write(
        conflict.path().join("skills/helper/SKILL.md"),
        "---\nname: helper\ndescription: Existing\n---\nExisting rules.\n",
    )
    .unwrap();
    assert!(normalize_compatible_mcp_package(conflict.path()).is_err());

    let invalid = tempfile::tempdir().unwrap();
    fs::write(invalid.path().join("SKILL.md"), "No frontmatter").unwrap();
    assert!(normalize_compatible_mcp_package(invalid.path()).is_err());

    let duplicate = tempfile::tempdir().unwrap();
    for directory_name in ["first", "second"] {
        let skill = duplicate.path().join("skills").join(directory_name);
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: first\ndescription: Duplicate\n---\nDuplicate rules.\n",
        )
        .unwrap();
    }
    fs::write(
        duplicate.path().join("plugin.json"),
        r#"{"name":"duplicate","version":"1.0.0"}"#,
    )
    .unwrap();
    normalize_compatible_mcp_package(duplicate.path()).unwrap();
    assert!(validate_local_package(duplicate.path()).is_err());
}

#[test]
fn compatible_skill_directory_scan_is_bounded_before_processing() {
    let directory = tempfile::tempdir().unwrap();
    let skills = directory.path().join("skills");
    fs::create_dir_all(&skills).unwrap();
    for index in 0..=MAXIMUM_COMPATIBLE_SKILL_ENTRIES {
        fs::create_dir(skills.join(format!("skill-{index}"))).unwrap();
    }
    assert!(normalize_compatible_skills(directory.path()).is_err());
}

#[test]
fn compatible_stdio_mcp_rejects_secrets_shell_commands_and_remote_transports() {
    for (name, server) in [
        (
            "secret",
            json!({"command": "node", "env": {"API_KEY": "plaintext"}}),
        ),
        ("shell", json!({"command": "npx", "args": ["server"]})),
        (
            "remote",
            json!({"type": "http", "url": "https://example.com/mcp"}),
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("mcp.json"),
            serde_json::to_vec(&json!({"mcpServers": {name: server}})).unwrap(),
        )
        .unwrap();
        assert!(normalize_compatible_mcp_package(directory.path()).is_err());
        assert!(!directory.path().join("chatos.plugin.json").exists());
    }
}

#[tokio::test]
#[ignore = "requires local Node.js"]
async fn compatible_node_mcp_config_smoke() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let plugin_root = directory.path().join("plugins");
    fs::create_dir_all(&source).unwrap();
    fs::write(
        source.join("server.mjs"),
        r#"import readline from "node:readline";
const lines = readline.createInterface({ input: process.stdin });
lines.on("line", (line) => {
  const request = JSON.parse(line);
  if (request.id === undefined) return;
  let result = {};
  if (request.method === "initialize") {
    result = { protocolVersion: request.params.protocolVersion, capabilities: {}, serverInfo: { name: "compat-fixture", version: "1" } };
  } else if (request.method === "tools/list") {
    result = { tools: [{ name: "ping", description: "Ping", inputSchema: { type: "object", properties: {} } }] };
  }
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\n");
});
"#,
    )
    .unwrap();
    fs::write(
        source.join("claude_desktop_config.json"),
        serde_json::to_vec(&json!({
            "mcpServers": {
                "compat-fixture": {"command": "node", "args": ["server.mjs"]}
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let acquired = acquire_local_package(&plugin_root, &source).unwrap();
    normalize_compatible_mcp_package(&acquired.package_root).unwrap();
    let local = inspect_local_package(&acquired.package_root, None).unwrap();
    let PluginMcpServer::Stdio { bin, args, env, .. } = &local.manifest.mcp_servers[0] else {
        panic!("expected a normalized stdio MCP server");
    };
    let executable = resolve_executable(&acquired.package_root, bin).unwrap();
    let tools = inspect_mcp_tools(&acquired.package_root, &executable, args, env)
        .await
        .unwrap();
    assert_eq!(tools[0]["name"], "ping");
    drop(acquired);

    let package = install_local_package(&plugin_root, &source, None, &local.fingerprint).unwrap();
    let installed = validate_local_package(&package.installation_root).unwrap();
    let PluginMcpServer::Stdio { bin, args, env, .. } = &installed.mcp_servers[0] else {
        panic!("expected an installed stdio MCP server");
    };
    let executable = resolve_executable(&package.installation_root, bin).unwrap();
    let tools = inspect_mcp_tools(&package.installation_root, &executable, args, env)
        .await
        .unwrap();
    assert_eq!(tools[0]["name"], "ping");
    package.finish().unwrap();
}
