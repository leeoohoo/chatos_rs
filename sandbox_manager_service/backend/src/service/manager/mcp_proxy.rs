// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use axum::http::StatusCode;
use chatos_service_runtime::http_body::{
    read_response_preview_text_limited_or_message, read_response_text_limited,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use chatos_service_runtime::{
    build_http_client, classify_http_request_error, http_client_builder, HttpClientTimeouts,
    HttpRequestErrorKind,
};
use serde_json::Value;

use crate::auth::{SandboxAuthContext, SCOPE_MCP_CALL, SCOPE_MCP_TOOLS};
use crate::error::ApiError;
use crate::models::SandboxLeaseRecord;

use super::SandboxManager;

impl SandboxManager {
    pub async fn mcp_proxy(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
        payload: Value,
    ) -> Result<Value, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        authorize_mcp_proxy_payload(auth, &record, &payload)?;
        let agent_endpoint = self.agent_endpoint_for(&record).await?;
        let agent_token = self.agent_token_for_record(&record);
        jsonrpc_agent_proxy(agent_endpoint.as_str(), Some(agent_token.as_str()), payload).await
    }

    async fn agent_endpoint_for(&self, record: &SandboxLeaseRecord) -> Result<String, ApiError> {
        if let Some(endpoint) = record
            .agent_endpoint
            .clone()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return validate_http_agent_endpoint(endpoint);
        }

        let inspected = self
            .backend
            .inspect(record.sandbox_id.as_str(), record.backend_id.as_deref())
            .await
            .map_err(|err| {
                ApiError::with_code(
                    StatusCode::BAD_GATEWAY,
                    "sandbox_backend_inspect_failed",
                    err,
                )
            })?;
        let endpoint = inspected.and_then(|instance| instance.agent_endpoint);
        let endpoint = endpoint
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::bad_request("sandbox agent endpoint is not available"))?;
        validate_http_agent_endpoint(endpoint)
    }
}

fn authorize_mcp_proxy_payload(
    auth: &SandboxAuthContext,
    record: &SandboxLeaseRecord,
    payload: &Value,
) -> Result<(), ApiError> {
    match payload {
        Value::Object(_) => authorize_mcp_proxy_request(auth, record, payload),
        Value::Array(items) => {
            if items.is_empty() {
                return Err(ApiError::bad_request("MCP JSON-RPC batch is empty"));
            }
            for item in items {
                authorize_mcp_proxy_request(auth, record, item)?;
            }
            Ok(())
        }
        _ => Err(ApiError::bad_request(
            "MCP JSON-RPC payload must be an object or array",
        )),
    }
}

fn authorize_mcp_proxy_request(
    auth: &SandboxAuthContext,
    record: &SandboxLeaseRecord,
    payload: &Value,
) -> Result<(), ApiError> {
    let method = payload
        .get("method")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("MCP JSON-RPC method is required"))?;

    match method {
        "tools/list" => auth.ensure_lease_access(record, SCOPE_MCP_TOOLS),
        "tools/call" => {
            auth.ensure_lease_access(record, SCOPE_MCP_CALL)?;
            let tool_name = payload
                .get("params")
                .and_then(|params| params.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ApiError::bad_request("tools/call.name is required"))?;
            auth.ensure_tool_allowed(tool_name)
        }
        _ => auth.ensure_lease_access(record, SCOPE_MCP_CALL),
    }
}

pub(super) async fn check_agent_health(agent_endpoint: Option<&str>) -> (Option<bool>, String) {
    let Some(endpoint) = agent_endpoint
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (None, "agent endpoint is not configured".to_string());
    };

    if endpoint.starts_with("mock://") {
        return (Some(true), "mock agent endpoint is reachable".to_string());
    }

    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return (
            Some(false),
            format!("unsupported agent endpoint scheme: {endpoint}"),
        );
    }

    let health_url = format!("{}/health", endpoint.trim_end_matches('/'));
    let client = match build_http_client(HttpClientTimeouts::new(Duration::from_secs(2))) {
        Ok(client) => client,
        Err(err) => {
            return (
                Some(false),
                format!("build agent health client failed: {err}"),
            );
        }
    };

    match client.get(&health_url).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                (
                    Some(true),
                    format!("agent health endpoint returned {status}"),
                )
            } else {
                (
                    Some(false),
                    format!("agent health endpoint returned {status}"),
                )
            }
        }
        Err(err) => (Some(false), format!("agent health request failed: {err}")),
    }
}

async fn jsonrpc_agent_proxy(
    agent_endpoint: &str,
    agent_token: Option<&str>,
    payload: Value,
) -> Result<Value, ApiError> {
    let url = format!("{}/mcp", agent_endpoint.trim_end_matches('/'));
    let client = http_client_builder(HttpClientTimeouts::new(Duration::from_secs(15)))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| ApiError::internal(format!("build MCP proxy client failed: {err}")))?;
    let mut request = client.post(url.as_str());
    if let Some(agent_token) = agent_token.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.bearer_auth(agent_token);
    }
    let response = request.json(&payload).send().await.map_err(|err| {
        let status = if classify_http_request_error(&err) == HttpRequestErrorKind::Timeout {
            StatusCode::GATEWAY_TIMEOUT
        } else {
            StatusCode::BAD_GATEWAY
        };
        ApiError::with_code(
            status,
            "sandbox_mcp_proxy_request_failed",
            format!("MCP proxy request failed: {err}"),
        )
    })?;

    let status = response.status();
    if !status.is_success() {
        let body =
            read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                .await;
        return Err(ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_proxy_http_error",
            format!("MCP proxy returned HTTP {status}: {}", preview_text(&body)),
        ));
    }
    let body = read_response_text_limited(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| {
            ApiError::with_code(
                StatusCode::BAD_GATEWAY,
                "sandbox_mcp_proxy_response_failed",
                format!("MCP proxy response read failed: {err}"),
            )
        })?;
    serde_json::from_str(body.as_str()).map_err(|err| {
        ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_proxy_invalid_json",
            format!(
                "MCP proxy returned invalid JSON: {err}; body={}",
                preview_text(&body)
            ),
        )
    })
}

fn validate_http_agent_endpoint(endpoint: String) -> Result<String, ApiError> {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        Ok(endpoint)
    } else {
        Err(ApiError::bad_request(format!(
            "sandbox agent endpoint is not an HTTP endpoint: {endpoint}"
        )))
    }
}

fn preview_text(value: &str) -> String {
    const LIMIT: usize = 1200;
    if value.chars().count() <= LIMIT {
        return value.to_string();
    }
    value.chars().take(LIMIT).collect::<String>() + "...[truncated]"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{SandboxSystemClient, SCOPE_MCP_CALL, SCOPE_MCP_TOOLS};
    use crate::models::{NetworkPolicy, ResourceLimits, SandboxStatus};
    use serde_json::json;

    fn lease_record() -> SandboxLeaseRecord {
        SandboxLeaseRecord {
            id: "lease-1".to_string(),
            sandbox_id: "sandbox-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            user_id: "user-1".to_string(),
            project_id: "project-1".to_string(),
            run_id: "run-1".to_string(),
            workspace_root: "/tmp/workspace".to_string(),
            run_workspace: "/tmp/workspace/.chatos/task-runner/runs/run-1".to_string(),
            backend: "mock".to_string(),
            backend_id: Some("backend-1".to_string()),
            image_id: None,
            image_ref: None,
            status: SandboxStatus::Ready,
            agent_endpoint: Some("http://127.0.0.1:49888".to_string()),
            resource_limits: ResourceLimits::default(),
            network: NetworkPolicy::default(),
            tools: vec!["filesystem".to_string(), "terminal".to_string()],
            agent_token_nonce: Some("nonce-1".to_string()),
            idempotency_key: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            expires_at: "2026-01-01T01:00:00Z".to_string(),
            destroyed_at: None,
            last_error: None,
            effective_policy: Default::default(),
            effective_permissions: None,
        }
    }

    fn system_auth(scopes: &[&str], tools: &[&str]) -> SandboxAuthContext {
        SandboxAuthContext::System(SandboxSystemClient {
            client_id: "task_runner".to_string(),
            scopes: scopes.iter().map(|value| value.to_string()).collect(),
            allowed_tenant_ids: vec!["tenant-1".to_string()],
            allowed_project_ids: vec!["project-1".to_string()],
            allowed_tools: tools.iter().map(|value| value.to_string()).collect(),
            max_lease_ttl_seconds: 3_600,
        })
    }

    #[test]
    fn mcp_proxy_authorizes_tools_list_with_tools_scope() {
        let auth = system_auth(&[SCOPE_MCP_TOOLS], &["read_file_raw"]);
        let payload = json!({
            "jsonrpc": "2.0",
            "id": "request-1",
            "method": "tools/list",
            "params": {}
        });

        assert!(authorize_mcp_proxy_payload(&auth, &lease_record(), &payload).is_ok());
    }

    #[test]
    fn mcp_proxy_enforces_tools_call_tool_policy() {
        let auth = system_auth(&[SCOPE_MCP_CALL], &["read_file_raw"]);
        let allowed = json!({
            "jsonrpc": "2.0",
            "id": "request-1",
            "method": "tools/call",
            "params": { "name": "read_file_raw", "arguments": {} }
        });
        let denied = json!({
            "jsonrpc": "2.0",
            "id": "request-2",
            "method": "tools/call",
            "params": { "name": "execute_command", "arguments": {} }
        });

        assert!(authorize_mcp_proxy_payload(&auth, &lease_record(), &allowed).is_ok());
        let err = authorize_mcp_proxy_payload(&auth, &lease_record(), &denied)
            .expect_err("unexpected allowed tool call");
        assert_eq!(err.status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn mcp_proxy_rejects_payload_without_method() {
        let auth = system_auth(&[SCOPE_MCP_CALL], &["*"]);
        let payload = json!({ "jsonrpc": "2.0", "id": "request-1", "params": {} });

        let err = authorize_mcp_proxy_payload(&auth, &lease_record(), &payload)
            .expect_err("unexpected accepted invalid payload");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }
}
