// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_mcp_service::{METHOD_NOTIFICATIONS_CANCELLED, METHOD_TOOLS_CALL};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde_json::{json, Value};

use crate::providers::{
    decode_cancel_notification_response, ProviderCallOutcome, ProviderCancelOutcome,
};
use crate::runtime::RuntimeSessionSnapshot;

use super::validation::cloud_sandbox_call_timeout;
use super::{decode_jsonrpc_response, CloudSandboxProvider, ProviderCallError};

impl CloudSandboxProvider {
    pub(in crate::providers) async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        if !self.supports(route) {
            return Err(ProviderCallError::provider_unavailable(
                "Cloud Sandbox Provider does not support this route",
            ));
        }
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "runtime session does not contain a Cloud Sandbox target",
            )
        })?;
        if route.provider_ref.as_deref() != Some(target.provider_ref().as_str()) {
            return Err(ProviderCallError::provider_unavailable(
                "Cloud Sandbox route does not match the immutable runtime target",
            ));
        }
        self.validate_target(
            target,
            snapshot.owner_user_id.as_str(),
            snapshot.project_id.as_str(),
            snapshot.run_id.as_deref(),
        )
        .await?;

        let sandbox_id = urlencoding::encode(target.sandbox_id.trim());
        let path = if target.is_environment {
            format!("/api/internal/sandbox-environments/{sandbox_id}/mcp")
        } else {
            format!("/api/internal/sandboxes/{sandbox_id}/mcp")
        };
        let mut request = self.authenticated(self.http.post(format!("{}{path}", self.base_url)))?;
        if let Some(service_id) = target.service_id.as_deref() {
            request = request.header("x-chatos-service-id", service_id);
        }
        request = request
            .header("x-chatos-sandbox-lease-id", target.lease_id.as_str())
            .header(
                "x-mcp-management-owner-user-id",
                snapshot.owner_user_id.as_str(),
            )
            .header("x-mcp-management-project-id", snapshot.project_id.as_str())
            .header(
                "x-mcp-management-run-id",
                snapshot.run_id.as_deref().unwrap_or_default(),
            );
        let response = request
            .timeout(cloud_sandbox_call_timeout(
                original_tool_name,
                &arguments,
                self.request_timeout,
            ))
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
                    "Cloud Sandbox Provider request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud Sandbox Provider response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Cloud Sandbox Provider rejected the request with HTTP {}",
                status.as_u16()
            )));
        }
        let result = decode_jsonrpc_response(bytes.as_slice(), invocation_id, "Cloud Sandbox")?;
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
        if !self.supports(route) {
            return Ok(ProviderCancelOutcome::NotSupported);
        }
        let target = snapshot.sandbox_target.as_ref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "runtime session does not contain a Cloud Sandbox target",
            )
        })?;
        if route.provider_ref.as_deref() != Some(target.provider_ref().as_str()) {
            return Err(ProviderCallError::provider_unavailable(
                "Cloud Sandbox route does not match the immutable runtime target",
            ));
        }
        let sandbox_id = urlencoding::encode(target.sandbox_id.trim());
        let path = if target.is_environment {
            format!("/api/internal/sandbox-environments/{sandbox_id}/mcp")
        } else {
            format!("/api/internal/sandboxes/{sandbox_id}/mcp")
        };
        let mut request = self.authenticated(self.http.post(format!("{}{path}", self.base_url)))?;
        if let Some(service_id) = target.service_id.as_deref() {
            request = request.header("x-chatos-service-id", service_id);
        }
        let response = request
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
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Cloud Sandbox cancellation request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Cloud Sandbox cancellation response could not be read: {error}"
                ))
            })?;
        decode_cancel_notification_response(status, bytes.as_slice(), "Cloud Sandbox")
    }
}
