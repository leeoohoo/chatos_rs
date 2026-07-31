// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    filter_transient_model_input_for_runtime, is_plugin_hook_dispatch, plugin_agent_prompt_text,
    plugin_command_execution_constraints, plugin_command_prompt_text, plugin_server_name,
    sanitize_runtime_error, validate_agent_response, validate_command_response,
    validate_hook_response, validate_native_skill_response, validate_plugin_relay_base_url,
    validate_prepare_response, validate_ui_response, PluginToolLifecycleHook,
    PluginToolLifecycleStage, PreparedPluginRuntime,
};
use chatos_mcp_runtime::{ToolLifecycleEvent, ToolLifecycleOutcome};
use chatos_plugin_management_sdk::{
    plugin_ui_snapshot_sha256, PluginComponentKind, PluginExecutionHost, PluginHookEvent,
    PluginHookOutcome, PluginUiAssetSnapshot, RunPluginComponentSnapshot, RunPluginSnapshot,
    PLUGIN_UI_BRIDGE_PROTOCOL_VERSION_V1, PLUGIN_UI_HOST_CSP_V1, PLUGIN_UI_IFRAME_SANDBOX_V1,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn plugin_snapshot() -> RunPluginSnapshot {
    RunPluginSnapshot {
        plugin_id: "plugin-browser".to_string(),
        release_id: "release-1".to_string(),
        version: "1.0.0".to_string(),
        artifact_sha256: "abc123".to_string(),
        device_id: Some("device-1".to_string()),
        workspace_id: Some("workspace-1".to_string()),
        component_snapshots: Vec::new(),
        permission_snapshot: Vec::new(),
        auth_connection_ids: Vec::new(),
    }
}

fn component_snapshot() -> RunPluginComponentSnapshot {
    RunPluginComponentSnapshot {
        component_key: "browser.tools/v1".to_string(),
        kind: PluginComponentKind::McpServer,
        execution_host: PluginExecutionHost::Local,
        content_sha256: "component-hash".to_string(),
        runtime: BTreeMap::new(),
    }
}

#[test]
fn plugin_prompts_sort_by_kind_then_stable_text_identity() {
    fn prompt(text: &str) -> serde_json::Value {
        json!({
            "type": "message",
            "role": "system",
            "content": [{"type": "input_text", "text": text}]
        })
    }

    let mut runtime = PreparedPluginRuntime {
        prompt_items: vec![
            prompt("[Plugin Agent Profile: z]"),
            prompt("[Plugin Skill: z]"),
            prompt("[Plugin Command: a]"),
            prompt("[Plugin Skill: a]"),
        ],
        ..PreparedPluginRuntime::default()
    };
    runtime.sort_prompt_items();
    assert_eq!(
        runtime
            .prompt_items
            .iter()
            .filter_map(|item| item
                .pointer("/content/0/text")
                .and_then(|text| text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            "[Plugin Skill: a]",
            "[Plugin Skill: z]",
            "[Plugin Command: a]",
            "[Plugin Agent Profile: z]",
        ]
    );
}

mod constraints;
mod native;
mod validation;
