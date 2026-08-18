// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_service::{METHOD_NOTIFICATIONS_CANCELLED, METHOD_TOOLS_CALL};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde_json::{json, Value};

use crate::providers::decode_cancel_notification_response;
use crate::providers::{ProviderCallOutcome, ProviderCancelOutcome};
use crate::runtime::RuntimeSessionSnapshot;

use super::super::project_service::decode_jsonrpc_response;
use super::{LocalConnectorProvider, ProviderCallError};
use crate::providers::managed_tool_call_params;
use chatos_mcp_management_sdk::ResolvedMcpRoute;

impl LocalConnectorProvider {
    pub(in crate::providers) async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let response = self
            .relay_request(snapshot, route)?
            .json(&json!({
                "jsonrpc": "2.0",
                "id": invocation_id,
                "method": METHOD_TOOLS_CALL,
                "params": managed_tool_call_params(
                    original_tool_name,
                    arguments,
                    snapshot.tool_result_max_chars,
                )
            }))
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "Local Connector Provider request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Connector Provider response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "Local Connector Provider rejected the request with HTTP {}",
                status.as_u16()
            )));
        }
        let result =
            decode_jsonrpc_response(bytes.as_slice(), invocation_id, "Local Connector Provider")?;
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
        let response = self
            .relay_request(snapshot, route)?
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
                    "Local Connector cancellation request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "Local Connector cancellation response could not be read: {error}"
                ))
            })?;
        decode_cancel_notification_response(status, bytes.as_slice(), "Local Connector Provider")
    }
}
