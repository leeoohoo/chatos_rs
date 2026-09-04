// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use chatos_mcp_management_sdk::{ProjectExecutionContext, ResolvedMcpRoute};
use chatos_mcp_service::MCP_ERROR_AUTH_REQUIRED;
use chatos_plugin_management_sdk::{SkillActivationAttestationClaims, SkillGateDeclaration};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::validation::{validate_bound_route, validate_prepare_response};
use super::{
    PluginCancelResponse, PluginExecuteResponse, PluginLocalProvider, PluginPrepareResponse,
    MCP_TOOL_CALL_OPERATION,
};
use crate::providers::{ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome};
use crate::runtime::{
    resolve_plugin_local_execution_target, PluginLocalProviderBinding, PluginMcpRuntimeBinding,
    RuntimeSessionSnapshot,
};

impl PluginLocalProvider {
    fn recovered_binding_key(
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
    ) -> String {
        format!("{}\n{}", snapshot.session_id, route.resource_id)
    }

    async fn effective_binding(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        prepared: &PluginLocalProviderBinding,
    ) -> PluginLocalProviderBinding {
        self.recovered_bindings
            .read()
            .await
            .get(Self::recovered_binding_key(snapshot, route).as_str())
            .cloned()
            .unwrap_or_else(|| prepared.clone())
    }

    async fn recover_binding(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        failed_binding: &PluginLocalProviderBinding,
    ) -> Result<PluginLocalProviderBinding, ProviderCallError> {
        let _guard = self.recovery_lock.lock().await;
        let prepared = snapshot
            .plugin_local_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable("Plugin Local runtime binding is missing")
            })?;
        let current = self.effective_binding(snapshot, route, prepared).await;
        if current.adapter_session_id != failed_binding.adapter_session_id {
            return Ok(current);
        }
        let immutable = snapshot
            .plugin_mcp_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "immutable Plugin MCP runtime binding is missing",
                )
            })?;
        let recovered = self
            .prepare_route(
                immutable,
                route,
                &snapshot.project_context,
                snapshot.session_id.as_str(),
                snapshot.owner_user_id.as_str(),
                snapshot.expires_at_unix,
            )
            .await?;
        validate_recovered_binding(prepared, &recovered)?;
        self.recovered_bindings.write().await.insert(
            Self::recovered_binding_key(snapshot, route),
            recovered.clone(),
        );
        tracing::info!(
            session_id = snapshot.session_id.as_str(),
            resource_id = route.resource_id.as_str(),
            previous_adapter_session_id = failed_binding.adapter_session_id.as_str(),
            adapter_session_id = recovered.adapter_session_id.as_str(),
            "recovered Plugin Local runtime binding"
        );
        Ok(recovered)
    }

    pub(super) async fn prepare_route(
        &self,
        immutable: &PluginMcpRuntimeBinding,
        route: &ResolvedMcpRoute,
        context: &ProjectExecutionContext,
        runtime_session_id: &str,
        owner_user_id: &str,
        expires_at_unix: i64,
    ) -> Result<PluginLocalProviderBinding, ProviderCallError> {
        if !self.supports(route)
            || route.provider_ref.as_deref() != Some(immutable.provider_ref.as_str())
            || route.allow_writes != immutable.allow_writes
            || route.resource_id != immutable.resource_id
        {
            return Err(ProviderCallError::provider_unavailable(
                "Plugin Local route does not match its immutable binding",
            ));
        }
        let target = resolve_plugin_local_execution_target(
            context,
            immutable.installation_device_id.as_deref(),
            immutable.permission_snapshot.as_slice(),
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
            ("component_key".to_string(), json!(immutable.component_key)),
            (
                "permission_snapshot".to_string(),
                json!(immutable.permission_snapshot),
            ),
            (
                "tool_allowlist".to_string(),
                json!(immutable.tool_allowlist),
            ),
            (
                "tool_blocklist".to_string(),
                json!(immutable.tool_blocklist),
            ),
        ]);
        body.insert("project_id".to_string(), json!(context.project_id));
        if let Some(server_key) = immutable.server_key.as_deref() {
            body.insert("server_key".to_string(), json!(server_key));
        }
        let response = self
            .request(
                owner_user_id,
                target.device_id.as_str(),
                target.workspace_id.as_deref(),
                target.project_root.as_deref(),
                "prepare",
                Value::Object(body),
            )
            .await?;
        let prepared = serde_json::from_slice::<PluginPrepareResponse>(response.as_slice())
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Plugin Local prepare returned invalid JSON: {error}"
                ))
            })?;
        validate_prepare_response(immutable, runtime_session_id, expires_at_unix, &prepared)?;
        let operation = prepared
            .operations
            .iter()
            .map(String::as_str)
            .find(|operation| *operation == MCP_TOOL_CALL_OPERATION)
            .ok_or_else(|| {
                ProviderCallError::invalid_response(
                    "Plugin Local prepare did not publish the MCP tool call operation",
                )
            })?;
        Ok(PluginLocalProviderBinding {
            runtime: immutable.clone(),
            run_id: runtime_session_id.to_string(),
            device_id: target.device_id,
            workspace_id: target.workspace_id,
            project_id: context.project_id.clone(),
            adapter_session_id: prepared.adapter_session_id,
            operation: operation.to_string(),
            session_sha256: prepared.session_sha256,
            snapshot_sha256: prepared.mcp.snapshot_sha256,
            tool_snapshot_sha256: prepared.mcp.tool_snapshot_sha256,
            server_instructions_sha256: prepared.mcp.server_instructions_sha256,
            server_instructions: prepared.mcp.server_instructions,
            tools: prepared.mcp.tools,
            oauth_connection_id: prepared.mcp.oauth_connection_id,
            expires_at_unix: prepared.expires_at,
        })
    }

    pub(in crate::providers) async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let prepared_binding = snapshot
            .plugin_local_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable("Plugin Local runtime binding is missing")
            })?;
        validate_bound_route(snapshot, route, prepared_binding)?;
        if !prepared_binding.publishes_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "tool is not published by the immutable Plugin MCP snapshot".to_string(),
            });
        }
        let binding = self
            .effective_binding(snapshot, route, prepared_binding)
            .await;
        validate_bound_route(snapshot, route, &binding)?;
        let arguments = self
            .apply_skill_gate(snapshot, prepared_binding, original_tool_name, arguments)
            .await?;
        let first = self
            .execute_tool_with_binding(
                snapshot,
                &binding,
                original_tool_name,
                arguments.clone(),
                invocation_id,
            )
            .await;
        match first {
            Ok(outcome) => Ok(outcome),
            Err(error) if is_recoverable_adapter_session_error(&error) => {
                let recovered = self.recover_binding(snapshot, route, &binding).await?;
                self.execute_tool_with_binding(
                    snapshot,
                    &recovered,
                    original_tool_name,
                    arguments,
                    invocation_id,
                )
                .await
            }
            Err(error) => Err(error),
        }
    }

    async fn apply_skill_gate(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        binding: &PluginLocalProviderBinding,
        tool_name: &str,
        mut arguments: Value,
    ) -> Result<Value, ProviderCallError> {
        let definition = binding
            .tools
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some(tool_name))
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Plugin MCP tool definition is missing from the immutable snapshot",
                )
            })?;
        let Some(raw_gate) = definition
            .get("_meta")
            .and_then(Value::as_object)
            .and_then(|meta| meta.get("chatos/skillGate"))
        else {
            return Ok(arguments);
        };
        let gate: SkillGateDeclaration =
            serde_json::from_value(raw_gate.clone()).map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Plugin MCP tool has an invalid chatos/skillGate declaration: {error}"
                ))
            })?;
        let evidence_argument = gate.evidence_argument.trim();
        if evidence_argument.is_empty()
            || (gate.all_of.is_empty() && gate.select_by_argument.is_none())
        {
            return Err(ProviderCallError::invalid_response(
                "Plugin MCP skill gate must declare an evidence argument and at least one required Skill",
            ));
        }
        let object = arguments.as_object_mut().ok_or_else(|| {
            ProviderCallError::invalid_response(
                "Plugin MCP tool arguments must be an object when a Skill gate is declared",
            )
        })?;
        let evidence_value = object
            .remove(evidence_argument)
            .ok_or_else(|| ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: format!(
                    "Plugin tool requires active Skill evidence in argument {evidence_argument}"
                ),
            })?;
        let tokens = match evidence_value {
            Value::String(token) => vec![token],
            Value::Array(values) => values
                .into_iter()
                .map(|value| {
                    value.as_str().map(str::to_string).ok_or_else(|| {
                        ProviderCallError::invalid_response(
                            "Plugin Skill evidence array must contain only strings",
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => {
                return Err(ProviderCallError::invalid_response(
                    "Plugin Skill evidence must be a token string or an array of token strings",
                ))
            }
        };
        if tokens.is_empty() || tokens.len() > 16 {
            return Err(ProviderCallError::invalid_response(
                "Plugin Skill evidence must contain between 1 and 16 tokens",
            ));
        }
        let mut required = gate
            .all_of
            .iter()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect::<HashSet<_>>();
        if let Some(selector) = gate.select_by_argument {
            let selected = arguments
                .pointer(selector.pointer.as_str())
                .ok_or_else(|| {
                    ProviderCallError::invalid_response(format!(
                        "Plugin Skill gate selector argument is missing: {}",
                        selector.pointer
                    ))
                })?;
            let selected = selected.as_str().ok_or_else(|| {
                ProviderCallError::invalid_response(
                    "Plugin Skill gate selector value must be a string",
                )
            })?;
            let skill_name = selector.map.get(selected).ok_or_else(|| {
                ProviderCallError::invalid_response(format!(
                    "Plugin Skill gate has no mapping for selector value {selected}"
                ))
            })?;
            required.insert(skill_name.clone());
        }
        let mut activated_names = HashSet::new();
        for token in tokens {
            let activation = self
                .skill_attestations
                .verify_active(token.trim())
                .await
                .map_err(ProviderCallError::provider_unavailable)?;
            self.validate_gated_activation(snapshot, binding, &activation.claims)?;
            if !activated_names.insert(activation.claims.skill_name.clone()) {
                return Err(ProviderCallError::invalid_response(
                    "Plugin Skill evidence contains a duplicate Skill activation",
                ));
            }
        }
        let mut missing = required
            .difference(&activated_names)
            .cloned()
            .collect::<Vec<_>>();
        missing.sort();
        if !missing.is_empty() {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: format!(
                    "Plugin tool requires activated Skills: {}",
                    missing.join(", ")
                ),
            });
        }
        Ok(arguments)
    }

    fn validate_gated_activation(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        binding: &PluginLocalProviderBinding,
        claims: &SkillActivationAttestationClaims,
    ) -> Result<(), ProviderCallError> {
        let skill_binding = snapshot
            .plugin_local_tool_component_bindings
            .values()
            .find(|candidate| {
                candidate.runtime.plugin_id == claims.plugin_id
                    && candidate.runtime.release_id == claims.release_id
                    && candidate.runtime.component.component_key == claims.component_key
            })
            .ok_or_else(|| ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "Plugin Skill evidence refers to a component outside this Runtime Session"
                    .to_string(),
            })?;
        let skill = skill_binding
            .runtime
            .skill_snapshot
            .as_ref()
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable("Plugin Skill v2 snapshot is missing")
            })?;
        let (scope_kind, scope_material) = if let Some(project_id) = snapshot.project_id.as_deref()
        {
            (
                "project",
                format!(
                    "project\n{}\n{}\n{}",
                    snapshot.tenant_id, snapshot.owner_user_id, project_id
                ),
            )
        } else {
            (
                "user_public",
                format!(
                    "user_public\n{}\n{}\n{}",
                    snapshot.tenant_id, snapshot.owner_user_id, binding.device_id
                ),
            )
        };
        let scope_id = hex::encode(Sha256::digest(scope_material.as_bytes()));
        if claims.tenant_id != snapshot.tenant_id
            || claims.owner_user_id != snapshot.owner_user_id
            || claims.task_id != snapshot.task_id
            || claims.run_id != snapshot.run_id
            || claims.runtime_session_id != snapshot.session_id
            || claims.scope_kind != scope_kind
            || claims.scope_id != scope_id
            || claims.device_id.as_deref() != Some(binding.device_id.as_str())
            || claims.workspace_id != binding.workspace_id
            || claims.plugin_id != binding.runtime.plugin_id
            || claims.release_id != binding.runtime.release_id
            || claims.skill_name != skill.metadata.name
            || claims.instructions_sha256 != skill.instructions_sha256
            || claims.resource_manifest_sha256 != skill.resource_manifest_sha256
        {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "Plugin Skill evidence does not match the target Plugin Runtime Session"
                    .to_string(),
            });
        }
        Ok(())
    }

    async fn execute_tool_with_binding(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        binding: &PluginLocalProviderBinding,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let body = json!({
            "run_id": binding.run_id,
            "plugin_id": binding.runtime.plugin_id,
            "release_id": binding.runtime.release_id,
            "artifact_sha256": binding.runtime.artifact_sha256,
            "component_key": binding.runtime.component_key,
            "adapter_session_id": binding.adapter_session_id,
            "invocation_id": invocation_id,
            "operation": binding.operation,
            "tool_name": original_tool_name,
            "arguments": arguments,
            "tool_result_max_chars": snapshot.tool_result_max_chars,
            "conversation_id": snapshot.source_session_id,
            "conversation_turn_id": snapshot.turn_id,
            "source_user_message_id": snapshot.source_user_message_id,
            "task_id": snapshot.task_id,
            "task_run_id": snapshot.run_id,
            "task_title": snapshot.task_title,
            "project_id": snapshot.project_context.project_id,
        });
        let bytes = self
            .request(
                snapshot.owner_user_id.as_str(),
                binding.device_id.as_str(),
                binding.workspace_id.as_deref(),
                snapshot
                    .project_context
                    .workspace
                    .as_ref()
                    .and_then(|workspace| workspace.relative_root.as_deref()),
                "execute",
                body,
            )
            .await?;
        let response =
            serde_json::from_slice::<PluginExecuteResponse>(bytes.as_slice()).map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Plugin Local execute returned invalid JSON: {error}"
                ))
            })?;
        if response.plugin_id != binding.runtime.plugin_id
            || response.release_id != binding.runtime.release_id
            || response.version != binding.runtime.version
            || response.artifact_sha256 != binding.runtime.artifact_sha256
            || response.component_key != binding.runtime.component_key
            || response.invocation_id != invocation_id
            || response.tool_name != original_tool_name
            || response.adapter_session_id != binding.adapter_session_id
            || response.operation != binding.operation
        {
            return Err(ProviderCallError::invalid_response(
                "Plugin Local execute response does not match the immutable runtime binding",
            ));
        }
        Ok(ProviderCallOutcome {
            result: response.result,
            response_bytes: bytes.len(),
        })
    }

    pub(in crate::providers) async fn cancel_invocation(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        invocation_id: &str,
    ) -> Result<ProviderCancelOutcome, ProviderCallError> {
        let prepared_binding = snapshot
            .plugin_local_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable("Plugin Local runtime binding is missing")
            })?;
        validate_bound_route(snapshot, route, prepared_binding)?;
        let binding = self
            .effective_binding(snapshot, route, prepared_binding)
            .await;
        validate_bound_route(snapshot, route, &binding)?;
        let body = json!({
            "run_id": binding.run_id,
            "plugin_id": binding.runtime.plugin_id,
            "release_id": binding.runtime.release_id,
            "artifact_sha256": binding.runtime.artifact_sha256,
            "component_key": binding.runtime.component_key,
            "adapter_session_id": binding.adapter_session_id,
            "invocation_id": invocation_id,
            "project_id": snapshot.project_context.project_id,
        });
        let bytes = self
            .request(
                snapshot.owner_user_id.as_str(),
                binding.device_id.as_str(),
                binding.workspace_id.as_deref(),
                snapshot
                    .project_context
                    .workspace
                    .as_ref()
                    .and_then(|workspace| workspace.relative_root.as_deref()),
                "cancel",
                body,
            )
            .await?;
        let response =
            serde_json::from_slice::<PluginCancelResponse>(bytes.as_slice()).map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Plugin Local cancel returned invalid JSON: {error}"
                ))
            })?;
        if response.run_id != binding.run_id
            || response.adapter_session_id != binding.adapter_session_id
            || response.invocation_id != invocation_id
        {
            return Err(ProviderCallError::invalid_response(
                "Plugin Local cancel response does not match the immutable invocation binding",
            ));
        }
        match response.status.trim() {
            "cancelled" => Ok(ProviderCancelOutcome::Cancelled),
            "cancel_requested" | "invocation_not_found" | "already_completed" => {
                Ok(ProviderCancelOutcome::CancelRequested)
            }
            other => Err(ProviderCallError::invalid_response(format!(
                "Plugin Local cancel returned invalid status: {other}"
            ))),
        }
    }

    pub(in crate::providers) async fn close_session(&self, snapshot: &RuntimeSessionSnapshot) {
        let mut effective = snapshot.plugin_local_bindings.clone();
        let prefix = format!("{}\n", snapshot.session_id);
        let recovered = {
            let mut bindings = self.recovered_bindings.write().await;
            let keys = bindings
                .keys()
                .filter(|key| key.starts_with(prefix.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|key| bindings.remove(key.as_str()))
                .collect::<Vec<_>>()
        };
        for binding in recovered {
            effective.insert(binding.runtime.resource_id.clone(), binding);
        }
        self.close_bindings(
            snapshot.owner_user_id.as_str(),
            snapshot.session_id.as_str(),
            &effective,
        )
        .await;
    }

    pub(in crate::providers) async fn close_bindings(
        &self,
        owner_user_id: &str,
        runtime_session_id: &str,
        bindings: &HashMap<String, PluginLocalProviderBinding>,
    ) {
        for binding in bindings.values() {
            let body = json!({
                "run_id": binding.run_id,
                "plugin_id": binding.runtime.plugin_id,
                "release_id": binding.runtime.release_id,
                "artifact_sha256": binding.runtime.artifact_sha256,
                "component_key": binding.runtime.component_key,
                "adapter_session_id": binding.adapter_session_id,
                "project_id": binding.project_id,
            });
            if let Err(error) = self
                .request(
                    owner_user_id,
                    binding.device_id.as_str(),
                    binding.workspace_id.as_deref(),
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
                    "close Plugin Local runtime session failed"
                );
            }
        }
    }
}

pub(super) fn is_recoverable_adapter_session_error(error: &ProviderCallError) -> bool {
    error.message.contains("Plugin 本机会话不存在或已经结束")
        || error.message.contains("no active control subscriber")
}

fn validate_recovered_binding(
    prepared: &PluginLocalProviderBinding,
    recovered: &PluginLocalProviderBinding,
) -> Result<(), ProviderCallError> {
    if recovered.runtime != prepared.runtime
        || recovered.run_id != prepared.run_id
        || recovered.device_id != prepared.device_id
        || recovered.workspace_id != prepared.workspace_id
        || recovered.project_id != prepared.project_id
        || recovered.operation != prepared.operation
        || recovered.snapshot_sha256 != prepared.snapshot_sha256
        || recovered.tool_snapshot_sha256 != prepared.tool_snapshot_sha256
        || recovered.server_instructions_sha256 != prepared.server_instructions_sha256
        || recovered.server_instructions != prepared.server_instructions
        || recovered.tools != prepared.tools
        || recovered.oauth_connection_id != prepared.oauth_connection_id
    {
        return Err(ProviderCallError::invalid_response(
            "recovered Plugin Local binding changed its immutable MCP snapshot",
        ));
    }
    Ok(())
}
