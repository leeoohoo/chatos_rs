// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    HeaderMap, Method, StatusCode, Uri,
};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use chatos_plugin_management_sdk::{
    ResolveAgentCapabilitiesRequest, SystemAgentKey, LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID,
};
use serde::Deserialize;
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::{
    normalize_optional_text, CurrentUser, HealthResponse, DEVICE_STATUS_ONLINE,
    WORKSPACE_STATUS_DISABLED,
};
use crate::relay::{RelayError, RelayRequest, RelayResponse};
use crate::state::AppState;

mod auth_middleware;
mod devices;
mod internal_auth;
mod memory_engine_proxy;
mod plugin_management_mcps;
mod plugin_management_skills;
mod project_bindings;
mod router;
mod sandbox_pairings;
mod terminal_relay;
mod workspaces;

use self::auth_middleware::require_auth;
pub use self::auth_middleware::ApiError;
use self::devices::{
    connect_device, create_device, disconnect_device, get_device, heartbeat_device, list_devices,
    load_owned_device, revoke_device,
};
use self::memory_engine_proxy::memory_engine_proxy;
use self::plugin_management_mcps::{
    create_local_mcp, delete_local_mcp, list_local_mcps, update_local_mcp, update_local_mcp_status,
};
use self::plugin_management_skills::{
    list_user_skills, sync_user_skill_inventory, update_user_skill_preference,
};
use self::project_bindings::{
    create_project_binding, delete_project_binding, list_project_bindings, update_project_binding,
};
pub use self::router::build_router;
use self::sandbox_pairings::{
    create_sandbox_pairing, delete_sandbox_pairing, list_sandbox_pairings,
    load_owned_sandbox_pairing, update_sandbox_pairing,
};
use self::terminal_relay::{
    terminal_exec_relay, terminal_input_relay, terminal_session_create_relay, terminal_ws_relay,
};
use self::workspaces::{
    create_workspace, delete_workspace, list_workspaces, load_owned_workspace, update_workspace,
};

const MAX_USER_SERVICE_PROXY_BODY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct McpRelayQuery {
    workspace_id: Option<String>,
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SkillRelayQuery {
    workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct LocalCommandApprovalCapabilitiesResponse {
    policy_revision: String,
    code_maintainer_read: bool,
    approval_decision: bool,
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "local_connector_service".to_string(),
    })
}

async fn current_user_handler(Extension(user): Extension<CurrentUser>) -> Json<CurrentUser> {
    Json(user)
}

async fn user_service_public_proxy(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let path = uri.path();
    if method != Method::POST
        || !matches!(
            path,
            "/api/auth/login"
                | "/api/auth/register"
                | "/api/auth/register/send-code"
                | "/api/auth/local-connector-ticket/exchange"
        )
    {
        return Err(ApiError::not_found("user_service proxy route not found"));
    }
    proxy_user_service_request(&state, method, uri, headers, body, false).await
}

async fn user_service_protected_proxy(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let path = uri.path();
    if !is_allowed_model_config_proxy_request(&method, path) {
        return Err(ApiError::not_found("user_service proxy route not found"));
    }
    proxy_user_service_request(&state, method, uri, headers, body, true).await
}

async fn proxy_user_service_request(
    state: &AppState,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
    forward_authorization: bool,
) -> Result<Response, ApiError> {
    if body.len() > MAX_USER_SERVICE_PROXY_BODY_BYTES {
        return Err(ApiError::bad_request(
            "user_service proxy request body is too large",
        ));
    }
    let mut target_url = format!(
        "{}{}",
        state.config.user_service_base_url.trim_end_matches('/'),
        uri.path()
    );
    if let Some(query) = uri.query().map(str::trim).filter(|value| !value.is_empty()) {
        target_url.push('?');
        target_url.push_str(query);
    }

    let client = reqwest::Client::builder()
        .timeout(state.config.user_service_request_timeout)
        .build()
        .map_err(|err| ApiError::internal(format!("build user_service client failed: {err}")))?;
    let mut request = client.request(method, target_url.as_str());
    if let Some(content_type) = headers.get(CONTENT_TYPE) {
        request = request.header(CONTENT_TYPE.as_str(), content_type);
    }
    if let Some(accept) = headers.get(ACCEPT) {
        request = request.header(ACCEPT.as_str(), accept);
    }
    if forward_authorization {
        if let Some(authorization) = headers.get(AUTHORIZATION) {
            request = request.header(AUTHORIZATION.as_str(), authorization);
        }
    }
    if !body.is_empty() {
        request = request.body(body.clone());
    }

    let response = request
        .send()
        .await
        .map_err(|err| ApiError::bad_gateway(format!("user_service request failed: {err}")))?;
    let status = StatusCode::from_u16(response.status().as_u16()).map_err(|err| {
        ApiError::bad_gateway(format!("user_service returned invalid status: {err}"))
    })?;
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = response.bytes().await.map_err(|err| {
        ApiError::bad_gateway(format!("read user_service response failed: {err}"))
    })?;
    let mut builder = Response::builder().status(status);
    if let Some(content_type) = content_type {
        builder = builder.header(CONTENT_TYPE, content_type);
    }
    builder.body(Body::from(bytes)).map_err(|err| {
        ApiError::internal(format!("build user_service proxy response failed: {err}"))
    })
}

fn is_allowed_model_config_proxy_request(method: &Method, path: &str) -> bool {
    if path == "/api/model-configs" {
        return matches!(method, &Method::GET | &Method::POST);
    }
    if path == "/api/model-configs/settings" {
        return matches!(method, &Method::GET | &Method::PUT);
    }
    if path
        .strip_prefix("/api/model-configs/")
        .is_some_and(|suffix| !suffix.trim_matches('/').is_empty())
    {
        return matches!(
            method,
            &Method::GET | &Method::PATCH | &Method::DELETE | &Method::POST
        );
    }
    false
}

async fn resolve_local_command_approval_capabilities(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<LocalCommandApprovalCapabilitiesResponse>, ApiError> {
    let owner_user_id = user.effective_owner_user_id();
    let request = ResolveAgentCapabilitiesRequest::new(
        SystemAgentKey::LocalConnectorCommandApprovalAgent,
        owner_user_id,
    );
    let capabilities = state
        .plugin_management_client
        .resolve_for_service(&request)
        .await
        .map_err(|err| ApiError::service_unavailable(err.to_string()))?;
    capabilities
        .ensure_required_available()
        .map_err(|err| ApiError::service_unavailable(err.to_string()))?;
    capabilities
        .ensure_required_skills_supported(std::iter::empty::<&str>())
        .map_err(|err| ApiError::service_unavailable(err.to_string()))?;
    let code_maintainer_read = capabilities.mcps.iter().any(|item| {
        item.binding.required
            && item.available
            && item.resource.runtime.kind == "builtin"
            && item.resource.runtime.builtin_kind.as_deref() == Some("CodeMaintainerRead")
    });
    let approval_decision = capabilities
        .require_available_mcp(LOCAL_CONNECTOR_APPROVAL_MCP_RESOURCE_ID)
        .is_ok();
    if !code_maintainer_read || !approval_decision {
        return Err(ApiError::service_unavailable(
            "local command approval agent required capabilities are unavailable",
        ));
    }
    Ok(Json(LocalCommandApprovalCapabilitiesResponse {
        policy_revision: capabilities.policy_revision,
        code_maintainer_read,
        approval_decision,
    }))
}

async fn mcp_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<McpRelayQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let workspace_id = normalize_optional_text(query.workspace_id);
    if let Some(workspace_id) = workspace_id.as_deref() {
        validate_device_workspace(&state, &user, device_id.as_str(), workspace_id).await?;
    } else if has_nonempty_header(&headers, "x-local-connector-mcp-manifest-id") {
        let device = load_owned_device(&state, &user, device_id.as_str(), true).await?;
        if device.status != DEVICE_STATUS_ONLINE {
            return Err(ApiError::service_unavailable(
                "Local Connector device is offline",
            ));
        }
        ensure_device_active_lease(&state, user.effective_owner_user_id(), device.id.as_str())
            .await?;
    } else {
        return Err(ApiError::bad_request("workspace_id is required"));
    }
    let mut relay_headers = relay_headers(&headers);
    if workspace_id.is_some() {
        if let Some(cwd) = normalize_optional_text(query.cwd) {
            relay_headers.insert("x-local-connector-cwd".to_string(), cwd);
        }
    }
    let request = RelayRequest {
        message_type: "mcp".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id: workspace_id.unwrap_or_default(),
        method: "POST".to_string(),
        path: "/mcp".to_string(),
        headers: relay_headers,
        body: relay_body(body.as_ref()),
    };
    let response = dispatch_relay(&state, request, state.config.relay_request_timeout).await?;
    Ok(relay_response_to_http(response))
}

async fn skill_prepare_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<SkillRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    skill_relay(state, user, device_id, query, "prepare", body).await
}

async fn skill_execute_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<SkillRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    skill_relay(state, user, device_id, query, "execute", body).await
}

async fn skill_cancel_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<SkillRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    skill_relay(state, user, device_id, query, "cancel", body).await
}

async fn skill_relay(
    state: AppState,
    user: CurrentUser,
    device_id: String,
    query: SkillRelayQuery,
    action: &str,
    body: Value,
) -> Result<Response, ApiError> {
    let workspace_id = normalize_optional_text(query.workspace_id)
        .or_else(|| {
            body.get("workspace_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    if workspace_id.is_empty() {
        load_owned_device(&state, &user, device_id.as_str(), true).await?;
        ensure_device_active_lease(&state, user.effective_owner_user_id(), device_id.as_str())
            .await?;
    } else {
        validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    }
    let request = RelayRequest {
        message_type: format!("skill_{action}_request"),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: format!("/skills/{action}"),
        headers: BTreeMap::new(),
        body,
    };
    let response = dispatch_relay(&state, request, state.config.relay_request_timeout).await?;
    Ok(relay_response_to_http(response))
}

async fn resolve_model_runtime(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(model_config_id): Path<String>,
) -> Result<Response, ApiError> {
    let model_config_id = required_text(Some(model_config_id), "model_config_id")?;
    let owner_user_id = user.effective_owner_user_id().to_string();
    let session = state
        .store
        .active_session(owner_user_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| {
            ApiError::service_unavailable(
                "Local Connector client is offline; model request was terminated",
            )
        })?;
    let device = load_owned_device(&state, &user, session.device_id.as_str(), true).await?;

    let request = RelayRequest {
        message_type: "model_runtime_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id,
        device_id: device.id,
        workspace_id: String::new(),
        method: "GET".to_string(),
        path: format!("/model-runtime/{model_config_id}"),
        headers: BTreeMap::new(),
        body: json!({ "model_config_id": model_config_id }),
    };
    let response = dispatch_relay(&state, request, state.config.relay_request_timeout).await?;
    Ok(relay_response_to_http(response))
}

async fn sandbox_facade_root(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(pairing_id): Path<String>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    sandbox_facade_impl(
        state,
        user,
        pairing_id,
        String::new(),
        method,
        headers,
        body,
    )
    .await
}

async fn sandbox_facade_path(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((pairing_id, path)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    sandbox_facade_impl(state, user, pairing_id, path, method, headers, body).await
}

async fn sandbox_facade_impl(
    state: AppState,
    user: CurrentUser,
    pairing_id: String,
    path: String,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let pairing = load_owned_sandbox_pairing(&state, &user, pairing_id.as_str()).await?;
    if !pairing.enabled {
        return Err(ApiError::bad_request(
            "Local Connector sandbox pairing is disabled",
        ));
    }
    validate_device_workspace(
        &state,
        &user,
        pairing.device_id.as_str(),
        pairing.workspace_id.as_str(),
    )
    .await?;

    let relay_path = normalize_relay_path(path.as_str());
    let relay_timeout = if relay_path == "/api/local/sandbox/images/mcp" {
        state.config.sandbox_image_relay_request_timeout
    } else {
        state.config.relay_request_timeout
    };
    let request = RelayRequest {
        message_type: "sandbox_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id: pairing.device_id.clone(),
        workspace_id: pairing.workspace_id.clone(),
        method: method.as_str().to_string(),
        path: relay_path,
        headers: relay_headers(&headers),
        body: relay_body(body.as_ref()),
    };

    let response = dispatch_relay(&state, request, relay_timeout).await?;
    Ok(relay_response_to_http(response))
}

async fn validate_device_workspace(
    state: &AppState,
    user: &CurrentUser,
    device_id: &str,
    workspace_id: &str,
) -> Result<(), ApiError> {
    let device = load_owned_device(state, user, device_id, true).await?;
    if device.status != DEVICE_STATUS_ONLINE {
        return Err(ApiError::service_unavailable(
            "Local Connector device is offline",
        ));
    }
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

fn has_nonempty_header(headers: &HeaderMap, name: &str) -> bool {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
}

fn relay_body(body: &[u8]) -> Value {
    if body.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice::<Value>(body)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(body).into_owned()))
}

fn relay_error_to_api_error(error: RelayError) -> ApiError {
    match error {
        RelayError::Offline => ApiError::service_unavailable(error.message()),
        RelayError::Timeout => ApiError::gateway_timeout(error.message()),
        RelayError::RequestEncode(_) | RelayError::ResponseChannelClosed => {
            ApiError::bad_gateway(error.message())
        }
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
