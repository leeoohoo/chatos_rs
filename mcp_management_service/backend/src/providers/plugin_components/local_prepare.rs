// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{McpProviderKind, ProjectExecutionContext, ResolvedMcpRoute};
use chatos_plugin_management_sdk::PluginComponentKind;
use serde_json::{json, Value};

use super::result::{plugin_agent_result, plugin_command_result};
use super::validation::*;
use super::{
    PluginComponentProvider, PluginPrepareResponse, AGENT_APPLY_OPERATION, COMMAND_INVOKE_OPERATION,
};
use crate::providers::ProviderCallError;
use crate::runtime::{
    resolve_plugin_local_execution_target, PluginLocalToolComponentBinding,
    PluginToolComponentRuntimeBinding,
};

impl PluginComponentProvider {
    pub(super) async fn prepare_local(
        &self,
        immutable: &PluginToolComponentRuntimeBinding,
        route: &ResolvedMcpRoute,
        context: &ProjectExecutionContext,
        runtime_session_id: &str,
        owner_user_id: &str,
        expires_at_unix: i64,
    ) -> Result<PluginLocalToolComponentBinding, ProviderCallError> {
        validate_immutable_route(immutable, route, McpProviderKind::PluginLocal)?;
        let target = resolve_plugin_local_execution_target(
            context,
            immutable.installation_device_id.as_deref(),
            immutable.permission_snapshot.as_slice(),
            false,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let mut body = serde_json::Map::from_iter([
            ("run_id".to_string(), json!(runtime_session_id)),
            ("plugin_id".to_string(), json!(immutable.plugin_id)),
            ("release_id".to_string(), json!(immutable.release_id)),
            (
                "artifact_sha256".to_string(),
                json!(immutable.artifact_sha256),
            ),
            (
                "component_key".to_string(),
                json!(immutable.component.component_key),
            ),
            (
                "permission_snapshot".to_string(),
                json!(immutable.permission_snapshot),
            ),
            (
                "content_sha256".to_string(),
                json!(immutable.component_content_sha256),
            ),
        ]);
        match immutable.component.kind {
            PluginComponentKind::SkillCollection => {
                let snapshot = immutable.skill_snapshot.as_ref().ok_or_else(|| {
                    ProviderCallError::provider_unavailable(
                        "Plugin Skill component is missing its required v2 snapshot",
                    )
                })?;
                body.insert(
                    "skill_keys".to_string(),
                    json!([immutable.component.component_key]),
                );
                body.insert("skill_runtime_protocol".to_string(), json!(2));
                body.insert("skill_snapshot".to_string(), json!(snapshot));
            }
            PluginComponentKind::Command | PluginComponentKind::Agent => {
                body.insert("catalog_only".to_string(), json!(true));
                if immutable.component.kind == PluginComponentKind::Command {
                    if let Some(arguments) = immutable.command_arguments.as_deref() {
                        body.insert("arguments".to_string(), json!(arguments));
                    }
                }
            }
            _ => {
                return Err(ProviderCallError::provider_unavailable(
                    "Plugin component kind is not an Agent tool",
                ));
            }
        }
        let bytes = self
            .request_local(
                owner_user_id,
                target.device_id.as_str(),
                target.workspace_id.as_deref(),
                target.project_root.as_deref(),
                "prepare",
                Value::Object(body),
            )
            .await?;
        let prepared =
            serde_json::from_slice::<PluginPrepareResponse>(bytes.as_slice()).map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Plugin Local component prepare returned invalid JSON: {error}"
                ))
            })?;
        validate_prepare_identity(immutable, runtime_session_id, expires_at_unix, &prepared)?;
        let (operation, tools, instruction_items, static_result) = match immutable.component.kind {
            PluginComponentKind::SkillCollection => {
                if prepared.skills.len() != 1 {
                    return Err(ProviderCallError::invalid_response(
                        "Plugin Skill prepare must return exactly one Skill snapshot",
                    ));
                }
                let skill = &prepared.skills[0];
                validate_local_skill_catalog(immutable, skill)?;
                required_operation(
                    prepared.operations.as_slice(),
                    super::SKILL_ACTIVATE_OPERATION,
                )?;
                required_operation(
                    prepared.operations.as_slice(),
                    super::SKILL_READ_RESOURCE_OPERATION,
                )?;
                (
                    super::SKILL_ACTIVATE_OPERATION.to_string(),
                    skill_runtime_tool_definitions(),
                    Vec::new(),
                    None,
                )
            }
            PluginComponentKind::Command => {
                if prepared.commands.len() != 1 {
                    return Err(ProviderCallError::invalid_response(
                        "Plugin Command catalog prepare must return exactly one command",
                    ));
                }
                validate_command_snapshot(
                    immutable,
                    &prepared.commands[0],
                    immutable.command_arguments.as_deref(),
                    false,
                )?;
                let operation =
                    required_operation(prepared.operations.as_slice(), COMMAND_INVOKE_OPERATION)?;
                let result = self
                    .invoke_prepared_local_command(
                        immutable,
                        runtime_session_id,
                        owner_user_id,
                        target.device_id.as_str(),
                        target.workspace_id.as_deref(),
                        target.project_root.as_deref(),
                        prepared.adapter_session_id.as_str(),
                        operation.as_str(),
                    )
                    .await?;
                (
                    operation,
                    vec![command_tool_definition(immutable)],
                    plugin_result_instruction_items(&result),
                    Some(result),
                )
            }
            PluginComponentKind::Agent => {
                if prepared.agents.len() != 1 {
                    return Err(ProviderCallError::invalid_response(
                        "Plugin Agent catalog prepare must return exactly one agent",
                    ));
                }
                validate_agent_snapshot(immutable, &prepared.agents[0])?;
                let result = plugin_agent_result(immutable, &prepared.agents[0])?;
                (
                    required_operation(prepared.operations.as_slice(), AGENT_APPLY_OPERATION)?,
                    vec![agent_tool_definition(immutable)],
                    plugin_result_instruction_items(&result),
                    None,
                )
            }
            _ => unreachable!("validated Plugin tool component kind"),
        };
        Ok(PluginLocalToolComponentBinding {
            runtime: immutable.clone(),
            run_id: runtime_session_id.to_string(),
            device_id: target.device_id,
            workspace_id: target.workspace_id,
            adapter_session_id: prepared.adapter_session_id,
            operation,
            session_sha256: prepared.session_sha256,
            tools,
            instruction_items,
            static_result,
            expires_at_unix: prepared.expires_at,
        })
    }
}

impl PluginComponentProvider {
    #[allow(clippy::too_many_arguments)]
    async fn invoke_prepared_local_command(
        &self,
        immutable: &PluginToolComponentRuntimeBinding,
        runtime_session_id: &str,
        owner_user_id: &str,
        device_id: &str,
        workspace_id: Option<&str>,
        project_root: Option<&str>,
        adapter_session_id: &str,
        operation: &str,
    ) -> Result<Value, ProviderCallError> {
        let mut body = serde_json::Map::from_iter([
            ("run_id".to_string(), json!(runtime_session_id)),
            ("plugin_id".to_string(), json!(immutable.plugin_id)),
            ("release_id".to_string(), json!(immutable.release_id)),
            (
                "artifact_sha256".to_string(),
                json!(immutable.artifact_sha256),
            ),
            (
                "component_key".to_string(),
                json!(immutable.component.component_key),
            ),
            ("adapter_session_id".to_string(), json!(adapter_session_id)),
            ("operation".to_string(), json!(operation)),
        ]);
        if let Some(arguments) = immutable.command_arguments.as_deref() {
            body.insert("arguments".to_string(), json!(arguments));
        }
        let bytes = self
            .request_local(
                owner_user_id,
                device_id,
                workspace_id,
                project_root,
                "execute",
                Value::Object(body),
            )
            .await?;
        let response: Value = serde_json::from_slice(bytes.as_slice()).map_err(|error| {
            ProviderCallError::invalid_response(format!(
                "Plugin Command invocation returned invalid JSON: {error}"
            ))
        })?;
        for (field, expected) in [
            ("plugin_id", immutable.plugin_id.as_str()),
            ("release_id", immutable.release_id.as_str()),
            ("version", immutable.version.as_str()),
            ("artifact_sha256", immutable.artifact_sha256.as_str()),
            ("component_key", immutable.component.component_key.as_str()),
            ("adapter_session_id", adapter_session_id),
            ("operation", operation),
        ] {
            if response.get(field).and_then(Value::as_str) != Some(expected) {
                return Err(ProviderCallError::invalid_response(format!(
                    "Plugin Command invocation response {field} does not match its prepared binding"
                )));
            }
        }
        let command = response.pointer("/result/command").ok_or_else(|| {
            ProviderCallError::invalid_response(
                "Plugin Command invocation response is missing result.command",
            )
        })?;
        validate_command_snapshot(
            immutable,
            command,
            immutable.command_arguments.as_deref(),
            true,
        )?;
        plugin_command_result(immutable, command, immutable.command_arguments.as_deref())
    }
}

fn plugin_result_instruction_items(result: &Value) -> Vec<Value> {
    result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .map(plugin_message_item)
        .collect()
}

fn plugin_message_item(text: &str) -> Value {
    json!({
        "type": "message",
        "role": "system",
        "content": [{"type": "input_text", "text": text}]
    })
}
