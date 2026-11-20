use super::*;

#[cfg(windows)]
#[tokio::test]
async fn local_program_mcp_runs_real_preflight_and_install_regression() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("fixture.exe");
    let plugin_root = directory.path().join("plugins");
    compile_local_program_mcp_fixture(&source, "fixture_ping");

    let draft = ProgramMcpDraft {
        kind: "local_executable".into(),
        location: source.to_string_lossy().into_owned(),
        exact_version: "1.0.0".into(),
        display_name: "Local MCP Fixture".into(),
        component_key: "fixture".into(),
        executable_path: String::new(),
        arguments: Vec::new(),
        access_profile: "restricted".into(),
        asset_name: None,
        archive_sha256: None,
    };

    let inspection = inspect_program_mcp(&plugin_root, &draft).await.unwrap();
    assert_eq!(inspection.tools[0]["name"], "fixture_ping");
    assert!(valid_sha256(&inspection.fingerprint));

    let (package, installed_inspection) =
        install_program_mcp(&plugin_root, &draft, &inspection.fingerprint)
            .await
            .unwrap();
    assert_eq!(installed_inspection.fingerprint, inspection.fingerprint);
    assert!(package.installation_root.join("bin/fixture.exe").is_file());
    assert!(package
        .installation_root
        .join("chatos.plugin.json")
        .is_file());
    package.finish().unwrap();

    let repeated_plugin_root = directory.path().join("repeated-plugins");
    let (repeated_package, repeated_inspection) =
        install_program_mcp(&repeated_plugin_root, &draft, &inspection.fingerprint)
            .await
            .unwrap();
    assert_eq!(repeated_inspection.fingerprint, inspection.fingerprint);
    repeated_package.finish().unwrap();

    compile_local_program_mcp_fixture(&source, "fixture_pong");
    let changed = inspect_program_mcp(&plugin_root, &draft).await.unwrap();
    assert_eq!(changed.tools[0]["name"], "fixture_pong");
    assert_ne!(changed.fingerprint, inspection.fingerprint);
    assert!(
        install_program_mcp(&plugin_root, &draft, &inspection.fingerprint)
            .await
            .is_err()
    );
}

#[cfg(windows)]
fn compile_local_program_mcp_fixture(output: &Path, tool_name: &str) {
    let source = output.with_extension("rs");
    let source_text = r###"use std::io::{self, BufRead, Write};

fn main() {
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.contains("\"method\":\"initialize\"") {
            println!("{}", r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"local-fixture","version":"1"}}}"#);
        } else if line.contains("\"method\":\"tools/list\"") {
            println!("{}", r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"__TOOL_NAME__","description":"Fixture tool","inputSchema":{"type":"object","properties":{}}}]}}"#);
        }
        let _ = io::stdout().flush();
    }
}
"###
    .replace("__TOOL_NAME__", tool_name);
    fs::write(&source, source_text).unwrap();
    let status = std::process::Command::new("rustc")
        .args(["--edition=2021", "-O"])
        .arg(&source)
        .arg("-o")
        .arg(output)
        .status()
        .expect("rustc must be available for the local MCP fixture");
    assert!(
        status.success(),
        "rustc failed to compile the local MCP fixture"
    );
}

#[tokio::test]
#[ignore = "requires public npm Registry access"]
async fn npm_registry_acquisition_smoke() {
    let search = search_npm_packages("is-number").await.unwrap();
    assert!(search["results"]
        .as_array()
        .is_some_and(|results| !results.is_empty()));
    let resolved = resolve_npm_package("is-number", Some("7.0.0"))
        .await
        .unwrap();
    assert_eq!(resolved["package_name"], "is-number");
    assert_eq!(resolved["package_version"], "7.0.0");

    let directory = tempfile::tempdir().unwrap();
    let acquired = acquire_npm_package(&directory.path().join("plugins"), "is-number", "7.0.0")
        .await
        .unwrap();
    assert!(acquired.package_root.join("package.json").is_file());
    assert!(acquired.package_root.join("index.js").is_file());
}

#[tokio::test]
#[ignore = "requires public npm Registry access and local Node.js/npm"]
async fn npm_mcp_isolated_preflight_smoke() {
    let package_name = "@modelcontextprotocol/server-everything";
    let resolved = resolve_npm_package(package_name, None).await.unwrap();
    let package_version = resolved["package_version"].as_str().unwrap().to_owned();
    let binaries = resolved["binaries"].as_object().unwrap();
    let binary_name = binaries.keys().next().unwrap().to_owned();
    let draft = NpmMcpDraft {
        display_name: "MCP Everything".into(),
        description: "Public MCP protocol fixture".into(),
        component_key: "mcp-everything".into(),
        package_name: package_name.into(),
        package_version,
        binary_name,
        arguments: Vec::new(),
        access_profile: "restricted".into(),
    };
    let directory = tempfile::tempdir().unwrap();
    let inspection = inspect_npm_mcp(&directory.path().join("plugins"), &draft)
        .await
        .unwrap();
    assert!(!inspection.tools.is_empty());
    assert!(valid_sha256(&inspection.fingerprint));
    let (package, repeated) = install_npm_mcp(
        &directory.path().join("plugins"),
        &draft,
        &inspection.fingerprint,
    )
    .await
    .unwrap();
    assert_eq!(repeated.fingerprint, inspection.fingerprint);
    assert!(package
        .installation_root
        .join("chatos.plugin.json")
        .is_file());
    assert!(package
        .installation_root
        .join("bin/mcp-everything.exe")
        .is_file());
    package.finish().unwrap();
}
