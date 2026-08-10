// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{
    McpProviderKind, ProjectExecutionContext, ResolvedMcpRoute, WorkspaceProviderKind,
};
use chatos_plugin_management_sdk::PluginComponentKind;
use serde_json::{json, Value};

use super::validation::*;
use super::{
    PluginComponentProvider, PluginPrepareResponse, AGENT_APPLY_OPERATION,
    COMMAND_INVOKE_OPERATION, NATIVE_SKILL_TOOL_CALL_OPERATION,
};
use crate::providers::ProviderCallError;
use crate::runtime::{PluginLocalToolComponentBinding, PluginToolComponentRuntimeBinding};

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
        if context.workspace_provider != WorkspaceProviderKind::LocalConnector {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin Local tool component requires a Local Connector project workspace",
            ));
        }
        let workspace = context.workspace.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Plugin Local tool component is missing its project workspace snapshot",
            )
        })?;
        let device_id = workspace
            .device_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Plugin Local tool component is missing its device id",
                )
            })?;
        if immutable.installation_device_id.as_deref() != Some(device_id) {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin tool component installation is not pinned to the Project Context device",
            ));
        }
        let workspace_id = workspace.workspace_id.trim();
        if workspace_id.is_empty() {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin Local tool component is missing its workspace id",
            ));
        }
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
                body.insert(
                    "skill_keys".to_string(),
                    json!([immutable.component.component_key]),
                );
                body.insert(
                    "runtime_kind".to_string(),
                    json!(immutable.component.runtime_kind),
                );
                body.insert(
                    "runtime_metadata".to_string(),
                    Value::Object(immutable.component.metadata.clone().into_iter().collect()),
                );
            }
            PluginComponentKind::Command | PluginComponentKind::Agent => {
                body.insert("catalog_only".to_string(), json!(true));
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
                device_id,
                workspace_id,
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
        let (operation, tools) = match immutable.component.kind {
            PluginComponentKind::SkillCollection => {
                let native_skill = prepared.native_skill.as_ref().ok_or_else(|| {
                    ProviderCallError::invalid_response(
                        "native Plugin Skill prepare response is missing native_skill",
                    )
                })?;
                validate_native_skill_snapshot(immutable, native_skill)?;
                let operation = required_operation(
                    prepared.operations.as_slice(),
                    NATIVE_SKILL_TOOL_CALL_OPERATION,
                )?;
                let tools = native_skill
                    .get("tools")
                    .and_then(Value::as_array)
                    .cloned()
                    .ok_or_else(|| {
                        ProviderCallError::invalid_response(
                            "native Plugin Skill prepare response is missing tools",
                        )
                    })?;
                validate_tool_snapshot(tools.as_slice())?;
                validate_native_tool_snapshot_hash(native_skill, tools.as_slice())?;
                (operation, tools)
            }
            PluginComponentKind::Command => {
                if prepared.commands.len() != 1 {
                    return Err(ProviderCallError::invalid_response(
                        "Plugin Command catalog prepare must return exactly one command",
                    ));
                }
                validate_command_snapshot(immutable, &prepared.commands[0], None, false)?;
                (
                    required_operation(prepared.operations.as_slice(), COMMAND_INVOKE_OPERATION)?,
                    vec![command_tool_definition(immutable)],
                )
            }
            PluginComponentKind::Agent => {
                if prepared.agents.len() != 1 {
                    return Err(ProviderCallError::invalid_response(
                        "Plugin Agent catalog prepare must return exactly one agent",
                    ));
                }
                validate_agent_snapshot(immutable, &prepared.agents[0])?;
                (
                    required_operation(prepared.operations.as_slice(), AGENT_APPLY_OPERATION)?,
                    vec![agent_tool_definition(immutable)],
                )
            }
            _ => unreachable!("validated Plugin tool component kind"),
        };
        Ok(PluginLocalToolComponentBinding {
            runtime: immutable.clone(),
            run_id: runtime_session_id.to_string(),
            device_id: device_id.to_string(),
            workspace_id: workspace_id.to_string(),
            adapter_session_id: prepared.adapter_session_id,
            operation,
            session_sha256: prepared.session_sha256,
            tools,
            expires_at_unix: prepared.expires_at,
        })
    }
}
