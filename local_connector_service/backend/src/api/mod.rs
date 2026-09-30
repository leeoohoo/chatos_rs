// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::{
    now_rfc3339, CurrentUser, LocalConnectorSystemStatsResponse, WORKSPACE_STATUS_DISABLED,
};
use crate::relay::{RelayError, RelayRequest, RelayResponse};
use crate::state::AppState;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    HeaderMap, Method, StatusCode, Uri,
};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use chatos_service_runtime::http_body::{
    read_response_bytes_limited, DEFAULT_RESPONSE_BODY_LIMIT_BYTES,
};

mod auth_middleware;
mod companion;
mod controlled_network_readiness;
mod devices;
mod managed_requirements;
mod managed_requirements_admin;
mod managed_runtime_config;
mod metrics;
mod plugin_management_capabilities;
mod plugin_management_installations;
mod plugin_management_oauth;
mod plugin_management_plugins;
mod plugin_management_prompts;
mod project_bindings;
mod router;
mod workspaces;

pub use self::auth_middleware::ApiError;
use self::auth_middleware::{require_public_auth, AuthState};
use self::controlled_network_readiness::controlled_network_readiness;
use self::devices::{
    connect_device, create_device, disconnect_device, get_device, heartbeat_device, list_devices,
    load_owned_device, revoke_device,
};
use self::managed_requirements::get_managed_requirements;
use self::managed_requirements_admin::{
    create_managed_requirements_assignment, create_managed_requirements_policy,
    delete_managed_requirements_assignment, delete_managed_requirements_policy,
    list_managed_requirements_assignments, list_managed_requirements_policies,
    update_managed_requirements_assignment, update_managed_requirements_policy,
};
use self::plugin_management_capabilities::resolve_local_runtime_capabilities;
use self::plugin_management_plugins::{
    list_plugin_install_sources, proxy_plugin_release_artifact, update_plugin_preference,
};
use self::plugin_management_prompts::{get_agent_prompt_bundle, get_agent_prompt_bundle_manifest};
use self::project_bindings::{
    create_project_binding, delete_project_binding, list_project_bindings, update_project_binding,
};
pub use self::router::build_public_router;
use self::workspaces::{
    create_workspace, delete_workspace, list_workspaces, load_owned_workspace, update_workspace,
};

const MAX_USER_SERVICE_PROXY_BODY_BYTES: usize = 2 * 1024 * 1024;

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

include!("mod_part01.rs");
