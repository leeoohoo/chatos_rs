// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::ResolvedMcpRoute;
use chatos_mcp_management_sdk::RuntimeRemoteConnectionRouteTarget;
use chatos_mcp_service::{METHOD_NOTIFICATIONS_CANCELLED, METHOD_TOOLS_CALL};
use chatos_service_runtime::http_body::read_response_bytes_limited;
use serde_json::{json, Value};

use crate::providers::project_service::decode_jsonrpc_response;
use crate::providers::{
    decode_cancel_notification_response, ProviderCallOutcome, ProviderCancelOutcome,
};
use crate::runtime::RuntimeSessionSnapshot;

use super::{
    is_memory_reader, memory_provider_ref, ChatosProvider, ChatosRequestBinding, ProviderCallError,
};
use crate::providers::managed_tool_call_params;

impl ChatosProvider {
    pub(in crate::providers) async fn resolve_remote_connection_route(
        &self,
        owner_user_id: &str,
        remote_connection_id: &str,
    ) -> Result<RuntimeRemoteConnectionRouteTarget, ProviderCallError> {
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "ChatOS Provider internal secret is not configured",
            )
        })?;
        let token = chatos_service_runtime::issue_internal_service_token(
            secret,
            super::CALLER_SERVICE,
            super::TOKEN_AUDIENCE,
            super::CHATOS_MCP_SCOPE,
            60,
        )
        .map_err(ProviderCallError::provider_unavailable)?;
        let endpoint = format!(
            "{}/internal/mcp-management/remote-connections/{}/route",
            self.base_url,
            urlencoding::encode(remote_connection_id.trim())
        );
        let response = self
            .http
            .post(endpoint)
            .header("x-chatos-caller", super::CALLER_SERVICE)
            .header("x-chatos-internal-token", token)
            .header("x-mcp-management-owner-user-id", owner_user_id.trim())
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "resolve remote connection Local Connector target failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "remote connection route response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            let message = serde_json::from_slice::<Value>(bytes.as_slice())
                .ok()
                .and_then(|value| {
                    value
                        .get("error")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
            return Err(ProviderCallError::provider_unavailable(message));
        }
        serde_json::from_slice(bytes.as_slice()).map_err(|error| {
            ProviderCallError::invalid_response(format!(
                "decode remote connection route failed: {error}"
            ))
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
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "ChatOS Provider internal secret is not configured",
            )
        })?;
        let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .filter(|_| self.supports(route))
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "ChatOS route is not a supported System MCP",
                )
            })?;
        if is_memory_reader(descriptor.key) {
            let contact_agent_id = snapshot
                .contact_agent_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    ProviderCallError::provider_unavailable(
                        "ChatOS Memory Reader has no bound contact agent",
                    )
                })?;
            let expected_provider_ref = memory_provider_ref(contact_agent_id);
            if snapshot
                .source_session_id
                .as_deref()
                .map(str::trim)
                .is_none_or(|value| value.is_empty())
                || route.provider_ref.as_deref() != Some(expected_provider_ref.as_str())
            {
                return Err(ProviderCallError::provider_unavailable(
                    "ChatOS Memory Reader route does not match the immutable runtime binding",
                ));
            }
        }
        let binding = ChatosRequestBinding::from(snapshot);
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/{}",
            self.base_url,
            urlencoding::encode(descriptor.key.as_str())
        );
        let timeout = match descriptor.key {
            SystemMcpKey::AskUser => self.ask_user_request_timeout,
            _ => self.request_timeout,
        };
        let response = self
            .bound_request(&binding, endpoint, timeout, secret)?
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
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "ChatOS Provider request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "ChatOS Provider response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "ChatOS Provider rejected the request with HTTP {}",
                status.as_u16()
            )));
        }
        let result = decode_jsonrpc_response(bytes.as_slice(), invocation_id, "ChatOS Provider")?;
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
                "ChatOS Provider internal secret is not configured",
            )
        })?;
        let descriptor = system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
            .filter(|_| self.supports(route))
            .ok_or_else(|| {
                ProviderCallError::provider_unavailable(
                    "ChatOS route is not a supported System MCP",
                )
            })?;
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/{}",
            self.base_url,
            urlencoding::encode(descriptor.key.as_str())
        );
        let binding = ChatosRequestBinding::from(snapshot);
        let response = self
            .bound_request(&binding, endpoint, Duration::from_secs(5), secret)?
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
                    "ChatOS Provider cancellation request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "ChatOS Provider cancellation response could not be read: {error}"
                ))
            })?;
        decode_cancel_notification_response(status, bytes.as_slice(), "ChatOS Provider")
    }

    pub(in crate::providers) async fn close_session(
        &self,
        _snapshot: &RuntimeSessionSnapshot,
    ) -> Result<(), ProviderCallError> {
        Ok(())
    }
}
use std::time::Duration;
