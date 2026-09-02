// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_management_sdk::{ProjectExecutionContext, ResolvedMcpRoute};
use chatos_mcp_service::MCP_ERROR_AUTH_REQUIRED;
use serde_json::{json, Value};

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
