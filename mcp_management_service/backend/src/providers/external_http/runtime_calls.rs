// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp_management_sdk::McpProviderKind;
use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_mcp_service::{
    MCP_ERROR_AUTH_REQUIRED, METHOD_NOTIFICATIONS_CANCELLED, METHOD_TOOLS_CALL,
};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde_json::{json, Value};

use crate::runtime::{ExternalHttpProviderBinding, RuntimeSessionSnapshot};

use super::super::project_service::decode_jsonrpc_response;
use super::{ExternalHttpProvider, ProviderCallError, JSON_CONTENT_TYPE};
use crate::providers::{decode_cancel_notification_response, managed_tool_call_params};
use crate::providers::{ProviderCallOutcome, ProviderCancelOutcome};

impl ExternalHttpProvider {
    pub(in crate::providers) async fn call_tool(
        &self,
        snapshot: &RuntimeSessionSnapshot,
        route: &ResolvedMcpRoute,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        let binding = snapshot
            .external_http_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "External HTTP MCP runtime binding is missing",
                )
            })?;
        if route.provider_kind != McpProviderKind::ExternalHttp
            || route.provider_ref.as_deref() != Some(binding.provider_ref.as_str())
            || route.allow_writes != binding.allow_writes
        {
            return Err(ProviderCallError::provider_unavailable(
                "External HTTP MCP route does not match its runtime binding",
            ));
        }
        if !binding.allows_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: "tool is blocked by the External HTTP MCP policy".to_string(),
            });
        }
        self.call_bound_tool(
            binding,
            original_tool_name,
            arguments,
            invocation_id,
            "External HTTP MCP",
            snapshot.tool_result_max_chars,
        )
        .await
    }

    pub(in crate::providers) async fn list_tools_for_binding(
        &self,
        binding: &ExternalHttpProviderBinding,
        request_id: &str,
        provider_label: &str,
    ) -> Result<Vec<Value>, ProviderCallError> {
        let response = binding
            .http
            .post(binding.endpoint.clone())
            .headers(binding.headers.clone())
            .header(CONTENT_TYPE, JSON_CONTENT_TYPE)
            .header(ACCEPT, JSON_CONTENT_TYPE)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": "tools/list",
                "params": {}
            }))
            .send()
            .await
            .map_err(|_| {
                ProviderCallError::provider_unavailable(format!("{provider_label} request failed"))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "{provider_label} response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(if matches!(status.as_u16(), 401 | 403) {
                ProviderCallError {
                    code: MCP_ERROR_AUTH_REQUIRED,
                    message: format!("{provider_label} rejected its configured credentials"),
                }
            } else {
                ProviderCallError::provider_unavailable(format!(
                    "{provider_label} returned HTTP {}",
                    status.as_u16()
                ))
            });
        }
        let result = decode_jsonrpc_response(bytes.as_slice(), request_id, provider_label)?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| {
                ProviderCallError::invalid_response(format!(
                    "{provider_label} tools/list response has no tools array"
                ))
            })?;
        if tools.iter().any(|tool| {
            !tool.is_object()
                || tool
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .is_none_or(str::is_empty)
        }) {
            return Err(ProviderCallError::invalid_response(format!(
                "{provider_label} tools/list returned an invalid tool definition"
            )));
        }
        Ok(tools)
    }

    pub(in crate::providers) async fn call_bound_tool(
        &self,
        binding: &ExternalHttpProviderBinding,
        original_tool_name: &str,
        arguments: Value,
        invocation_id: &str,
        provider_label: &str,
        tool_result_max_chars: Option<usize>,
    ) -> Result<ProviderCallOutcome, ProviderCallError> {
        if !binding.allows_tool(original_tool_name) {
            return Err(ProviderCallError {
                code: MCP_ERROR_AUTH_REQUIRED,
                message: format!("tool is blocked by the {provider_label} policy"),
            });
        }
        let response = binding
            .http
            .post(binding.endpoint.clone())
            .headers(binding.headers.clone())
            .header(CONTENT_TYPE, JSON_CONTENT_TYPE)
            .header(ACCEPT, JSON_CONTENT_TYPE)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": invocation_id,
                "method": METHOD_TOOLS_CALL,
                "params": managed_tool_call_params(
                    original_tool_name,
                    arguments,
                    tool_result_max_chars,
                )
            }))
            .send()
            .await
            .map_err(|_| {
                ProviderCallError::provider_unavailable(format!("{provider_label} request failed"))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|err| {
                ProviderCallError::invalid_response(format!(
                    "{provider_label} response could not be read: {err}"
                ))
            })?;
        if !status.is_success() {
            return Err(if matches!(status.as_u16(), 401 | 403) {
                ProviderCallError {
                    code: MCP_ERROR_AUTH_REQUIRED,
                    message: format!("{provider_label} rejected its configured credentials"),
                }
            } else {
                ProviderCallError::provider_unavailable(format!(
                    "{provider_label} returned HTTP {}",
                    status.as_u16()
                ))
            });
        }
        let result = decode_jsonrpc_response(bytes.as_slice(), invocation_id, provider_label)?;
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
        let binding = snapshot
            .external_http_bindings
            .get(route.resource_id.as_str())
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "External HTTP MCP runtime binding is missing",
                )
            })?;
        if route.provider_kind != McpProviderKind::ExternalHttp
            || route.provider_ref.as_deref() != Some(binding.provider_ref.as_str())
        {
            return Err(ProviderCallError::provider_unavailable(
                "External HTTP MCP route does not match its runtime binding",
            ));
        }
        self.cancel_bound_invocation(binding, invocation_id, "External HTTP MCP")
            .await
    }

    pub(in crate::providers) async fn cancel_bound_invocation(
        &self,
        binding: &ExternalHttpProviderBinding,
        invocation_id: &str,
        provider_label: &str,
    ) -> Result<ProviderCancelOutcome, ProviderCallError> {
        let response = binding
            .http
            .post(binding.endpoint.clone())
            .headers(binding.headers.clone())
            .header(CONTENT_TYPE, JSON_CONTENT_TYPE)
            .header(ACCEPT, JSON_CONTENT_TYPE)
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
            .map_err(|_| {
                ProviderCallError::provider_unavailable(format!(
                    "{provider_label} cancellation request failed"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "{provider_label} cancellation response could not be read: {error}"
                ))
            })?;
        decode_cancel_notification_response(status, bytes.as_slice(), provider_label)
    }
}
