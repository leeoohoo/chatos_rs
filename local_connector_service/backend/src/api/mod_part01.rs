async fn validate_device_workspace(
    state: &AppState,
    user: &CurrentUser,
    device_id: &str,
    workspace_id: &str,
) -> Result<(), ApiError> {
    let device = load_owned_device(state, user, device_id, true).await?;
    // The active lease and relay connection are the authoritative online signal.
    // The persisted device status is updated by heartbeats and can briefly lag a
    // successful reconnect, which previously caused valid local project requests
    // to fail with a stale "device is offline" response.
    ensure_device_active_lease(state, user.effective_owner_user_id(), device_id).await?;
    let workspace = load_owned_workspace(state, user, workspace_id).await?;
    if workspace.device_id != device.id {
        return Err(ApiError::bad_request(
            "Local Connector workspace is not attached to the selected device",
        ));
    }
    if workspace.status == WORKSPACE_STATUS_DISABLED {
        return Err(ApiError::bad_request(
            "Local Connector workspace is disabled",
        ));
    }
    Ok(())
}

async fn ensure_device_active_lease(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
) -> Result<(), ApiError> {
    let active = state
        .store
        .session_holds_active_lease(owner_user_id, device_id)
        .await
        .map_err(ApiError::internal)?;
    if !active {
        return Err(ApiError::service_unavailable(
            "Local Connector device does not hold the active session lease",
        ));
    }
    Ok(())
}

async fn dispatch_relay(
    state: &AppState,
    request: RelayRequest,
    timeout: std::time::Duration,
) -> Result<RelayResponse, ApiError> {
    ensure_device_active_lease(
        state,
        request.owner_user_id.as_str(),
        request.device_id.as_str(),
    )
    .await?;
    state
        .relay
        .dispatch(request, timeout)
        .await
        .map_err(relay_error_to_api_error)
}

async fn dispatch_companion_relay(
    state: &AppState,
    request: RelayRequest,
    timeout: std::time::Duration,
    client_session_id: &str,
) -> Result<RelayResponse, ApiError> {
    ensure_device_active_lease(
        state,
        request.owner_user_id.as_str(),
        request.device_id.as_str(),
    )
    .await?;
    state
        .relay
        .dispatch_companion(request, timeout, client_session_id)
        .await
        .map_err(relay_error_to_api_error)
}

async fn send_relay(state: &AppState, request: RelayRequest) -> Result<(), ApiError> {
    ensure_device_active_lease(
        state,
        request.owner_user_id.as_str(),
        request.device_id.as_str(),
    )
    .await?;
    state
        .relay
        .send(request)
        .await
        .map_err(relay_error_to_api_error)
}

fn normalize_relay_path(path: &str) -> String {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn relay_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(key, value)| {
            let key = key.as_str().to_ascii_lowercase();
            if matches!(
                key.as_str(),
                "authorization"
                    | "cookie"
                    | "set-cookie"
                    | "x-local-connector-caller"
                    | "x-local-connector-internal-token"
                    | "x-local-connector-internal-secret"
                    | "x-local-connector-owner-user-id"
                    | "x-chatos-owner-user-id"
            ) {
                return None;
            }
            value.to_str().ok().map(|value| (key, value.to_string()))
        })
        .collect()
}

fn is_local_sandbox_mcp_path(path: &str) -> bool {
    let parts = path.trim_matches('/').split('/').collect::<Vec<_>>();
    matches!(parts.as_slice(), ["api", "sandboxes", _, "mcp"])
}

fn has_nonempty_header(headers: &HeaderMap, name: &str) -> bool {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
}

fn has_inline_http_mcp_runtime_header(headers: &HeaderMap) -> bool {
    has_nonempty_header(headers, "x-local-connector-inline-mcp-runtime")
}

fn relay_body(body: &[u8]) -> Value {
    if body.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice::<Value>(body)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(body).into_owned()))
}

fn mcp_relay_timeout(configured_timeout: Duration, body: &Value) -> Duration {
    let baseline = configured_timeout.max(STANDARD_MCP_RELAY_TIMEOUT);
    if !is_terminal_wait_mcp_call(body) {
        return baseline;
    }
    let arguments = body.pointer("/params/arguments").unwrap_or(&Value::Null);
    let requested_timeout_ms = arguments
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .or_else(|| {
            arguments
                .get("timeout")
                .and_then(Value::as_u64)
                .map(|seconds| seconds.saturating_mul(1_000))
        })
        .unwrap_or(30_000)
        .clamp(1_000, MCP_TERMINAL_WAIT_MAX_TIMEOUT_MS);
    baseline.max(Duration::from_millis(
        requested_timeout_ms.saturating_add(MCP_TERMINAL_WAIT_TRANSPORT_GRACE_MS),
    ))
}

fn is_terminal_wait_mcp_call(body: &Value) -> bool {
    if body.get("method").and_then(Value::as_str) != Some("tools/call") {
        return false;
    }
    let Some(tool_name) = body.pointer("/params/name").and_then(Value::as_str) else {
        return false;
    };
    let tool_name = tool_name.trim();
    tool_name == "process_wait"
        || tool_name.ends_with("_process_wait")
        || ((tool_name == "process" || tool_name.ends_with("_process"))
            && body
                .pointer("/params/arguments/action")
                .and_then(Value::as_str)
                == Some("wait"))
}

fn relay_error_to_api_error(error: RelayError) -> ApiError {
    match error {
        RelayError::Offline => ApiError::service_unavailable(error.message()),
        RelayError::Timeout => ApiError::gateway_timeout(error.message()),
        RelayError::TooManyPendingRequests { .. } => ApiError::too_many_requests(error.message()),
        RelayError::Coordination(_) => ApiError::service_unavailable(error.message()),
        RelayError::RequestEncode(_)
        | RelayError::Signing(_)
        | RelayError::DuplicateRequestId(_)
        | RelayError::ResponseChannelClosed => ApiError::bad_gateway(error.message()),
    }
}

fn relay_response_to_http(response: RelayResponse) -> Response {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::BAD_GATEWAY);
    (status, Json(response.body)).into_response()
}

fn required_text(value: Option<String>, field: &str) -> Result<String, ApiError> {
    normalize_optional_text(value)
        .ok_or_else(|| ApiError::bad_request(format!("{field} is required and cannot be empty")))
}

fn is_plugin_hook_dispatch(action: &str, body: &Value) -> bool {
    action == "execute"
        && body.get("operation").and_then(Value::as_str) == Some("dispatch_hook_event")
}

fn plugin_relay_timeout(
    configured_timeout: Duration,
    plugin_hook_timeout: Duration,
    action: &str,
    body: &Value,
) -> Duration {
    let configured_timeout = if is_plugin_hook_dispatch(action, body) {
        plugin_hook_timeout
    } else {
        configured_timeout
    };
    if matches!(action, "prepare" | "execute") {
        configured_timeout.max(STANDARD_MCP_RELAY_TIMEOUT)
    } else {
        configured_timeout
    }
}

#[cfg(test)]
include!("mod_inline_tests.rs");
