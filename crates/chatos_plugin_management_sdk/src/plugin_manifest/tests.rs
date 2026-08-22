// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use super::*;
use crate::{normalized_plugin_manifest_sha256, plugin_component_descriptors, PluginComponentKind};

fn manifest_with_mcp(server: serde_json::Value) -> String {
    json!({
        "schemaVersion": 3,
        "name": "open-computer-use",
        "version": "0.2.3",
        "description": "Local computer use MCP",
        "author": {"name": "Open Computer Use"},
        "mcpServers": {"computer-use": server},
        "interface": {
            "displayName": "Open Computer Use",
            "shortDescription": "Computer use",
            "longDescription": "Runs the computer-use MCP from an installed npm package.",
            "developerName": "Open Computer Use",
            "category": "Developer Tools"
        },
        "permissions": [{
            "permission": "computer.control",
            "required": true,
            "components": ["computer-use"]
        }, {
            "permission": "process.spawn",
            "required": true,
            "components": ["computer-use"]
        }]
    })
    .to_string()
}

#[test]
fn parses_schema_v3_npm_stdio_mcp() {
    let raw = manifest_with_mcp(json!({
        "type": "stdio",
        "bin": "open-computer-use",
        "args": ["mcp"]
    }));
    let manifest = parse_plugin_manifest(raw.as_str()).expect("schema-v3 manifest");

    assert_eq!(manifest.schema_version, PLUGIN_MANIFEST_SCHEMA_VERSION);
    assert!(matches!(
        manifest.mcp_servers.as_slice(),
        [PluginMcpServer::Stdio { bin, args, .. }]
            if bin == "open-computer-use" && args == &["mcp"]
    ));
    let descriptors = plugin_component_descriptors(&manifest);
    assert!(matches!(
        descriptors.as_slice(),
        [component]
            if component.kind == PluginComponentKind::McpServer
                && component.runtime_kind == "npm_stdio"
                && component.component_key == "computer-use"
    ));
    assert_eq!(
        normalized_plugin_manifest_sha256(&manifest).unwrap().len(),
        64
    );
}

#[test]
fn infers_stdio_transport_from_bin() {
    let raw = manifest_with_mcp(json!({"bin": "open-computer-use", "args": ["mcp"]}));
    let manifest = parse_plugin_manifest(raw.as_str()).expect("inferred stdio manifest");
    assert!(matches!(
        manifest.mcp_servers[0],
        PluginMcpServer::Stdio { .. }
    ));
}

#[test]
fn accepts_https_http_mcp_for_local_client_invocation() {
    let raw = manifest_with_mcp(json!({
        "type": "http",
        "url": "https://mcp.example.com/v1",
        "headers": {"authorization": "Bearer ${TOKEN}"}
    }));
    let manifest = parse_plugin_manifest(raw.as_str()).expect("HTTP MCP manifest");
    assert!(matches!(
        manifest.mcp_servers[0],
        PluginMcpServer::Http { .. }
    ));
}

#[test]
fn rejects_config_file_and_arbitrary_command_stdio_models() {
    let mut config = serde_json::from_str::<serde_json::Value>(
        manifest_with_mcp(json!({"bin": "open-computer-use"})).as_str(),
    )
    .unwrap();
    config["mcpServers"] = json!("./.mcp.json");
    assert!(parse_plugin_manifest(config.to_string().as_str()).is_err());

    let command = manifest_with_mcp(json!({
        "type": "stdio",
        "command": "npx",
        "args": ["open-computer-use@latest"]
    }));
    assert!(parse_plugin_manifest(command.as_str()).is_err());
}

#[test]
fn rejects_old_schema_versions() {
    let mut raw = serde_json::from_str::<serde_json::Value>(
        manifest_with_mcp(json!({"bin": "open-computer-use"})).as_str(),
    )
    .unwrap();
    raw["schemaVersion"] = json!(2);
    assert!(parse_plugin_manifest(raw.to_string().as_str()).is_err());
}
