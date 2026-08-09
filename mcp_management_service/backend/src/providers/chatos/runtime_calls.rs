// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_mcp::{system_mcp_descriptor_by_resource_id, SystemMcpKey};
use chatos_mcp_management_sdk::ResolvedMcpRoute;
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
    CLOUD_BROWSER_SESSION_CLOSE_METHOD,
};

impl ChatosProvider {
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
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/{}",
            self.base_url,
            urlencoding::encode(descriptor.key.as_str())
        );
        let timeout = match descriptor.key {
            SystemMcpKey::AskUser => self.ask_user_request_timeout,
            SystemMcpKey::BrowserTools => self.browser_request_timeout,
            _ => self.request_timeout,
        };
        let binding = ChatosRequestBinding::from(snapshot);
        let response = self
            .bound_request(&binding, endpoint, timeout, secret)?
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
        snapshot: &RuntimeSessionSnapshot,
    ) -> Result<(), ProviderCallError> {
        let has_cloud_browser = snapshot.routes.iter().any(|route| {
            self.supports(route)
                && system_mcp_descriptor_by_resource_id(route.resource_id.as_str())
                    .is_some_and(|descriptor| descriptor.key == SystemMcpKey::BrowserTools)
        });
        if !has_cloud_browser {
            return Ok(());
        }
        let secret = self.internal_secret.as_deref().ok_or_else(|| {
            ProviderCallError::provider_unavailable(
                "ChatOS Provider internal secret is not configured",
            )
        })?;
        let invocation_id = format!("close-{}", snapshot.session_id);
        let endpoint = format!(
            "{}/internal/mcp-management/mcp/browser_tools/sessions/{}/close",
            self.base_url,
            urlencoding::encode(snapshot.session_id.as_str())
        );
        let binding = ChatosRequestBinding::from(snapshot);
        let response = self
            .bound_request(&binding, endpoint, self.browser_request_timeout, secret)?
            .json(&json!({
                "jsonrpc": "2.0",
                "id": invocation_id,
                "method": CLOUD_BROWSER_SESSION_CLOSE_METHOD,
                "params": {}
            }))
            .send()
            .await
            .map_err(|error| {
                ProviderCallError::provider_unavailable(format!(
                    "ChatOS Browser Runtime close request failed: {error}"
                ))
            })?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, self.response_limit_bytes)
            .await
            .map_err(|error| {
                ProviderCallError::invalid_response(format!(
                    "ChatOS Browser Runtime close response could not be read: {error}"
                ))
            })?;
        if !status.is_success() {
            return Err(ProviderCallError::provider_unavailable(format!(
                "ChatOS Browser Runtime close was rejected with HTTP {}",
                status.as_u16()
            )));
        }
        decode_jsonrpc_response(
            bytes.as_slice(),
            invocation_id.as_str(),
            "ChatOS Browser Runtime close",
        )?;
        Ok(())
    }
}
