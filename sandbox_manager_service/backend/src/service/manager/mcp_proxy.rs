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
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::auth::{SandboxAuthContext, SCOPE_MCP_CALL, SCOPE_MCP_TOOLS};
use crate::error::ApiError;
use crate::models::{
    CloudStdioMcpCallRequest, CloudStdioMcpCallResponse, CloudStdioMcpCancelRequest,
    CloudStdioMcpCancelResponse, CloudStdioMcpCloseRequest, CloudStdioMcpCloseResponse,
    SandboxLeaseRecord,
};

use super::SandboxManager;

const SANDBOX_AGENT_MCP_TIMEOUT: Duration = Duration::from_secs(135);
const SANDBOX_AGENT_CLOUD_STDIO_TIMEOUT: Duration = Duration::from_secs(10 * 60 + 15);
const TERMINAL_WAIT_TRANSPORT_GRACE_MS: u64 = 15_000;

#[derive(Debug, Clone)]
pub struct SandboxMcpRuntimeBinding {
    pub lease_id: String,
    pub owner_user_id: String,
    pub project_id: String,
    pub run_id: String,
    pub runtime_session_id: Option<String>,
}

impl SandboxManager {
    pub async fn mcp_proxy(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
        binding: Option<&SandboxMcpRuntimeBinding>,
        mut payload: Value,
    ) -> Result<Value, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        validate_mcp_runtime_binding(auth, &record, binding)?;
        strip_ungrantable_command_permission_requests(&mut payload);
        authorize_mcp_proxy_payload(auth, &record, &payload)?;
        let agent_endpoint = self.agent_endpoint_for(&record).await?;
        let agent_token = self.agent_token_for_record(&record);
        let mut response =
            jsonrpc_agent_proxy(agent_endpoint.as_str(), Some(agent_token.as_str()), payload)
                .await?;
        strip_ungrantable_command_permission_schemas(&mut response);
        Ok(response)
    }

    pub async fn browser_mcp_proxy(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
        binding: Option<&SandboxMcpRuntimeBinding>,
        payload: Value,
    ) -> Result<Value, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        let runtime_session_id =
            validate_browser_mcp_runtime_binding(auth, &record, binding, &payload)?;
        let agent_endpoint = self.agent_endpoint_for(&record).await?;
        let agent_token = self.agent_token_for_record(&record);
        jsonrpc_agent_proxy_at(
            agent_endpoint.as_str(),
            "/internal/browser-mcp",
            Some(agent_token.as_str()),
            Some(runtime_session_id),
            payload,
        )
        .await
    }

    pub async fn cloud_stdio_mcp_call(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
        binding: Option<&SandboxMcpRuntimeBinding>,
        input: CloudStdioMcpCallRequest,
    ) -> Result<CloudStdioMcpCallResponse, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        validate_cloud_stdio_runtime_binding(auth, &record, binding)?;
        let agent_endpoint = self.agent_endpoint_for(&record).await?;
        let agent_token = self.agent_token_for_record(&record);
        cloud_stdio_agent_proxy(
            agent_endpoint.as_str(),
            agent_token.as_str(),
            "/internal/cloud-stdio-mcp/call",
            &input,
        )
        .await
    }

    pub async fn cloud_stdio_mcp_close(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
        binding: Option<&SandboxMcpRuntimeBinding>,
        input: CloudStdioMcpCloseRequest,
    ) -> Result<CloudStdioMcpCloseResponse, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        validate_cloud_stdio_runtime_binding(auth, &record, binding)?;
        let agent_endpoint = self.agent_endpoint_for(&record).await?;
        let agent_token = self.agent_token_for_record(&record);
        cloud_stdio_agent_proxy(
            agent_endpoint.as_str(),
            agent_token.as_str(),
            "/internal/cloud-stdio-mcp/close",
            &input,
        )
        .await
    }

    pub async fn cloud_stdio_mcp_cancel(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
        binding: Option<&SandboxMcpRuntimeBinding>,
        input: CloudStdioMcpCancelRequest,
    ) -> Result<CloudStdioMcpCancelResponse, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        validate_cloud_stdio_runtime_binding(auth, &record, binding)?;
        let agent_endpoint = self.agent_endpoint_for(&record).await?;
        let agent_token = self.agent_token_for_record(&record);
        cloud_stdio_agent_proxy(
            agent_endpoint.as_str(),
            agent_token.as_str(),
            "/internal/cloud-stdio-mcp/cancel",
            &input,
        )
        .await
    }

    pub(in crate::service::manager) async fn agent_endpoint_for(
        &self,
        record: &SandboxLeaseRecord,
    ) -> Result<String, ApiError> {
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

pub(in crate::service::manager) fn validate_cloud_stdio_runtime_binding(
    auth: &SandboxAuthContext,
    record: &SandboxLeaseRecord,
    binding: Option<&SandboxMcpRuntimeBinding>,
) -> Result<(), ApiError> {
    if auth.system_client_id() != Some("mcp-management-service") {
        return Err(ApiError::forbidden(
            "cloud stdio MCP proxy is restricted to MCP Management",
        ));
    }
    auth.ensure_lease_access(record, SCOPE_MCP_CALL)?;
    validate_mcp_runtime_binding(auth, record, binding)
}

pub(in crate::service::manager) fn validate_browser_mcp_runtime_binding<'a>(
    auth: &SandboxAuthContext,
    record: &SandboxLeaseRecord,
    binding: Option<&'a SandboxMcpRuntimeBinding>,
    payload: &Value,
) -> Result<&'a str, ApiError> {
    if auth.system_client_id() != Some("mcp-management-service") {
        return Err(ApiError::forbidden(
            "Browser MCP proxy is restricted to MCP Management",
        ));
    }
    validate_mcp_runtime_binding(auth, record, binding)?;
    let binding = binding.expect("validated MCP Management binding");
    let runtime_session_id = binding
        .runtime_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("Browser MCP runtime session id is required"))?;
    authorize_browser_mcp_payload(auth, record, payload)?;
    Ok(runtime_session_id)
}

fn authorize_browser_mcp_payload(
    auth: &SandboxAuthContext,
    record: &SandboxLeaseRecord,
    payload: &Value,
) -> Result<(), ApiError> {
    let method = payload
        .get("method")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("Browser MCP JSON-RPC method is required"))?;
    match method {
        "tools/list" => auth.ensure_lease_access(record, SCOPE_MCP_TOOLS),
        "tools/call" => {
            auth.ensure_lease_access(record, SCOPE_MCP_CALL)?;
            let name = payload
                .get("params")
                .and_then(|params| params.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| value.starts_with("browser_"))
                .ok_or_else(|| {
                    ApiError::bad_request("Browser MCP tools/call requires a browser_* tool")
                })?;
            auth.ensure_tool_allowed(name)
        }
        "browser/session/close" => auth.ensure_lease_access(record, SCOPE_MCP_CALL),
        _ => Err(ApiError::bad_request(
            "Browser MCP proxy only accepts tools/list, tools/call, or browser/session/close",
        )),
    }
}

pub(in crate::service::manager) fn validate_mcp_runtime_binding(
    auth: &SandboxAuthContext,
    record: &SandboxLeaseRecord,
    binding: Option<&SandboxMcpRuntimeBinding>,
) -> Result<(), ApiError> {
    if auth.system_client_id() != Some("mcp-management-service") {
        return Ok(());
    }
    let binding = binding.ok_or_else(|| {
        ApiError::bad_request("MCP Management sandbox runtime binding headers are required")
    })?;
    if record.id != binding.lease_id
        || record.tenant_id != binding.owner_user_id
        || record.project_id != binding.project_id
        || record.run_id != binding.run_id
    {
        return Err(ApiError::forbidden(
            "MCP Management sandbox runtime binding does not match the lease",
        ));
    }
    Ok(())
}

fn strip_ungrantable_command_permission_requests(payload: &mut Value) {
    match payload {
        Value::Array(items) => {
            for item in items {
                strip_ungrantable_command_permission_requests(item);
            }
        }
        Value::Object(object) => {
            let is_execute_command = object.get("method").and_then(Value::as_str)
                == Some("tools/call")
                && object
                    .get("params")
                    .and_then(|params| params.get("name"))
                    .and_then(Value::as_str)
                    == Some("execute_command");
            if !is_execute_command {
                return;
            }
            if let Some(arguments) = object
                .get_mut("params")
                .and_then(|params| params.get_mut("arguments"))
                .and_then(Value::as_object_mut)
            {
                arguments.remove("additionalPermissions");
                arguments.remove("_grantedPermissions");
            }
        }
        _ => {}
    }
}

fn strip_ungrantable_command_permission_schemas(response: &mut Value) {
    match response {
        Value::Array(items) => {
            for item in items {
                strip_ungrantable_command_permission_schemas(item);
            }
        }
        Value::Object(object) => {
            let Some(tools) = object
                .get_mut("result")
                .and_then(|result| result.get_mut("tools"))
                .and_then(Value::as_array_mut)
            else {
                return;
            };
            for tool in tools {
                if tool.get("name").and_then(Value::as_str) != Some("execute_command") {
                    continue;
                }
                if let Some(properties) = tool
                    .get_mut("inputSchema")
                    .and_then(|schema| schema.get_mut("properties"))
                    .and_then(Value::as_object_mut)
                {
                    properties.remove("additionalPermissions");
                    properties.remove("_grantedPermissions");
                }
            }
        }
        _ => {}
    }
}

pub(in crate::service::manager) fn authorize_mcp_proxy_payload(
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

pub(in crate::service::manager) async fn jsonrpc_agent_proxy(
    agent_endpoint: &str,
    agent_token: Option<&str>,
    payload: Value,
) -> Result<Value, ApiError> {
    jsonrpc_agent_proxy_at(agent_endpoint, "/mcp", agent_token, None, payload).await
}

pub(in crate::service::manager) async fn jsonrpc_agent_proxy_at(
    agent_endpoint: &str,
    path: &str,
    agent_token: Option<&str>,
    runtime_session_id: Option<&str>,
    payload: Value,
) -> Result<Value, ApiError> {
    let url = format!("{}{}", agent_endpoint.trim_end_matches('/'), path);
    let request_timeout = sandbox_agent_mcp_timeout(&payload);
    let client = http_client_builder(HttpClientTimeouts::new(request_timeout))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| ApiError::internal(format!("build MCP proxy client failed: {err}")))?;
    let mut request = client.post(url.as_str());
    if let Some(agent_token) = agent_token.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.bearer_auth(agent_token);
    }
    if let Some(runtime_session_id) = runtime_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.header("x-mcp-management-session-id", runtime_session_id);
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

fn sandbox_agent_mcp_timeout(payload: &Value) -> Duration {
    terminal_wait_timeout_ms(payload)
        .map(|timeout_ms| {
            Duration::from_millis(timeout_ms.saturating_add(TERMINAL_WAIT_TRANSPORT_GRACE_MS))
        })
        .unwrap_or(SANDBOX_AGENT_MCP_TIMEOUT)
        .max(SANDBOX_AGENT_MCP_TIMEOUT)
}

fn terminal_wait_timeout_ms(payload: &Value) -> Option<u64> {
    match payload {
        Value::Array(items) => items.iter().filter_map(terminal_wait_timeout_ms).max(),
        Value::Object(_) => {
            if payload.get("method").and_then(Value::as_str) != Some("tools/call") {
                return None;
            }
            let tool_name = payload.pointer("/params/name").and_then(Value::as_str)?;
            let arguments = payload.pointer("/params/arguments").unwrap_or(&Value::Null);
            let is_wait = tool_name == "process_wait"
                || tool_name.ends_with("_process_wait")
                || ((tool_name == "process" || tool_name.ends_with("_process"))
                    && arguments.get("action").and_then(Value::as_str) == Some("wait"));
            is_wait.then(|| chatos_mcp::resolve_wait_timeout_ms(arguments))
        }
        _ => None,
    }
}

pub(in crate::service::manager) async fn cloud_stdio_agent_proxy<I, O>(
    agent_endpoint: &str,
    agent_token: &str,
    path: &str,
    input: &I,
) -> Result<O, ApiError>
where
    I: Serialize + ?Sized,
    O: DeserializeOwned,
{
    let url = format!("{}{}", agent_endpoint.trim_end_matches('/'), path);
    let client = http_client_builder(HttpClientTimeouts::new(SANDBOX_AGENT_CLOUD_STDIO_TIMEOUT))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| {
            ApiError::internal(format!("build cloud stdio proxy client failed: {err}"))
        })?;
    let response = client
        .post(url.as_str())
        .bearer_auth(agent_token.trim())
        .json(input)
        .send()
        .await
        .map_err(|err| {
            let status = if classify_http_request_error(&err) == HttpRequestErrorKind::Timeout {
                StatusCode::GATEWAY_TIMEOUT
            } else {
                StatusCode::BAD_GATEWAY
            };
            ApiError::with_code(
                status,
                "sandbox_cloud_stdio_proxy_request_failed",
                format!("cloud stdio MCP proxy request failed: {err}"),
            )
        })?;
    let status = response.status();
    if !status.is_success() {
        let body =
            read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                .await;
        return Err(ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_cloud_stdio_proxy_http_error",
            format!(
                "cloud stdio MCP proxy returned HTTP {status}: {}",
                preview_text(&body)
            ),
        ));
    }
    let body = read_response_text_limited(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| {
            ApiError::with_code(
                StatusCode::BAD_GATEWAY,
                "sandbox_cloud_stdio_proxy_response_failed",
                format!("cloud stdio MCP proxy response read failed: {err}"),
            )
        })?;
    serde_json::from_str(body.as_str()).map_err(|err| {
        ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_cloud_stdio_proxy_invalid_json",
            format!(
                "cloud stdio MCP proxy returned invalid JSON: {err}; body={}",
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
            run_workspace: "/tmp/workspace/.chatos/runtime/runs/run-1".to_string(),
            backend: "mock".to_string(),
            backend_id: Some("backend-1".to_string()),
            image_id: None,
            image_ref: None,
            status: SandboxStatus::Ready,
            agent_endpoint: Some("http://127.0.0.1:49888".to_string()),
            resource_limits: ResourceLimits::default(),
            network: NetworkPolicy::default(),
            tools: vec!["filesystem".to_string(), "terminal".to_string()],
            lease_kind: "sandbox".to_string(),
            execution_service_id: None,
            environment_services: Vec::new(),
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
            internal_identity: None,
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
    fn mcp_management_proxy_requires_exact_runtime_binding() {
        let auth = SandboxAuthContext::System(SandboxSystemClient {
            client_id: "mcp-management-service".to_string(),
            scopes: vec![SCOPE_MCP_CALL.to_string()],
            allowed_tenant_ids: vec!["*".to_string()],
            allowed_project_ids: vec!["*".to_string()],
            allowed_tools: vec!["*".to_string()],
            max_lease_ttl_seconds: 3_600,
            internal_identity: None,
        });
        let binding = SandboxMcpRuntimeBinding {
            lease_id: "lease-1".to_string(),
            owner_user_id: "tenant-1".to_string(),
            project_id: "project-1".to_string(),
            run_id: "run-1".to_string(),
            runtime_session_id: None,
        };
        validate_mcp_runtime_binding(&auth, &lease_record(), Some(&binding)).unwrap();

        let mut mismatched = binding;
        mismatched.run_id = "another-run".to_string();
        let error = validate_mcp_runtime_binding(&auth, &lease_record(), Some(&mismatched))
            .expect_err("mismatched runtime binding must be rejected");
        assert_eq!(error.status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn cloud_stdio_proxy_is_restricted_to_mcp_management() {
        let binding = SandboxMcpRuntimeBinding {
            lease_id: "lease-1".to_string(),
            owner_user_id: "tenant-1".to_string(),
            project_id: "project-1".to_string(),
            run_id: "run-1".to_string(),
            runtime_session_id: None,
        };
        let task_runner = system_auth(&[SCOPE_MCP_CALL], &["*"]);
        let error =
            validate_cloud_stdio_runtime_binding(&task_runner, &lease_record(), Some(&binding))
                .expect_err("Task Runner must not send cloud stdio execution config");
        assert_eq!(error.status, StatusCode::FORBIDDEN);

        let mcp_management = SandboxAuthContext::System(SandboxSystemClient {
            client_id: "mcp-management-service".to_string(),
            scopes: vec![SCOPE_MCP_CALL.to_string()],
            allowed_tenant_ids: vec!["*".to_string()],
            allowed_project_ids: vec!["*".to_string()],
            allowed_tools: vec!["*".to_string()],
            max_lease_ttl_seconds: 3_600,
            internal_identity: None,
        });
        validate_cloud_stdio_runtime_binding(&mcp_management, &lease_record(), Some(&binding))
            .unwrap();
    }

    #[test]
    fn browser_proxy_requires_mcp_management_runtime_binding_and_browser_methods() {
        let mcp_management = SandboxAuthContext::System(SandboxSystemClient {
            client_id: "mcp-management-service".to_string(),
            scopes: vec![SCOPE_MCP_CALL.to_string(), SCOPE_MCP_TOOLS.to_string()],
            allowed_tenant_ids: vec!["*".to_string()],
            allowed_project_ids: vec!["*".to_string()],
            allowed_tools: vec!["*".to_string()],
            max_lease_ttl_seconds: 3_600,
            internal_identity: None,
        });
        let binding = SandboxMcpRuntimeBinding {
            lease_id: "lease-1".to_string(),
            owner_user_id: "tenant-1".to_string(),
            project_id: "project-1".to_string(),
            run_id: "run-1".to_string(),
            runtime_session_id: Some("runtime-session-1".to_string()),
        };
        let allowed = json!({
            "jsonrpc": "2.0",
            "id": "browser-1",
            "method": "tools/call",
            "params": {"name": "browser_navigate", "arguments": {"url": "http://localhost:5173"}}
        });
        assert_eq!(
            validate_browser_mcp_runtime_binding(
                &mcp_management,
                &lease_record(),
                Some(&binding),
                &allowed,
            )
            .unwrap(),
            "runtime-session-1"
        );

        let terminal = json!({
            "jsonrpc": "2.0",
            "id": "terminal-1",
            "method": "tools/call",
            "params": {"name": "execute_command", "arguments": {"command": "id"}}
        });
        assert!(validate_browser_mcp_runtime_binding(
            &mcp_management,
            &lease_record(),
            Some(&binding),
            &terminal,
        )
        .is_err());

        let task_runner = system_auth(&[SCOPE_MCP_CALL], &["*"]);
        assert!(validate_browser_mcp_runtime_binding(
            &task_runner,
            &lease_record(),
            Some(&binding),
            &allowed,
        )
        .is_err());

        let mut missing_session = binding;
        missing_session.runtime_session_id = None;
        assert!(validate_browser_mcp_runtime_binding(
            &mcp_management,
            &lease_record(),
            Some(&missing_session),
            &allowed,
        )
        .is_err());
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

    #[test]
    fn mcp_proxy_strips_ungrantable_command_permission_overlays() {
        let mut payload = json!({
            "jsonrpc": "2.0",
            "id": "request-1",
            "method": "tools/call",
            "params": {
                "name": "execute_command",
                "arguments": {
                    "command": "pwd",
                    "additionalPermissions": null,
                    "_grantedPermissions": { "network": { "enabled": true } }
                }
            }
        });

        strip_ungrantable_command_permission_requests(&mut payload);

        let arguments = payload["params"]["arguments"]
            .as_object()
            .expect("arguments");
        assert_eq!(
            arguments.get("command").and_then(Value::as_str),
            Some("pwd")
        );
        assert!(!arguments.contains_key("additionalPermissions"));
        assert!(!arguments.contains_key("_grantedPermissions"));
    }

    #[test]
    fn mcp_proxy_hides_ungrantable_command_permission_schema() {
        let mut response = json!({
            "jsonrpc": "2.0",
            "id": "request-1",
            "result": {
                "tools": [{
                    "name": "execute_command",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "command": { "type": "string" },
                            "additionalPermissions": { "type": "object" }
                        }
                    }
                }]
            }
        });

        strip_ungrantable_command_permission_schemas(&mut response);

        let properties = response["result"]["tools"][0]["inputSchema"]["properties"]
            .as_object()
            .expect("properties");
        assert!(properties.contains_key("command"));
        assert!(!properties.contains_key("additionalPermissions"));
    }

    #[test]
    fn mcp_proxy_timeout_tracks_terminal_wait_request() {
        let wait = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "call-1",
            "method": "tools/call",
            "params": {
                "name": "process_wait",
                "arguments": {"timeout_ms": 600_000}
            }
        });
        let poll = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "call-2",
            "method": "tools/call",
            "params": {"name": "process_poll", "arguments": {}}
        });
        assert_eq!(
            sandbox_agent_mcp_timeout(&wait),
            Duration::from_millis(615_000)
        );
        assert_eq!(sandbox_agent_mcp_timeout(&poll), SANDBOX_AGENT_MCP_TIMEOUT);
    }
}
