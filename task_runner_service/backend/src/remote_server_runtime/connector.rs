// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{json, Value};

use crate::config::AppConfig;
use crate::models::{now_rfc3339, RemoteServerRecord, RemoteServerTestResponse};
use crate::trace_context::InternalTraceContextExt;

const REMOTE_CONNECTION_RELAY_SCOPE: &str = "remote-connection.execute";
const LOCAL_CONNECTOR_TOKEN_AUDIENCE: &str = "local-connector-service";
const RELAY_RESPONSE_LIMIT_BYTES: usize = 12 * 1024 * 1024;

pub async fn test_remote_server_connectivity(
    config: &AppConfig,
    owner_user_id: &str,
    server: &RemoteServerRecord,
    server_id: Option<String>,
) -> Result<RemoteServerTestResponse, String> {
    let value = relay_request(
        config,
        owner_user_id,
        server,
        "test",
        json!({
            "connection": connection_payload(server),
        }),
        Duration::from_secs(20),
    )
    .await?;
    Ok(RemoteServerTestResponse {
        ok: true,
        server_id,
        name: server.name.clone(),
        host: server.host.clone(),
        port: server.port,
        username: server.username.clone(),
        auth_type: server.auth_type.clone(),
        remote_host: value
            .get("remote_host")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        error: None,
        tested_at: value
            .get("connected_at")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(now_rfc3339),
    })
}

pub(super) async fn run_remote_command(
    config: &AppConfig,
    owner_user_id: &str,
    server: &RemoteServerRecord,
    command: &str,
    timeout: Duration,
) -> Result<String, String> {
    let value = relay_request(
        config,
        owner_user_id,
        server,
        "command",
        json!({
            "connection": connection_payload(server),
            "command": command,
            "timeout_ms": timeout.as_millis().clamp(1_000, 600_000) as u64,
        }),
        timeout.saturating_add(Duration::from_secs(10)),
    )
    .await?;
    value
        .get("output")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| "Local Connector remote command response is missing output".to_string())
}

pub(super) async fn remote_sftp_request(
    config: &AppConfig,
    owner_user_id: &str,
    server: &RemoteServerRecord,
    operation: &str,
    payload: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let mut body = payload
        .as_object()
        .cloned()
        .ok_or_else(|| "remote SFTP relay payload must be an object".to_string())?;
    body.insert(
        "operation".to_string(),
        Value::String(operation.to_string()),
    );
    body.insert(
        "connection_id".to_string(),
        Value::String(server.id.clone()),
    );
    body.insert("connection".to_string(), connection_payload(server));
    relay_request(
        config,
        owner_user_id,
        server,
        "sftp",
        Value::Object(body),
        timeout,
    )
    .await
}

async fn relay_request(
    config: &AppConfig,
    owner_user_id: &str,
    server: &RemoteServerRecord,
    operation: &str,
    mut body: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let device_id = required_connector_field(
        server.local_connector_device_id.as_deref(),
        "local_connector_device_id",
    )?;
    let workspace_id = required_connector_field(
        server.local_connector_workspace_id.as_deref(),
        "local_connector_workspace_id",
    )?;
    let object = body
        .as_object_mut()
        .ok_or_else(|| "Local Connector relay body must be an object".to_string())?;
    object.insert(
        "workspace_id".to_string(),
        Value::String(workspace_id.to_string()),
    );
    let base_url = config
        .local_connector_service_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL is required for remote connections"
                .to_string()
        })?
        .trim_end_matches('/');
    let secret = config
        .local_connector_internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET is required for remote connections"
                .to_string()
        })?;
    let owner_user_id = owner_user_id.trim();
    if owner_user_id.is_empty() {
        return Err("remote connection owner user id is required".to_string());
    }
    let token = chatos_service_runtime::issue_internal_service_token_for_owner(
        secret,
        "task-runner",
        LOCAL_CONNECTOR_TOKEN_AUDIENCE,
        REMOTE_CONNECTION_RELAY_SCOPE,
        timeout.as_secs().clamp(60, 600),
        owner_user_id,
    )
    .map_err(|error| format!("issue remote connection relay token failed: {error}"))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-local-connector-caller",
        HeaderValue::from_static("task-runner"),
    );
    headers.insert(
        "x-local-connector-internal-token",
        HeaderValue::from_str(token.as_str())
            .map_err(|_| "remote connection relay token is not a valid header".to_string())?,
    );
    headers.insert(
        "x-local-connector-owner-user-id",
        HeaderValue::from_str(owner_user_id)
            .map_err(|_| "remote connection owner user id is not a valid header".to_string())?,
    );
    let url = format!(
        "{base_url}/api/local-connectors/relay/{}/remote-connections/{operation}",
        urlencoding::encode(device_id),
    );
    let response = config
        .local_connector_http_client
        .post(url)
        .headers(headers)
        .json(&body)
        .timeout(timeout.max(config.local_connector_service_request_timeout))
        .with_internal_trace_context()
        .send()
        .await
        .map_err(|error| format!("Local Connector remote {operation} request failed: {error}"))?;
    let status = response.status();
    let bytes = read_response_bytes_limited(response, RELAY_RESPONSE_LIMIT_BYTES)
        .await
        .map_err(|error| {
            format!("read Local Connector remote {operation} response failed: {error}")
        })?;
    let value = serde_json::from_slice::<Value>(bytes.as_slice()).map_err(|error| {
        format!("decode Local Connector remote {operation} response failed: {error}")
    })?;
    if !status.is_success() {
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Local Connector rejected the remote connection request");
        let code = value
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return Err(format!(
            "Local Connector remote {operation} failed ({code}): {message}"
        ));
    }
    Ok(value)
}

fn connection_payload(server: &RemoteServerRecord) -> Value {
    json!({
        "host": server.host,
        "port": server.port,
        "username": server.username,
        "auth_type": server.auth_type,
        "password": server.password,
        "private_key_path": server.private_key_path,
        "certificate_path": server.certificate_path,
        "host_key_policy": server.host_key_policy,
        "jump_enabled": false,
        "jump_host": null,
        "jump_port": null,
        "jump_username": null,
        "jump_private_key_path": null,
        "jump_certificate_path": null,
        "jump_password": null,
    })
}

fn required_connector_field<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("remote server must configure {field} before execution"))
}
