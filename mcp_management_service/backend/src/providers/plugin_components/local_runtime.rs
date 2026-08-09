// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::{
    McpProviderKind, ProjectExecutionContext, ResolvedMcpRoute, WorkspaceProviderKind,
};
use chatos_mcp_service::MCP_ERROR_AUTH_REQUIRED;
use chatos_plugin_management_sdk::PluginComponentKind;
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde_json::{json, Value};

use super::result::*;
use super::validation::*;
use super::{
    PluginComponentProvider, PluginPrepareResponse, ProviderCallError, ProviderCallOutcome,
    AGENT_APPLY_OPERATION, AGENT_TOOL_NAME, CALLER_SERVICE, COMMAND_INVOKE_OPERATION,
    COMMAND_TOOL_NAME, NATIVE_SKILL_TOOL_CALL_OPERATION, PLUGIN_RELAY_SCOPE, TOKEN_AUDIENCE,
};
use crate::runtime::{
    PluginLocalToolComponentBinding, PluginToolComponentRuntimeBinding, RuntimeSessionSnapshot,
};
use crate::trace_context::InternalTraceContextExt;

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
                ))
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
        let mut invoked_command_arguments = None;
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
            PluginComponentKind::Command => {
                ensure_expected_tool(original_tool_name, COMMAND_TOOL_NAME)?;
                let command_arguments = parse_command_arguments(arguments)?;
                invoked_command_arguments = command_arguments.clone();
                if let Some(arguments) = command_arguments.as_deref() {
                    body.insert("arguments".to_string(), json!(arguments));
                }
            }
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
            PluginComponentKind::Command => {
                let command = result.get("command").ok_or_else(|| {
                    ProviderCallError::invalid_response(
                        "Plugin Command invocation response is missing command",
                    )
                })?;
                let arguments_sha256 = command
                    .get("arguments_sha256")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        ProviderCallError::invalid_response(
                            "Plugin Command invocation response is missing arguments_sha256",
                        )
                    })?;
                let expected_arguments_sha256 =
                    sha256_text(invoked_command_arguments.as_deref().unwrap_or_default());
                if arguments_sha256 != expected_arguments_sha256 {
                    return Err(ProviderCallError::invalid_response(
                        "Plugin Command invocation response arguments hash does not match the MCP call",
                    ));
                }
                validate_command_snapshot(
                    &binding.runtime,
                    command,
                    invoked_command_arguments.as_deref(),
                    true,
                )?;
                plugin_command_result(
                    &binding.runtime,
                    command,
                    invoked_command_arguments.as_deref(),
                )?
            }
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

    async fn request_local(
        &self,
        owner_user_id: &str,
        device_id: &str,
        workspace_id: &str,
        action: &str,
        body: Value,
    ) -> Result<Vec<u8>, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Plugin Component Provider internal secret is not configured",
            )
        })?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            PLUGIN_RELAY_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let mut url = reqwest::Url::parse(
            format!(
                "{}/api/local-connectors/relay/{}/plugins/{action}",
                self.base_url,
                urlencoding::encode(device_id)
            )
            .as_str(),
        )
        .map_err(|error| {
            ProviderCallError::provider_unavailable(format!(
                "build Plugin Component Provider URL failed: {error}"
            ))
        })?;
        url.query_pairs_mut()
            .append_pair("workspace_id", workspace_id);
        let response = self
            .http
            .post(url)
            .header("x-local-connector-caller", CALLER_SERVICE)
            .header("x-local-connector-internal-token", token)
            .header("x-local-connector-owner-user-id", owner_user_id)
            .with_internal_trace_context()
            .json(&body)
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Plugin Component Provider request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Plugin Component Provider response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Plugin Component Provider rejected {action} with HTTP {}",
                status.as_u16()
            )));
        }
        Ok(bytes.to_vec())
    }
}
