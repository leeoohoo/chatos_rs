// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_mcp_service::MCP_ERROR_AUTH_REQUIRED;
use chatos_plugin_management_sdk::PluginComponentKind;
use serde_json::{json, Value};

use super::result::*;
use super::validation::*;
use super::{
    PluginComponentProvider, AGENT_TOOL_NAME, COMMAND_TOOL_NAME, LOCAL_SKILL_APPLY_OPERATION,
};
use crate::providers::{ProviderCallError, ProviderCallOutcome};
use crate::runtime::{PluginLocalToolComponentBinding, RuntimeSessionSnapshot};

impl PluginComponentProvider {
    pub(super) async fn call_local(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let binding = snapshot
            .plugin_local_tool_component_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Plugin Local tool component binding is missing",
                )
            })?;
        validate_local_bound_route(snapshot, route, binding)?;
        if !binding.publishes_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "tool is not published by the immutable Plugin component snapshot"
                    .to_string(),
            });
        }
        if binding.operation == LOCAL_SKILL_APPLY_OPERATION {
            ensure_expected_tool(original_tool_name, "apply")?;
            validate_empty_arguments(&arguments, "Plugin Skill apply")?;
            let result = binding.static_result.clone().ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "prepared Plugin Skill has no static instruction result",
                )
            })?;
            let response_bytes = serde_json::to_vec(&result)
                .map_err(|error| ProviderCallError::invalid_response(error.to_string()))?
                .len();
            return Ok(ProviderCallOutcome {
                result,
                response_bytes,
            });
        }
        if binding.runtime.component.kind == PluginComponentKind::Command {
            ensure_expected_tool(original_tool_name, COMMAND_TOOL_NAME)?;
            let command_arguments = parse_command_arguments(arguments)?;
            if command_arguments != binding.runtime.command_arguments {
                return Err(ProviderCallError {
                    code: MCP_ERROR_AUTH_REQUIRED,
                    message: "Plugin Command arguments do not match the Runtime Session selection"
                        .to_string(),
                });
            }
            let result = binding.static_result.clone().ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "prepared Plugin Command has no approved instruction result",
                )
            })?;
            let response_bytes = serde_json::to_vec(&result)
                .map_err(|error| ProviderCallError::invalid_response(error.to_string()))?
                .len();
            return Ok(ProviderCallOutcome {
                result,
                response_bytes,
            });
        }
        let mut body = serde_json::Map::from_iter([
            ("run_id".to_string(), json!(binding.run_id)),
            ("plugin_id".to_string(), json!(binding.runtime.plugin_id)),
            ("release_id".to_string(), json!(binding.runtime.release_id)),
            (
                "artifact_sha256".to_string(),
                json!(binding.runtime.artifact_sha256),
            ),
            (
                "component_key".to_string(),
                json!(binding.runtime.component.component_key),
            ),
            (
                "adapter_session_id".to_string(),
                json!(binding.adapter_session_id),
            ),
            ("operation".to_string(), json!(binding.operation)),
        ]);
        if let Some(max_chars) = snapshot.tool_result_max_chars {
            body.insert("tool_result_max_chars".to_string(), json!(max_chars.max(1)));
        }
        match binding.runtime.component.kind {
            PluginComponentKind::SkillCollection => {
                if !arguments.is_object() {
                    return Err(ProviderCallError::invalid_response(
                        "native Plugin Skill tool arguments must be an object",
                    ));
                }
                body.insert("tool_name".to_string(), json!(original_tool_name));
                body.insert("arguments".to_string(), arguments);
            }
            PluginComponentKind::Command => unreachable!("prepared commands return above"),
            PluginComponentKind::Agent => {
                ensure_expected_tool(original_tool_name, AGENT_TOOL_NAME)?;
                validate_empty_arguments(&arguments, "Plugin Agent apply")?;
                body.insert("arguments".to_string(), json!({}));
            }
            _ => {
                return Err(ProviderCallError::provider_unavailable(
                    "Plugin component kind is not callable",
                ))
            }
        }
        let bytes = self
            .request_local(
                snapshot.owner_user_id.as_str(),
                binding.device_id.as_str(),
                binding.workspace_id.as_str(),
                snapshot
                    .project_context
                    .workspace
                    .as_ref()
                    .and_then(|workspace| workspace.relative_root.as_deref()),
                "execute",
                Value::Object(body),
            )
            .await?;
        let response: Value = serde_json::from_slice(bytes.as_slice()).map_err(|error| {
            ProviderCallError::invalid_response(format!(
                "Plugin Local component execute returned invalid JSON: {error}"
            ))
        })?;
        validate_execute_identity(binding, &response)?;
        let result = response.get("result").ok_or_else(|| {
            ProviderCallError::invalid_response(
                "Plugin Local component execute response is missing result",
            )
        })?;
        let result = match binding.runtime.component.kind {
            PluginComponentKind::SkillCollection => result.clone(),
            PluginComponentKind::Command => unreachable!("prepared commands return above"),
            PluginComponentKind::Agent => {
                let agent = result.get("agent").ok_or_else(|| {
                    ProviderCallError::invalid_response(
                        "Plugin Agent invocation response is missing agent",
                    )
                })?;
                validate_agent_snapshot(&binding.runtime, agent)?;
                plugin_agent_result(&binding.runtime, agent)?
            }
            _ => unreachable!("validated local Plugin component kind"),
        };
        Ok(ProviderCallOutcome {
            result,
            response_bytes: bytes.len(),
        })
    }

    pub(in crate::providers) async fn close_local_bindings(
        &self,
        owner_user_id: &str,
        runtime_session_id: &str,
        bindings: &HashMap<String, PluginLocalToolComponentBinding>,
    ) {
        for binding in bindings.values() {
            let body = json!({
                "run_id": binding.run_id,
                "plugin_id": binding.runtime.plugin_id,
                "release_id": binding.runtime.release_id,
                "artifact_sha256": binding.runtime.artifact_sha256,
                "component_key": binding.runtime.component.component_key,
                "adapter_session_id": binding.adapter_session_id,
            });
            if let Err(error) = self
                .request_local(
                    owner_user_id,
                    binding.device_id.as_str(),
                    binding.workspace_id.as_str(),
                    None,
                    "cancel",
                    body,
                )
                .await
            {
                tracing::warn!(
                    session_id = runtime_session_id,
                    resource_id = binding.runtime.resource_id.as_str(),
                    error = error.message,
                    "close Plugin Local tool component session failed"
                );
            }
        }
    }
}
