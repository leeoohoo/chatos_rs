// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::{TaskEphemeralHttpMcpServer, TaskMcpConfig};
use chatos_mcp_runtime::{BuiltinToolRegistry, McpExecutorBuilder};
use std::collections::BTreeMap;

#[test]
fn resolves_workspace_from_local_connector_server() {
    let mut config = TaskMcpConfig::default();
    config.ephemeral_http_servers.push(TaskEphemeralHttpMcpServer {
            name: "local_connector".to_string(),
            url: "http://connector/api/local-connectors/relay/device-1/mcp?workspace_id=workspace%201&cwd=app".to_string(),
            headers: BTreeMap::new(),
            auth_mode: None,
        });
    assert_eq!(
        local_connector_workspace_id_from_config(&config, "device-1").as_deref(),
        Some("workspace 1")
    );
    assert_eq!(
        local_connector_workspace_id_from_config(&config, "device-2"),
        None
    );
}

#[test]
fn skill_server_name_is_stable_and_safe() {
    assert_eq!(
        local_skill_server_name("internal_skill_plugin_creator"),
        "local_skill_plugin_creator"
    );
    assert_eq!(
        local_skill_server_name("internal_skill_figma-use"),
        "local_skill_figma_use"
    );
}

#[test]
fn prepared_skill_tools_are_registered_with_the_model_runtime() {
    let prepared = PreparedSkill {
        skill_id: "internal_skill_visualize".to_string(),
        display_name: "Visualize".to_string(),
        instructions: "Create a local visualization.".to_string(),
        server_name: "local_skill_visualize".to_string(),
        tools: vec![json!({
            "name": "write_visualization_html",
            "description": "Write HTML locally.",
            "inputSchema": {"type":"object","properties":{},"additionalProperties":false}
        })],
        permissions: vec!["workspace.write".to_string()],
        owner_user_id: "owner-1".to_string(),
        device_id: "device-1".to_string(),
        workspace_id: "workspace-1".to_string(),
        task_id: "task-1".to_string(),
        run_id: "run-1".to_string(),
        bundle_id: "chatos.internal.visualize".to_string(),
        version: "1.0.0".to_string(),
        bundle_hash: "hash-1".to_string(),
        adapter_session_id: "session-1".to_string(),
        base_url: "http://127.0.0.1:39230".to_string(),
        internal_secret: "secret".to_string(),
    };
    let server = prepared.builtin_server();
    let session = prepared.session_handle(reqwest::Client::new());
    let provider = prepared.builtin_provider(session);
    let mut registry = BuiltinToolRegistry::new();
    registry.register(provider);
    let executor = McpExecutorBuilder::new()
        .with_builtin_servers([server])
        .with_builtin_registry(registry)
        .build_builtin_only()
        .expect("executor");
    let available_tools = executor.available_tools();
    let names = available_tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(names.contains(&"local_skill_visualize_write_visualization_html"));
}
