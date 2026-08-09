// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use chatos_mcp_service::{MCP_ERROR_AUTH_REQUIRED, METHOD_TOOLS_CALL};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde_json::{json, Value};

use super::prepare::{extract_tool_snapshot, CloudStdioRequestContext};
use super::{
    CloudStdioCallRequest, CloudStdioCallResponse, CloudStdioCancelRequest,
    CloudStdioCancelResponse, CloudStdioCloseRequest, CloudStdioProvider,
    CloudStdioProviderBinding, ProviderCallError, ProviderCallOutcome, ProviderCancelOutcome,
    ResolvedMcpRoute, RuntimeSessionSnapshot, SandboxExecutionTarget,
};

impl CloudStdioProvider {
    pub(super) async fn list_tools(
        &self,
        target: &SandboxExecutionTarget,
        context: &CloudStdioRequestContext<'_>,
        resource_id: &str,
        binding: &CloudStdioProviderBinding,
    ) -> Result<Vec<Value>, ProviderCallError> {
        let body = CloudStdioCallRequest {
            runtime_session_id: context.runtime_session_id,
            resource_id,
            invocation_id: None,
            command: binding.command.as_str(),
            args: binding.args.as_slice(),
            env: &binding.env,
            cwd: binding.cwd.as_deref(),
            plugin_artifact: binding.plugin_artifact.as_ref(),
            plugin_workspace_write: binding.plugin_artifact.is_some() && binding.allow_writes,
            method: "tools/list",
            params: json!({}),
            expires_at_unix: context.expires_at_unix,
            timeout_ms: self.request_timeout.as_millis().min(u128::from(u64::MAX)) as u64,
        };
        let response = self.request(target, context, "call", &body).await?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud stdio MCP tools/list response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Cloud stdio MCP tools/list returned HTTP {}",
                status.as_u16()
            )));
        }
        let response = serde_json::from_slice::<CloudStdioCallResponse>(bytes.as_slice()).map_err(
            |error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud stdio MCP tools/list returned an invalid response: {error}"
                ))
            },
        )?;
        extract_tool_snapshot(response.result).map_err(ProviderCallError::invalid_response)
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::providers) async fn list_plugin_tools(
        &self,
        target: &SandboxExecutionTarget,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
        resource_id: &str,
        binding: &CloudStdioProviderBinding,
    ) -> Result<Vec<Value>, ProviderCallError> {
        let context = CloudStdioRequestContext {
            runtime_session_id,
            owner_user_id,
            project_id,
            run_id,
            expires_at_unix,
        };
        self.list_tools(target, &context, resource_id, binding)
            .await
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
            .cloud_stdio_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Cloud stdio MCP runtime binding is missing",
                )
            })?;
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Cloud stdio MCP runtime session has no sandbox target",
            )
        })?;
        if !self.supports(route)
            || route.provider_ref.as_deref() != Some(binding.provider_ref.as_str())
            || binding.provider_ref != target.provider_ref()
            || route.allow_writes != binding.allow_writes
        {
            return Err(ProviderCallError::provider_unavailable(
                "Cloud stdio MCP route does not match its immutable runtime binding",
            ));
        }
        if !binding.allows_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "tool is blocked by the Cloud stdio MCP policy".to_string(),
            });
        }
        self.call_bound_tool(
            snapshot,
            route.resource_id.as_str(),
            binding,
            original_tool_name,
            arguments,
            invocation_id,
        )
        .await
    }

    pub(in crate::providers) async fn call_bound_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        resource_id: &str,
        binding: &CloudStdioProviderBinding,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Cloud stdio MCP runtime session has no sandbox target",
            )
        })?;
        if !binding.allows_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "tool is blocked by the Cloud stdio MCP policy".to_string(),
            });
        }
        let body = CloudStdioCallRequest {
            runtime_session_id: snapshot.session_id.as_str(),
            resource_id,
            invocation_id: Some(invocation_id),
            command: binding.command.as_str(),
            args: binding.args.as_slice(),
            env: &binding.env,
            cwd: binding.cwd.as_deref(),
            plugin_artifact: binding.plugin_artifact.as_ref(),
            plugin_workspace_write: binding.plugin_artifact.is_some() && binding.allow_writes,
            method: METHOD_TOOLS_CALL,
            params: json!({
                "name": original_tool_name,
                "arguments": arguments,
            }),
            expires_at_unix: snapshot.expires_at_unix,
            timeout_ms: self.request_timeout.as_millis().min(u128::from(u64::MAX)) as u64,
        };
        let context = CloudStdioRequestContext::from_snapshot(snapshot);
        let response = self.request(target, &context, "call", &body).await?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud stdio MCP response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Cloud stdio MCP runner returned HTTP {}",
                status.as_u16()
            )));
        }
        let response = serde_json::from_slice::<CloudStdioCallResponse>(bytes.as_slice()).map_err(
            |error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud stdio MCP runner returned an invalid response: {error}"
                ))
            },
        )?;
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
            .cloud_stdio_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Cloud stdio MCP runtime binding is missing",
                )
            })?;
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Cloud stdio MCP runtime session has no sandbox target",
            )
        })?;
        if !self.supports(route)
            || route.provider_ref.as_deref() != Some(binding.provider_ref.as_str())
            || binding.provider_ref != target.provider_ref()
            || route.allow_writes != binding.allow_writes
        {
            return Err(ProviderCallError::provider_unavailable(
                "Cloud stdio MCP route does not match its immutable runtime binding",
            ));
        }
        self.cancel_bound_invocation(snapshot, route.resource_id.as_str(), invocation_id)
            .await
    }

    pub(in crate::providers) async fn cancel_bound_invocation(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        resource_id: &str,
        invocation_id: &str,
    ) -> Result<ProviderCancelOutcome, ProviderCallError> {
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Cloud stdio MCP runtime session has no sandbox target",
            )
        })?;
        let body = CloudStdioCancelRequest {
            runtime_session_id: snapshot.session_id.as_str(),
            resource_id,
            invocation_id,
        };
        let context = CloudStdioRequestContext::from_snapshot(snapshot);
        let response = self.request(target, &context, "cancel", &body).await?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud stdio MCP cancellation response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Cloud stdio MCP cancellation returned HTTP {}",
                status.as_u16()
            )));
        }
        let response = serde_json::from_slice::<CloudStdioCancelResponse>(bytes.as_slice())
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud stdio MCP cancellation returned invalid JSON: {error}"
                ))
            })?;
        match response.status.trim() {
            "cancelled" => Ok(ProviderCancelOutcome::Cancelled),
            "cancel_requested" | "already_completed" | "invocation_not_found" => {
                Ok(ProviderCancelOutcome::CancelRequested)
            }
            other => Err(ProviderCallError::invalid_response(format!(
                "Cloud stdio MCP cancellation returned invalid status: {other}"
            ))),
        }
    }

    pub(in crate::providers) async fn close_session(&self, snapshot: &RuntimeSessionSnapshot) {
        let Some(target) = snapshot.sandbox_target.as_ref() else {
            return;
        };
        self.close_bindings(
            target,
            snapshot.session_id.as_str(),
            snapshot.owner_user_id.as_str(),
            snapshot.project_id.as_str(),
            snapshot.run_id.as_deref(),
            snapshot.expires_at_unix,
            &snapshot.cloud_stdio_bindings,
        )
        .await;
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::providers) async fn close_bindings(
        &self,
        target: &SandboxExecutionTarget,
        runtime_session_id: &str,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        expires_at_unix: i64,
        bindings: &HashMap<String, CloudStdioProviderBinding>,
    ) {
        let context = CloudStdioRequestContext {
            runtime_session_id,
            owner_user_id,
            project_id,
            run_id,
            expires_at_unix,
        };
        for resource_id in bindings.keys() {
            let body = CloudStdioCloseRequest {
                runtime_session_id,
                resource_id: resource_id.as_str(),
            };
            if let Err(error) = self.request(target, &context, "close", &body).await {
                tracing::warn!(
                    session_id = runtime_session_id,
                    resource_id = resource_id.as_str(),
                    error = error.message,
                    "close Cloud stdio MCP session failed"
                );
            }
        }
    }
}
