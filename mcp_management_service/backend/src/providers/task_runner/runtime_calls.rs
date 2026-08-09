// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_mcp_service::{METHOD_NOTIFICATIONS_CANCELLED, METHOD_TOOLS_CALL};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde_json::{json, Value};

use crate::providers::{
    decode_cancel_notification_response, ProviderCallOutcome, ProviderCancelOutcome,
};
use crate::runtime::RuntimeSessionSnapshot;

use super::{
    decode_jsonrpc_response, ProviderCallError, TaskRunnerProvider, TaskRunnerRequestBinding,
    CALLER_SERVICE, TASK_RUNNER_MCP_SCOPE, TOKEN_AUDIENCE,
};

impl TaskRunnerProvider {
    pub(in crate::providers) async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Task Runner Provider internal secret is not configured",
            )
        })?;
        let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .filter(|descriptor| {
                matches!(
                    descriptor.key,
                    SystemMcpKey::TaskRunnerService
                        | SystemMcpKey::TaskProcessLog
                        | SystemMcpKey::AskUser
                ) && self.supports(route)
            })
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Task Runner route is not a supported System MCP",
                )
            })?;
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/{}",
            self.base_url,
            urlencoding::encode(descriptor.key.as_str())
        );
        let binding = TaskRunnerRequestBinding::from(snapshot);
        let response = self
            .bound_request(
                &binding,
                endpoint,
                if descriptor.key == SystemMcpKey::AskUser {
                    self.ask_user_request_timeout
                } else {
                    self.request_timeout
                },
                secret,
                TASK_RUNNER_MCP_SCOPE,
            )?
            .json(&json!({
                "jsonrpc": "2.0",
                "id": invocation_id,
                "method": METHOD_TOOLS_CALL,
                "params": {
                    "name": original_tool_name,
                    "arguments": arguments,
                }
            }))
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Task Runner Provider request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Task Runner Provider response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Task Runner Provider rejected the request with HTTP {}",
                status.as_u16()
            )));
        }
        let result =
            decode_jsonrpc_response(bytes.as_slice(), invocation_id, "Task Runner Provider")?;
        Ok(ProviderCallOutcome {
            result,
            response_bytes: bytes.len(),
        })
    }

    pub(in crate::providers) async fn cancel_invocation(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        invocation_id: &str,
    ) -> Result<ProviderCancelOutcome, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "Task Runner Provider internal secret is not configured",
            )
        })?;
        let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .filter(|descriptor| {
                matches!(
                    descriptor.key,
                    SystemMcpKey::TaskRunnerService
                        | SystemMcpKey::TaskProcessLog
                        | SystemMcpKey::AskUser
                ) && self.supports(route)
            })
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "Task Runner route is not a supported System MCP",
                )
            })?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            CALLER_SERVICE,
            TOKEN_AUDIENCE,
            TASK_RUNNER_MCP_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/{}",
            self.base_url,
            urlencoding::encode(descriptor.key.as_str())
        );
        let mut request = self
            .http
            .post(endpoint)
            .header("x-task-runner-caller", CALLER_SERVICE)
            .header("x-task-runner-internal-token", token)
            .header(
                "x-mcp-management-owner-user-id",
                snapshot.owner_user_id.as_str(),
            )
            .header("x-mcp-management-agent-key", snapshot.agent_key.as_str())
            .header("x-mcp-management-session-id", snapshot.session_id.as_str())
            .header(
                "x-mcp-management-session-expires-at-unix",
                snapshot.expires_at_unix.to_string(),
            )
            .header("x-mcp-management-project-id", snapshot.project_id.as_str())
            .header("x-chatos-project-id", snapshot.project_id.as_str())
            .timeout(Duration::from_secs(5));
        for (header, value) in [
            ("x-mcp-management-run-id", snapshot.run_id.as_deref()),
            ("x-mcp-management-turn-id", snapshot.turn_id.as_deref()),
            ("x-mcp-management-task-id", snapshot.task_id.as_deref()),
            (
                "x-mcp-management-source-session-id",
                snapshot.source_session_id.as_deref(),
            ),
            (
                "x-mcp-management-source-user-message-id",
                snapshot.source_user_message_id.as_deref(),
            ),
        ] {
            if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
                request = request.header(header, value);
            }
        }
        let response = request
            .json(&json!({
                "jsonrpc": "2.0",
                "method": METHOD_NOTIFICATIONS_CANCELLED,
                "params": {
                    "requestId": invocation_id,
                    "reason": "MCP Management runtime cancelled the invocation"
                }
            }))
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Task Runner Provider cancellation request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Task Runner Provider cancellation response could not be read: {error}"
                ))
            })?;
        decode_cancel_notification_response(status, bytes.as_slice(), "Task Runner Provider")
    }
}
