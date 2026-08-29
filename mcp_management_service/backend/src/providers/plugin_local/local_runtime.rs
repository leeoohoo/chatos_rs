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
        let binding = snapshot
            .plugin_local_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable("Plugin Local runtime binding is missing")
            })?;
        validate_bound_route(snapshot, route, binding)?;
        if !binding.publishes_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "tool is not published by the immutable Plugin MCP snapshot".to_string(),
            });
        }
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
        let binding = snapshot
            .plugin_local_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable("Plugin Local runtime binding is missing")
            })?;
        validate_bound_route(snapshot, route, binding)?;
        let body = json!({
            "run_id": binding.run_id,
            "plugin_id": binding.runtime.plugin_id,
            "release_id": binding.runtime.release_id,
            "artifact_sha256": binding.runtime.artifact_sha256,
            "component_key": binding.runtime.component_key,
            "adapter_session_id": binding.adapter_session_id,
            "invocation_id": invocation_id,
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
        self.close_bindings(
            snapshot.owner_user_id.as_str(),
            snapshot.session_id.as_str(),
            &snapshot.plugin_local_bindings,
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
