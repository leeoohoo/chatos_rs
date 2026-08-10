// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::{ResolvedMcpRoute, SandboxExecutionTarget, SandboxProviderKind};
use chatos_mcp_service::{METHOD_NOTIFICATIONS_CANCELLED, METHOD_TOOLS_CALL};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde_json::{json, Value};

use crate::providers::decode_cancel_notification_response;
use crate::providers::{ProviderCallOutcome, ProviderCancelOutcome};
use crate::runtime::RuntimeSessionSnapshot;

use super::super::project_service::decode_jsonrpc_response;
use super::manager_client::required_pairing_id;
use super::{LocalSandboxProvider, ProviderCallError, SANDBOX_SERVICE_SCOPE};

impl LocalSandboxProvider {
    pub(in crate::providers) async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let target = self.validated_snapshot_target(snapshot, route).await?;
        let pairing_id = required_pairing_id(target)?;
        let response = self
            .authenticated(
                self.http.post(self.sandbox_url(
                    pairing_id,
                    target.sandbox_id.as_str(),
                    Some("mcp"),
                )),
                SANDBOX_SERVICE_SCOPE,
                snapshot.owner_user_id.as_str(),
            )?
            .header("x-chatos-sandbox-lease-id", target.lease_id.as_str())
            .header(
                "x-mcp-management-owner-user-id",
                snapshot.owner_user_id.as_str(),
            )
            .header("x-mcp-management-project-id", snapshot.project_id.as_str())
            .header(
                "x-mcp-management-run-id",
                snapshot.run_id.as_deref().unwrap_or_default(),
            )
            .json(&json!({
                "jsonrpc": "2.0",
                "id": invocation_id,
                "method": METHOD_TOOLS_CALL,
                "params": {
                    "name": original_tool_name,
                    "arguments": arguments,
                }
            }))
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Local Sandbox MCP request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Sandbox MCP response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Local Sandbox MCP rejected the request with HTTP {}",
                status.as_u16()
            )));
        }
        let result = decode_jsonrpc_response(bytes.as_slice(), invocation_id, "Local Sandbox")?;
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
        let target = self.validated_snapshot_target(snapshot, route).await?;
        let pairing_id = required_pairing_id(target)?;
        let response = self
            .authenticated(
                self.http.post(self.sandbox_url(
                    pairing_id,
                    target.sandbox_id.as_str(),
                    Some("mcp"),
                )),
                SANDBOX_SERVICE_SCOPE,
                snapshot.owner_user_id.as_str(),
            )?
            .header("x-chatos-sandbox-lease-id", target.lease_id.as_str())
            .header(
                "x-mcp-management-owner-user-id",
                snapshot.owner_user_id.as_str(),
            )
            .header("x-mcp-management-project-id", snapshot.project_id.as_str())
            .header(
                "x-mcp-management-run-id",
                snapshot.run_id.as_deref().unwrap_or_default(),
            )
            .json(&json!({
                "jsonrpc": "2.0",
                "method": METHOD_NOTIFICATIONS_CANCELLED,
                "params": {
                    "requestId": invocation_id,
                    "reason": "MCP Management runtime cancelled the invocation"
                }
            }))
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Local Sandbox cancellation request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Sandbox cancellation response could not be read: {error}"
                ))
            })?;
        decode_cancel_notification_response(status, bytes.as_slice(), "Local Sandbox")
    }

    async fn validated_snapshot_target<'a>(
        &self,
        snapshot: &'a RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
    ) -> Result<&'a SandboxExecutionTarget, ProviderCallError> {
        if !self.supports(route) {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox Provider does not support this route",
            ));
        }
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "runtime session does not contain a Local Sandbox target",
            )
        })?;
        if target.provider != SandboxProviderKind::LocalConnector
            || route.provider_ref.as_deref() != Some(target.provider_ref().as_str())
            || snapshot.project_context.sandbox_pairing_id.as_deref()
                != target.pairing_id.as_deref()
        {
            return Err(ProviderCallError::provider_unavailable(
                "Local Sandbox route does not match the immutable runtime target",
            ));
        }
        self.validate_target(
            target,
            snapshot.owner_user_id.as_str(),
            snapshot.project_id.as_str(),
            snapshot.run_id.as_deref(),
        )
        .await?;
        Ok(target)
    }
}
