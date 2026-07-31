// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::sync::Arc;

use chatos_mcp_runtime::{BuiltinToolProvider, McpBuiltinServer};
use chatos_plugin_management_sdk::{
    PluginComponentKind, RunPluginComponentSnapshot, RunPluginSnapshot,
};
use serde_json::{json, Map, Value};

use super::{
    plugin_agent_prompt_text, plugin_command_prompt_text, plugin_server_name,
    required_response_text, validate_agent_response, validate_command_response,
    validate_hook_response, validate_native_skill_response, validate_prepare_response,
    validate_ui_response, PluginRelayClient, PluginRelayToolProvider, PreparedPluginRuntime,
    PreparedPluginSession,
};

pub(super) struct PreparedComponent {
    server: Option<McpBuiltinServer>,
    provider: Option<Arc<dyn BuiltinToolProvider>>,
    prompt_items: Vec<Value>,
    session: PreparedPluginSession,
}

impl PreparedPluginRuntime {
    pub(super) fn extend(&mut self, component: PreparedComponent) {
        if let Some(server) = component.server {
            self.builtin_servers.push(server);
        }
        if let Some(provider) = component.provider {
            self.providers.push(provider);
        }
        self.prompt_items.extend(component.prompt_items);
        self.sessions.push(component.session);
    }
}

pub(super) async fn prepare_component(
    relay: PluginRelayClient,
    plugin: &RunPluginSnapshot,
    component: &RunPluginComponentSnapshot,
    effective_workspace_dir: &str,
) -> Result<PreparedComponent, String> {
    let mut body = Map::from_iter([
        ("plugin_id".to_string(), json!(plugin.plugin_id)),
        ("release_id".to_string(), json!(plugin.release_id)),
        ("artifact_sha256".to_string(), json!(plugin.artifact_sha256)),
        ("component_key".to_string(), json!(component.component_key)),
        (
            "permission_snapshot".to_string(),
            json!(plugin.permission_snapshot),
        ),
    ]);
    match component.kind {
        PluginComponentKind::SkillCollection => {
            let skill_keys = component
                .runtime
                .get("skill_keys")
                .and_then(Value::as_array)
                .filter(|values| !values.is_empty())
                .ok_or_else(|| {
                    format!(
                        "Plugin Skill component is missing immutable skill_keys: {}:{}",
                        plugin.plugin_id, component.component_key
                    )
                })?;
            body.insert("skill_keys".to_string(), Value::Array(skill_keys.clone()));
            let runtime_kind = component
                .runtime
                .get("runtime_kind")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    format!(
                        "Plugin Skill component is missing immutable runtime_kind: {}:{}",
                        plugin.plugin_id, component.component_key
                    )
                })?;
            body.insert("runtime_kind".to_string(), json!(runtime_kind));
            if let Some(metadata) = component.runtime.get("metadata") {
                body.insert("runtime_metadata".to_string(), metadata.clone());
            }
            body.insert(
                "content_sha256".to_string(),
                json!(component.content_sha256),
            );
        }
        PluginComponentKind::McpServer => {
            if let Some(server_key) = component.runtime.get("server_key") {
                body.insert("server_key".to_string(), server_key.clone());
            }
            for key in ["tool_allowlist", "tool_blocklist"] {
                if let Some(value) = component.runtime.get(key) {
                    body.insert(key.to_string(), value.clone());
                }
            }
        }
        PluginComponentKind::Command => {
            body.insert(
                "content_sha256".to_string(),
                json!(component.content_sha256),
            );
            if let Some(arguments) = component.runtime.get("arguments") {
                body.insert("arguments".to_string(), arguments.clone());
            }
        }
        PluginComponentKind::Agent => {
            body.insert(
                "content_sha256".to_string(),
                json!(component.content_sha256),
            );
        }
        PluginComponentKind::HookSet => {
            body.insert(
                "content_sha256".to_string(),
                json!(component.content_sha256),
            );
        }
        PluginComponentKind::UiContribution => {
            body.insert(
                "content_sha256".to_string(),
                json!(component.content_sha256),
            );
        }
        _ => {
            return Err(format!(
                "Plugin component runtime is not supported by Task Runner: {}:{}",
                plugin.plugin_id, component.component_key
            ));
        }
    }
    let response = relay.request("prepare", Value::Object(body)).await?;
    if response.get("run_id").and_then(Value::as_str) != Some(relay.run_id.as_str()) {
        return Err("Plugin prepare response run_id does not match the active Run".to_string());
    }
    validate_prepare_response(plugin, component, &response)?;
    let adapter_session_id = required_response_text(&response, "adapter_session_id")?;
    let operations = response
        .get("operations")
        .and_then(Value::as_array)
        .ok_or_else(|| "Plugin prepare response is missing operations".to_string())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| "Plugin prepare response contains an invalid operation".to_string())
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let hook_snapshot_sha256 = if component.kind == PluginComponentKind::HookSet {
        Some(validate_hook_response(plugin, component, &response)?)
    } else {
        None
    };
    let ui_snapshot = if component.kind == PluginComponentKind::UiContribution {
        Some(validate_ui_response(plugin, component, &response)?)
    } else {
        None
    };
    if component.kind == PluginComponentKind::UiContribution && !operations.is_empty() {
        return Err(
            "Plugin UI prepare response must not publish executable operations before the isolated Workbench host is attached"
                .to_string(),
        );
    }
    let session = PreparedPluginSession {
        relay,
        plugin_id: plugin.plugin_id.clone(),
        release_id: plugin.release_id.clone(),
        artifact_sha256: plugin.artifact_sha256.clone(),
        component_key: component.component_key.clone(),
        adapter_session_id,
        component_kind: component.kind,
        operations,
        hook_snapshot_sha256,
        ui_snapshot,
    };
    let mut prompt_items = Vec::new();
    if let Some(skills) = response.get("skills").and_then(Value::as_array) {
        for skill in skills {
            let Some(instructions) = skill
                .get("instructions")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let skill_key = skill
                .get("skill_key")
                .and_then(Value::as_str)
                .unwrap_or("plugin-skill");
            prompt_items.push(json!({
                "type": "message",
                "role": "system",
                "content": [{
                    "type": "input_text",
                    "text": format!(
                        "{}\n\n[Plugin Skill: {} / {} / {}]\n{}",
                        super::THIRD_PARTY_PLUGIN_ENVELOPE,
                        plugin.plugin_id,
                        component.component_key,
                        skill_key,
                        instructions
                    )
                }]
            }));
        }
    }
    if component.kind == PluginComponentKind::Command {
        let commands = response
            .get("commands")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                "Plugin Command prepare response is missing the Command snapshot".to_string()
            })?;
        if commands.len() != 1 {
            return Err(
                "Plugin Command prepare response must contain exactly one Command".to_string(),
            );
        }
        let command = &commands[0];
        validate_command_response(plugin, component, command)?;
        let command_prompt = plugin_command_prompt_text(plugin, component, command)?;
        prompt_items.push(json!({
            "type": "message",
            "role": "system",
            "content": [{
                "type": "input_text",
                "text": command_prompt
            }]
        }));
        return Ok(PreparedComponent {
            server: None,
            provider: None,
            prompt_items,
            session,
        });
    }
    if component.kind == PluginComponentKind::HookSet {
        return Ok(PreparedComponent {
            server: None,
            provider: None,
            prompt_items,
            session,
        });
    }
    if component.kind == PluginComponentKind::UiContribution {
        return Ok(PreparedComponent {
            server: None,
            provider: None,
            prompt_items,
            session,
        });
    }
    if let Some(native_skill) = response
        .get("native_skill")
        .filter(|value| !value.is_null())
    {
        validate_native_skill_response(plugin, component, native_skill)?;
        let tools = native_skill
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .filter(|tools| !tools.is_empty())
            .ok_or_else(|| {
                "native Plugin Skill prepare response is missing executable tools".to_string()
            })?;
        let operation = response
            .get("operations")
            .and_then(Value::as_array)
            .and_then(|operations| {
                operations
                    .iter()
                    .filter_map(Value::as_str)
                    .find(|operation| *operation == "native_skill_tool_call")
            })
            .ok_or_else(|| {
                "native Plugin Skill prepare response did not publish native_skill_tool_call"
                    .to_string()
            })?
            .to_string();
        let server_name = plugin_server_name(plugin, component);
        let provider: Arc<dyn BuiltinToolProvider> = Arc::new(PluginRelayToolProvider {
            server_name: server_name.clone(),
            session: session.clone(),
            operation,
            tools,
        });
        let allow_writes = native_skill
            .get("permissions")
            .and_then(Value::as_array)
            .is_some_and(|permissions| {
                permissions
                    .iter()
                    .any(|permission| permission.as_str() == Some("workspace.write"))
            });
        return Ok(PreparedComponent {
            server: Some(plugin_relay_server(
                server_name,
                &session,
                effective_workspace_dir,
                allow_writes,
                "plugin_native_relay",
            )),
            provider: Some(provider),
            prompt_items,
            session,
        });
    }
    if component.kind == PluginComponentKind::Agent {
        let agents = response
            .get("agents")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                "Plugin Agent prepare response is missing the Agent snapshot".to_string()
            })?;
        if agents.len() != 1 {
            return Err("Plugin Agent prepare response must contain exactly one Agent".to_string());
        }
        let agent = &agents[0];
        validate_agent_response(plugin, component, agent)?;
        let agent_prompt = plugin_agent_prompt_text(plugin, component, agent)?;
        prompt_items.push(json!({
            "type": "message",
            "role": "system",
            "content": [{
                "type": "input_text",
                "text": agent_prompt
            }]
        }));
        return Ok(PreparedComponent {
            server: None,
            provider: None,
            prompt_items,
            session,
        });
    }
    let Some(mcp) = response.get("mcp").filter(|value| !value.is_null()) else {
        return Ok(PreparedComponent {
            server: None,
            provider: None,
            prompt_items,
            session,
        });
    };
    let tools = mcp
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "Plugin MCP prepare response is missing tools".to_string())?;
    let operation = response
        .get("operations")
        .and_then(Value::as_array)
        .and_then(|operations| {
            operations
                .iter()
                .filter_map(Value::as_str)
                .find(|operation| *operation == "mcp_tools_call")
        })
        .ok_or_else(|| "Plugin MCP prepare response did not publish mcp_tools_call".to_string())?
        .to_string();
    let server_name = plugin_server_name(plugin, component);
    let provider: Arc<dyn BuiltinToolProvider> = Arc::new(PluginRelayToolProvider {
        server_name: server_name.clone(),
        session: session.clone(),
        operation,
        tools,
    });
    Ok(PreparedComponent {
        server: Some(plugin_relay_server(
            server_name,
            &session,
            effective_workspace_dir,
            false,
            "plugin_relay",
        )),
        provider: Some(provider),
        prompt_items,
        session,
    })
}

fn plugin_relay_server(
    name: String,
    session: &PreparedPluginSession,
    effective_workspace_dir: &str,
    allow_writes: bool,
    kind: &str,
) -> McpBuiltinServer {
    let native_relay = kind == "plugin_native_relay";
    McpBuiltinServer {
        name,
        kind: kind.to_string(),
        workspace_dir: effective_workspace_dir.to_string(),
        user_id: Some(session.relay.owner_user_id.clone()),
        project_id: None,
        remote_connection_id: None,
        contact_agent_id: None,
        auto_create_task: false,
        allow_writes,
        max_file_bytes: if native_relay { 2 * 1024 * 1024 } else { 0 },
        max_write_bytes: if native_relay && allow_writes {
            2 * 1024 * 1024
        } else {
            0
        },
        search_limit: 0,
    }
}
