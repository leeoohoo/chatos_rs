// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::time::Duration;

use crate::models::normalize_optional_text;
use crate::models::{
    now_rfc3339, CurrentUser, LocalConnectorSystemStatsResponse, LocalConnectorWorkspace,
    WORKSPACE_STATUS_ACTIVE, WORKSPACE_STATUS_DISABLED,
};
use crate::relay::{
    plugin_artifact_relay_request, PluginArtifactRelayAction, RelayError, RelayRequest,
    RelayResponse,
};
use crate::state::AppState;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    HeaderMap, Method, StatusCode, Uri,
};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use chatos_service_runtime::http_body::{
    read_response_bytes_limited, DEFAULT_RESPONSE_BODY_LIMIT_BYTES,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

mod auth_middleware;
mod companion;
mod devices;
mod internal_auth;
mod managed_requirements;
mod managed_requirements_admin;
mod managed_runtime_config;
mod metrics;
mod plugin_artifact_relay;
mod plugin_management_capabilities;
mod plugin_management_installations;
mod plugin_management_oauth;
mod plugin_management_plugins;
mod plugin_management_prompts;
mod project_bindings;
mod project_context;
mod remote_connection_relay;
mod router;
mod sandbox_pairings;
mod terminal_relay;
mod workspace_directory_relay;
mod workspaces;

pub use self::auth_middleware::ApiError;
use self::auth_middleware::{require_internal_auth, require_public_auth, AuthState};
use self::devices::{
    connect_device, create_device, disconnect_device, get_device, heartbeat_device, list_devices,
    load_owned_device, revoke_device,
};
use self::internal_auth::require_chatos_service_caller;
use self::managed_requirements::get_managed_requirements;
use self::managed_requirements_admin::{
    create_managed_requirements_assignment, create_managed_requirements_policy,
    delete_managed_requirements_assignment, delete_managed_requirements_policy,
    list_managed_requirements_assignments, list_managed_requirements_policies,
    update_managed_requirements_assignment, update_managed_requirements_policy,
};
use self::plugin_artifact_relay::PluginArtifactRelayState;
#[cfg(feature = "test-support")]
pub use self::plugin_artifact_relay::PluginArtifactRelayTestScope;
use self::plugin_management_capabilities::resolve_local_runtime_capabilities;
use self::plugin_management_plugins::{
    list_plugin_install_sources, proxy_plugin_release_artifact, update_plugin_preference,
};
use self::plugin_management_prompts::{get_agent_prompt_bundle, get_agent_prompt_bundle_manifest};
use self::project_bindings::{
    create_project_binding, delete_project_binding, list_project_bindings, update_project_binding,
};
use self::remote_connection_relay::{
    remote_connection_command_relay, remote_connection_test_relay, remote_sftp_relay,
    remote_terminal_close_relay, remote_terminal_ws_relay,
};
pub use self::router::{build_internal_router, build_public_router};
#[cfg(feature = "test-support")]
pub use self::router::{
    build_plugin_artifact_relay_store_test_router, build_plugin_artifact_relay_test_router,
};
use self::sandbox_pairings::{
    create_sandbox_pairing, delete_sandbox_pairing, list_sandbox_pairings,
    load_owned_sandbox_pairing, update_sandbox_pairing,
};
use self::terminal_relay::{
    controlled_network_readiness, drop_terminal_subscription, terminal_close_relay,
    terminal_event_to_ws_payload, terminal_exec_relay, terminal_input_relay,
    terminal_session_create_relay, terminal_ws_relay,
};
use self::workspace_directory_relay::{
    workspace_directory_create_relay, workspace_directory_list_relay, workspace_filesystem_relay,
};
use self::workspaces::{
    create_workspace, delete_workspace, list_workspaces, load_owned_workspace, update_workspace,
};

const MAX_USER_SERVICE_PROXY_BODY_BYTES: usize = 2 * 1024 * 1024;
// Ordinary MCP tools are long-running operations. Keep this transport aligned
// with the platform-wide two-hour MCP execution budget instead of inheriting
// the short control-plane relay timeout.
const STANDARD_MCP_RELAY_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
const MCP_TERMINAL_WAIT_TRANSPORT_GRACE_MS: u64 = 15_000;
const MCP_TERMINAL_WAIT_MAX_TIMEOUT_MS: u64 = 10 * 60 * 1_000;
const NATIVE_REMOTE_CONNECTION_DEVICE_ALIAS: &str = "chatos-swift-native-client";
const NATIVE_REMOTE_CONNECTION_WORKSPACE_ALIAS: &str = "local-machine";

#[derive(Debug, Deserialize)]
struct McpRelayQuery {
    workspace_id: Option<String>,
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PluginRelayQuery {
    workspace_id: Option<String>,
    cwd: Option<String>,
}

async fn system_stats_handler(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<LocalConnectorSystemStatsResponse>, ApiError> {
    if user.principal_type != "service" && !user.is_super_admin() {
        return Err(ApiError::forbidden(
            "Local Connector system stats are restricted to service callers or super admins",
        ));
    }
    let relay = state.relay.stats().await;
    let store = state
        .store
        .system_stats()
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(LocalConnectorSystemStatsResponse {
        ok: true,
        service: "local_connector_service".to_string(),
        now: now_rfc3339(),
        pressure_level: state.pressure.snapshot().level,
        relay,
        store,
    }))
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

    let mut request = state
        .user_service_http()
        .request(method, target_url.as_str());
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
    let bytes = read_response_bytes_limited(response, DEFAULT_RESPONSE_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| {
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
    if path == "/api/model-providers" {
        return matches!(method, &Method::GET | &Method::POST);
    }
    if path
        .strip_prefix("/api/model-providers/")
        .is_some_and(|suffix| !suffix.trim_matches('/').is_empty())
    {
        return matches!(
            method,
            &Method::GET | &Method::PATCH | &Method::DELETE | &Method::POST
        );
    }
    false
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
    let (device_id, workspace_id) =
        resolve_native_remote_connection_relay_target(&state, &user, device_id, workspace_id)
            .await?;
    if let Some(workspace_id) = workspace_id.as_deref() {
        validate_device_workspace(&state, &user, device_id.as_str(), workspace_id).await?;
    } else if !has_inline_http_mcp_runtime_header(&headers) {
        return Err(ApiError::bad_request("workspace_id is required"));
    }
    let mut relay_headers = relay_headers(&headers);
    if workspace_id.is_some() {
        if let Some(cwd) = normalize_optional_text(query.cwd) {
            relay_headers.insert("x-local-connector-cwd".to_string(), cwd);
        }
    }
    let relay_body = relay_body(body.as_ref());
    let relay_timeout = mcp_relay_timeout(state.config.relay_request_timeout, &relay_body);
    let request = RelayRequest {
        message_type: "mcp".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id: workspace_id.unwrap_or_default(),
        method: "POST".to_string(),
        path: "/mcp".to_string(),
        headers: relay_headers,
        body: relay_body,
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let response = dispatch_relay(&state, request, relay_timeout).await?;
    Ok(relay_response_to_http(response))
}

async fn resolve_native_remote_connection_relay_target(
    state: &AppState,
    user: &CurrentUser,
    device_id: String,
    workspace_id: Option<String>,
) -> Result<(String, Option<String>), ApiError> {
    if device_id != NATIVE_REMOTE_CONNECTION_DEVICE_ALIAS
        || workspace_id.as_deref() != Some(NATIVE_REMOTE_CONNECTION_WORKSPACE_ALIAS)
    {
        return Ok((device_id, workspace_id));
    }
    let owner_user_id = user.effective_owner_user_id();
    let session = state
        .store
        .active_session(owner_user_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| {
            ApiError::service_unavailable(
                "no active Local Connector device is available for the selected remote connection",
            )
        })?;
    let workspaces = state
        .store
        .list_workspaces(owner_user_id, Some(session.device_id.clone()))
        .await
        .map_err(ApiError::internal)?;
    let workspace = active_remote_connection_workspace(workspaces.as_slice()).ok_or_else(|| {
        ApiError::service_unavailable(
            "the active Local Connector device has no available workspace for remote connection relay",
        )
    })?;
    Ok((session.device_id, Some(workspace.id.clone())))
}

fn active_remote_connection_workspace(
    workspaces: &[LocalConnectorWorkspace],
) -> Option<&LocalConnectorWorkspace> {
    workspaces
        .iter()
        .find(|workspace| workspace.status == WORKSPACE_STATUS_ACTIVE)
}

async fn plugin_prepare_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<PluginRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    plugin_relay(state, user, device_id, query, "prepare", body).await
}

async fn plugin_execute_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<PluginRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    plugin_relay(state, user, device_id, query, "execute", body).await
}

async fn plugin_cancel_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<PluginRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    plugin_relay(state, user, device_id, query, "cancel", body).await
}

async fn plugin_ui_asset_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<PluginRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    require_chatos_service_caller(&user)?;
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
        message_type: "plugin_ui_asset_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/plugins/ui/assets".to_string(),
        headers: BTreeMap::new(),
        body,
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let response = dispatch_relay(&state, request, state.config.relay_request_timeout).await?;
    Ok(relay_response_to_http(response))
}

async fn plugin_artifact_list_relay(
    State(state): State<PluginArtifactRelayState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<PluginRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    plugin_artifact_relay(
        state,
        user,
        device_id,
        query,
        PluginArtifactRelayAction::List,
        body,
    )
    .await
}

async fn plugin_artifact_read_relay(
    State(state): State<PluginArtifactRelayState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<PluginRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    plugin_artifact_relay(
        state,
        user,
        device_id,
        query,
        PluginArtifactRelayAction::Read,
        body,
    )
    .await
}

async fn plugin_artifact_create_relay(
    State(state): State<PluginArtifactRelayState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<PluginRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    plugin_artifact_relay(
        state,
        user,
        device_id,
        query,
        PluginArtifactRelayAction::Create,
        body,
    )
    .await
}

async fn plugin_artifact_update_relay(
    State(state): State<PluginArtifactRelayState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<PluginRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    plugin_artifact_relay(
        state,
        user,
        device_id,
        query,
        PluginArtifactRelayAction::Update,
        body,
    )
    .await
}

async fn plugin_artifact_relay(
    state: PluginArtifactRelayState,
    user: CurrentUser,
    device_id: String,
    query: PluginRelayQuery,
    action: PluginArtifactRelayAction,
    body: Value,
) -> Result<Response, ApiError> {
    require_chatos_service_caller(&user)?;
    let workspace_id = normalize_optional_text(query.workspace_id)
        .or_else(|| {
            body.get("workspace_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    if workspace_id.is_empty() {
        return Err(ApiError::bad_request(
            "workspace_id is required for Plugin Artifact access",
        ));
    }
    state
        .authorize(&user, device_id.as_str(), workspace_id.as_str())
        .await?;
    let request = plugin_artifact_relay_request(
        user.effective_owner_user_id(),
        device_id,
        workspace_id,
        action,
        body,
    );
    let relay_timeout = if action.is_write() {
        state.write_timeout
    } else {
        state.read_timeout
    };
    let response = state
        .relay
        .dispatch(request, relay_timeout)
        .await
        .map_err(relay_error_to_api_error)?;
    Ok(relay_response_to_http(response))
}

async fn plugin_relay(
    state: AppState,
    user: CurrentUser,
    device_id: String,
    query: PluginRelayQuery,
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
    let relay_timeout = plugin_relay_timeout(
        state.config.relay_request_timeout,
        state.config.plugin_hook_relay_request_timeout,
        action,
        &body,
    );
    let mut relay_headers = BTreeMap::new();
    if let Some(cwd) = normalize_optional_text(query.cwd) {
        relay_headers.insert("x-local-connector-cwd".to_string(), cwd);
    }
    let request = RelayRequest {
        message_type: format!("plugin_{action}_request"),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: format!("/plugins/{action}"),
        headers: relay_headers,
        body,
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let response = dispatch_relay(&state, request, relay_timeout).await?;
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
    if is_local_sandbox_mcp_path(relay_path.as_str()) {
        internal_auth::require_mcp_management_service_caller(&user)?;
    }
    let relay_timeout = state.config.relay_request_timeout;
    let request = RelayRequest {
        message_type: "lease_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id: pairing.device_id.clone(),
        workspace_id: pairing.workspace_id.clone(),
        method: method.as_str().to_string(),
        path: relay_path,
        headers: relay_headers(&headers),
        body: relay_body(body.as_ref()),
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };

    let response = dispatch_relay(&state, request, relay_timeout).await?;
    Ok(relay_response_to_http(response))
}

include!("mod_part01.rs");
