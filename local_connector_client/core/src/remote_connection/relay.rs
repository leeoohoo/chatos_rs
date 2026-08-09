// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::relay::{relay_error_response, RelayRequest, RelayResponse};

use super::runtime::{extract_second_factor_prompt, test_connectivity, RemoteConnectionSpec};

#[derive(Debug, Deserialize)]
struct RemoteConnectionTestBody {
    connection: RemoteConnectionSpec,
    verification_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RemoteConnectionCommandBody {
    connection: RemoteConnectionSpec,
    command: String,
    timeout_ms: Option<u64>,
    verification_code: Option<String>,
}

pub(crate) async fn handle_remote_connection_test_request(value: Value) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(error) => {
            return relay_error_response(
                "remote_connection_test_response",
                "",
                400,
                error.to_string(),
            );
        }
    };
    let body = match serde_json::from_value::<RemoteConnectionTestBody>(request.body) {
        Ok(body) => body,
        Err(error) => {
            return relay_error_response(
                "remote_connection_test_response",
                request.request_id.as_str(),
                400,
                format!("invalid remote connection test body: {error}"),
            );
        }
    };
    let verification_code = body
        .verification_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let result = tokio::task::spawn_blocking(move || {
        test_connectivity(&body.connection, verification_code.as_deref())
    })
    .await
    .map_err(|error| format!("remote connection test worker failed: {error}"))
    .and_then(|result| result);

    match result {
        Ok(remote_host) => RelayResponse {
            message_type: "remote_connection_test_response".to_string(),
            request_id: request.request_id,
            status: 200,
            headers: BTreeMap::new(),
            body: json!({
                "success": true,
                "remote_host": remote_host,
                "connected_at": crate::local_now_rfc3339(),
            }),
        }
        .into_value(),
        Err(error) => error_response("remote_connection_test_response", request.request_id, error),
    }
}

pub(crate) async fn handle_remote_connection_command_request(value: Value) -> Value {
    let request = match serde_json::from_value::<RelayRequest>(value) {
        Ok(request) => request,
        Err(error) => {
            return relay_error_response(
                "remote_connection_command_response",
                "",
                400,
                error.to_string(),
            );
        }
    };
    let body = match serde_json::from_value::<RemoteConnectionCommandBody>(request.body) {
        Ok(body) => body,
        Err(error) => {
            return relay_error_response(
                "remote_connection_command_response",
                request.request_id.as_str(),
                400,
                format!("invalid remote connection command body: {error}"),
            );
        }
    };
    if body.command.trim().is_empty() {
        return relay_error_response(
            "remote_connection_command_response",
            request.request_id.as_str(),
            400,
            "command is required".to_string(),
        );
    }
    let timeout =
        std::time::Duration::from_millis(body.timeout_ms.unwrap_or(30_000).clamp(1_000, 600_000));
    let verification_code = body
        .verification_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let result = tokio::task::spawn_blocking(move || {
        super::runtime::run_command(
            &body.connection,
            body.command.as_str(),
            timeout,
            verification_code.as_deref(),
        )
    })
    .await
    .map_err(|error| format!("remote command worker failed: {error}"))
    .and_then(|result| result);

    match result {
        Ok(output) => RelayResponse {
            message_type: "remote_connection_command_response".to_string(),
            request_id: request.request_id,
            status: 200,
            headers: BTreeMap::new(),
            body: json!({ "output": output }),
        }
        .into_value(),
        Err(error) => error_response(
            "remote_connection_command_response",
            request.request_id,
            error,
        ),
    }
}

fn error_response(message_type: &str, request_id: String, error: String) -> Value {
    let (status, body) = if let Some(prompt) = extract_second_factor_prompt(&error) {
        (
            400,
            json!({
                "error": "需要二次验证",
                "code": "second_factor_required",
                "challenge_prompt": prompt,
            }),
        )
    } else {
        let (status, code) = classify_error(error.as_str());
        (status, json!({ "error": error, "code": code }))
    };
    RelayResponse {
        message_type: message_type.to_string(),
        request_id,
        status,
        headers: BTreeMap::new(),
        body,
    }
    .into_value()
}

fn classify_error(error: &str) -> (u16, &'static str) {
    let normalized = error.to_lowercase();
    if normalized.contains("known_hosts") || normalized.contains("主机指纹") {
        return (400, "host_key_verification_failed");
    }
    if normalized.contains("认证失败") || normalized.contains("authentication") {
        return (401, "auth_failed");
    }
    if normalized.contains("解析") || normalized.contains("name or service not known") {
        return (502, "dns_resolve_failed");
    }
    if normalized.contains("超时") || normalized.contains("timed out") {
        return (408, "network_timeout");
    }
    if normalized.contains("connection refused")
        || normalized.contains("network is unreachable")
        || normalized.contains("连接远端失败")
        || normalized.contains("连接跳板机失败")
    {
        return (502, "network_unreachable");
    }
    (400, "connectivity_test_failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_authentication_failures() {
        assert_eq!(classify_error("SSH 认证失败"), (401, "auth_failed"));
    }

    #[tokio::test]
    async fn rejects_missing_connection_payload() {
        let response = handle_remote_connection_test_request(json!({
            "type": "remote_connection_test_request",
            "request_id": "request-1",
            "owner_user_id": "user-1",
            "device_id": "device-1",
            "workspace_id": "workspace-1",
            "body": {},
        }))
        .await;

        assert_eq!(response.get("status").and_then(Value::as_u64), Some(400));
        assert_eq!(
            response.get("request_id").and_then(Value::as_str),
            Some("request-1")
        );
    }
}
