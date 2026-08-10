// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use crate::models::normalize_optional_text;
use crate::models::{
    now_rfc3339, CurrentUser, HealthResponse, LocalConnectorSystemStatsResponse,
    WORKSPACE_STATUS_DISABLED,
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
mod devices;
mod internal_auth;
mod managed_requirements;
mod managed_requirements_admin;
mod managed_runtime_config;
mod plugin_artifact_relay;
mod plugin_management_capabilities;
mod plugin_management_mcps;
mod plugin_management_oauth;
mod plugin_management_plugins;
mod plugin_management_prompts;
mod plugin_management_skills;
mod project_bindings;
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
use self::plugin_management_mcps::{
    create_local_mcp, delete_local_mcp, list_local_mcps, update_local_mcp, update_local_mcp_status,
};
use self::plugin_management_plugins::{
    list_plugin_install_sources, proxy_plugin_release_artifact, update_plugin_preference,
};
use self::plugin_management_prompts::{get_agent_prompt_bundle, get_agent_prompt_bundle_manifest};
use self::plugin_management_skills::{
    list_user_skills, sync_user_skill_inventory, update_user_skill_preference,
};
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
    drop_terminal_subscription, terminal_close_relay, terminal_event_to_ws_payload,
    terminal_exec_relay, terminal_input_relay, terminal_session_create_relay, terminal_ws_relay,
};
use self::workspace_directory_relay::workspace_directory_create_relay;
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

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "local_connector_service".to_string(),
    })
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
    if let Some(workspace_id) = workspace_id.as_deref() {
        validate_device_workspace(&state, &user, device_id.as_str(), workspace_id).await?;
    } else if has_nonempty_header(&headers, "x-local-connector-mcp-manifest-id") {
        let device = load_owned_device(&state, &user, device_id.as_str(), true).await?;
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
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
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
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let response = dispatch_relay(&state, request, state.config.relay_request_timeout).await?;
    Ok(relay_response_to_http(response))
}

async fn plugin_prepare_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<SkillRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    plugin_relay(state, user, device_id, query, "prepare", body).await
}

async fn plugin_execute_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<SkillRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    plugin_relay(state, user, device_id, query, "execute", body).await
}

async fn plugin_cancel_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<SkillRelayQuery>,
    Json(body): Json<Value>,
) -> Result<Response, ApiError> {
    plugin_relay(state, user, device_id, query, "cancel", body).await
}

async fn plugin_ui_asset_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<SkillRelayQuery>,
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
    Query(query): Query<SkillRelayQuery>,
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
    Query(query): Query<SkillRelayQuery>,
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
    Query(query): Query<SkillRelayQuery>,
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
    Query(query): Query<SkillRelayQuery>,
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
    query: SkillRelayQuery,
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
    let relay_timeout = if is_plugin_hook_dispatch(action, &body) {
        state.config.plugin_hook_relay_request_timeout
    } else {
        state.config.relay_request_timeout
    };
    let request = RelayRequest {
        message_type: format!("plugin_{action}_request"),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: format!("/plugins/{action}"),
        headers: BTreeMap::new(),
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
    let relay_timeout = if relay_path == "/api/local/sandbox/images/mcp"
        || relay_path.starts_with("/api/local/sandbox/environments/compose/")
    {
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
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
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

#[cfg(test)]
mod tests {
    use super::{is_local_sandbox_mcp_path, is_plugin_hook_dispatch};
    use serde_json::json;

    #[test]
    fn only_hook_dispatch_uses_the_extended_interactive_relay_window() {
        assert!(is_plugin_hook_dispatch(
            "execute",
            &json!({"operation": "dispatch_hook_event"})
        ));
        assert!(!is_plugin_hook_dispatch(
            "execute",
            &json!({"operation": "mcp_tools_call"})
        ));
        assert!(!is_plugin_hook_dispatch(
            "prepare",
            &json!({"operation": "dispatch_hook_event"})
        ));
    }

    #[test]
    fn only_concrete_sandbox_tool_calls_require_the_mcp_management_caller() {
        assert!(is_local_sandbox_mcp_path("/api/sandboxes/sandbox-1/mcp"));
        assert!(!is_local_sandbox_mcp_path("/api/sandboxes/leases"));
        assert!(!is_local_sandbox_mcp_path("/api/local/sandbox/images/mcp"));
    }
}
