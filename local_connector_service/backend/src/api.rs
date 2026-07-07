// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post, put};
use axum::{Extension, Json, Router};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;
use uuid::Uuid;

use crate::auth::{bearer_token_from_headers, verify_token_via_user_service};
use crate::models::{
    normalize_binding_mode, normalize_capabilities, normalize_optional_text,
    normalize_sandbox_mode, normalize_workspace_status, CurrentUser, ErrorResponse, HealthResponse,
    LocalConnectorDevice, LocalConnectorProjectBinding, LocalConnectorSandboxPairing,
    LocalConnectorSession, LocalConnectorWorkspace, DEVICE_STATUS_ONLINE, DEVICE_STATUS_REVOKED,
    WORKSPACE_STATUS_DISABLED,
};
use crate::relay::{RelayError, RelayRequest, RelayResponse};
use crate::state::AppState;

const DEFAULT_TERMINAL_EXEC_TIMEOUT_MS: u64 = 30_000;
const MAX_TERMINAL_EXEC_TIMEOUT_MS: u64 = 10 * 60 * 1000;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    pub fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
        }
    }

    pub fn gateway_timeout(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            message: message.into(),
        }
    }

    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_IMPLEMENTED,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}

#[derive(Debug, Deserialize)]
struct DeviceQuery {
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateDeviceRequest {
    display_name: Option<String>,
    public_key: Option<String>,
    client_version: Option<String>,
    os: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeviceHeartbeatRequest {
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceQuery {
    device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateWorkspaceRequest {
    device_id: Option<String>,
    display_name: Option<String>,
    local_path_alias: Option<String>,
    local_path_fingerprint: Option<String>,
    capabilities: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct UpdateWorkspaceRequest {
    device_id: Option<String>,
    display_name: Option<String>,
    local_path_alias: Option<String>,
    local_path_fingerprint: Option<String>,
    capabilities: Option<Vec<String>>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectBindingQuery {
    project_id: Option<String>,
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateProjectBindingRequest {
    project_id: Option<String>,
    device_id: Option<String>,
    workspace_id: Option<String>,
    mode: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateProjectBindingRequest {
    device_id: Option<String>,
    workspace_id: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SandboxPairingQuery {
    device_id: Option<String>,
    workspace_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct McpRelayQuery {
    workspace_id: Option<String>,
    cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TerminalExecRelayRequest {
    workspace_id: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TerminalSessionCreateRelayRequest {
    workspace_id: Option<String>,
    terminal_session_id: Option<String>,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct TerminalInputRelayRequest {
    workspace_id: Option<String>,
    terminal_session_id: Option<String>,
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TerminalWsRelayQuery {
    workspace_id: Option<String>,
    terminal_id: Option<String>,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct CreateSandboxPairingRequest {
    device_id: Option<String>,
    workspace_id: Option<String>,
    sandbox_mode: Option<String>,
    enabled: Option<bool>,
    access_client_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateSandboxPairingRequest {
    workspace_id: Option<String>,
    sandbox_mode: Option<String>,
    enabled: Option<bool>,
    access_client_id: Option<String>,
}

pub fn build_router(state: AppState) -> Router {
    let protected_api = Router::new()
        .route("/api/auth/me", get(current_user_handler))
        .route(
            "/api/local-connectors/devices",
            get(list_devices).post(create_device),
        )
        .route("/api/local-connectors/devices/{id}", get(get_device))
        .route(
            "/api/local-connectors/devices/{id}/heartbeat",
            post(heartbeat_device),
        )
        .route(
            "/api/local-connectors/devices/{id}/revoke",
            post(revoke_device),
        )
        .route(
            "/api/local-connectors/devices/{id}/disconnect",
            post(disconnect_device),
        )
        .route(
            "/api/local-connectors/devices/{id}/connect",
            get(connect_device),
        )
        .route(
            "/api/local-connectors/workspaces",
            get(list_workspaces).post(create_workspace),
        )
        .route(
            "/api/local-connectors/workspaces/{id}",
            put(update_workspace).delete(delete_workspace),
        )
        .route(
            "/api/local-connectors/project-bindings",
            get(list_project_bindings).post(create_project_binding),
        )
        .route(
            "/api/local-connectors/project-bindings/{id}",
            put(update_project_binding).delete(delete_project_binding),
        )
        .route(
            "/api/local-connectors/sandbox-pairings",
            get(list_sandbox_pairings).post(create_sandbox_pairing),
        )
        .route(
            "/api/local-connectors/sandbox-pairings/{id}",
            put(update_sandbox_pairing).delete(delete_sandbox_pairing),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/mcp",
            post(mcp_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/terminal/exec",
            post(terminal_exec_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/terminal/sessions",
            post(terminal_session_create_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/terminal/input",
            post(terminal_input_relay),
        )
        .route(
            "/api/local-connectors/relay/{device_id}/terminal/ws",
            get(terminal_ws_relay),
        )
        .route(
            "/api/local-connectors/sandbox-facade/{pairing_id}",
            any(sandbox_facade_root),
        )
        .route(
            "/api/local-connectors/sandbox-facade/{pairing_id}/{*path}",
            any(sandbox_facade_path),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/api/health", get(health_handler))
        .merge(protected_api)
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::DEBUG))
                .on_request(DefaultOnRequest::new().level(Level::DEBUG))
                .on_response(DefaultOnResponse::new().level(Level::DEBUG)),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
}

async fn require_auth(
    State(state): State<AppState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }
    if let Some(user) = internal_service_user_from_headers(&state, request.headers())? {
        request.extensions_mut().insert(user);
        return Ok(next.run(request).await);
    }
    let token = bearer_token_from_request(&request).map_err(ApiError::unauthorized)?;
    let user = verify_token_via_user_service(&state.config, token.as_str())
        .await
        .map_err(ApiError::unauthorized)?;
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

fn internal_service_user_from_headers(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Option<CurrentUser>, ApiError> {
    let Some(secret) = header_text(headers, "x-local-connector-internal-secret") else {
        return Ok(None);
    };
    let expected = state
        .config
        .internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::unauthorized("Local Connector internal auth is not configured"))?;
    if secret != expected {
        return Err(ApiError::unauthorized(
            "Local Connector internal auth secret is invalid",
        ));
    }
    let owner_user_id = header_text(headers, "x-local-connector-owner-user-id")
        .or_else(|| header_text(headers, "x-chatos-owner-user-id"))
        .ok_or_else(|| ApiError::unauthorized("Local Connector owner user id is required"))?;
    Ok(Some(CurrentUser {
        principal_type: "service".to_string(),
        user_id: format!("task_runner:{owner_user_id}"),
        username: Some("task_runner".to_string()),
        display_name: Some("Task Runner".to_string()),
        role: "service".to_string(),
        owner_user_id: Some(owner_user_id),
    }))
}

fn bearer_token_from_request(request: &Request<axum::body::Body>) -> Result<String, String> {
    bearer_token_from_headers(request.headers())
        .map(ToOwned::to_owned)
        .or_else(|_| {
            token_from_query(request.uri().query()).ok_or_else(|| "缺少登录令牌".to_string())
        })
}

fn token_from_query(query: Option<&str>) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next()?;
        let value = parts.next()?.trim();
        ((key == "access_token" || key == "token") && !value.is_empty()).then(|| value.to_string())
    })
}

fn header_text(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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

async fn list_devices(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<DeviceQuery>,
) -> Result<Json<Vec<LocalConnectorDevice>>, ApiError> {
    let owner_user_id = resolve_owner_user_id(query.user_id, &user)?;
    state
        .store
        .list_devices(owner_user_id.as_str())
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

async fn create_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(req): Json<CreateDeviceRequest>,
) -> Result<(StatusCode, Json<LocalConnectorDevice>), ApiError> {
    let device = LocalConnectorDevice::new(
        user.effective_owner_user_id().to_string(),
        required_text(req.display_name, "display_name")?,
        required_text(req.public_key, "public_key")?,
        normalize_optional_text(req.client_version),
        normalize_optional_text(req.os),
    );
    state
        .store
        .create_device(&device)
        .await
        .map_err(ApiError::internal)?;
    Ok((StatusCode::CREATED, Json(device)))
}

async fn get_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<LocalConnectorDevice>, ApiError> {
    load_owned_device(&state, &user, id.as_str(), false)
        .await
        .map(Json)
}

async fn heartbeat_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
    Json(req): Json<DeviceHeartbeatRequest>,
) -> Result<Json<LocalConnectorDevice>, ApiError> {
    let device = load_owned_device(&state, &user, id.as_str(), true).await?;
    if let Some(session_id) = normalize_optional_text(req.session_id) {
        state
            .store
            .heartbeat_session(session_id.as_str(), device.id.as_str())
            .await
            .map_err(ApiError::internal)?;
    } else {
        state
            .store
            .mark_device_online(device.id.as_str())
            .await
            .map_err(ApiError::internal)?;
    }
    load_owned_device(&state, &user, id.as_str(), true)
        .await
        .map(Json)
}

async fn revoke_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    load_owned_device(&state, &user, id.as_str(), false).await?;
    state
        .store
        .revoke_device(user.effective_owner_user_id(), id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(json!({ "success": true })))
}

async fn disconnect_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<LocalConnectorDevice>, ApiError> {
    let device = load_owned_device(&state, &user, id.as_str(), true).await?;
    state
        .store
        .mark_device_offline(device.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    load_owned_device(&state, &user, id.as_str(), true)
        .await
        .map(Json)
}

async fn connect_device(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let device = load_owned_device(&state, &user, id.as_str(), true).await?;
    let owner_user_id = user.effective_owner_user_id().to_string();
    Ok(ws
        .on_upgrade(move |socket| handle_connector_socket(state, owner_user_id, device.id, socket))
        .into_response())
}

async fn list_workspaces(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<WorkspaceQuery>,
) -> Result<Json<Vec<LocalConnectorWorkspace>>, ApiError> {
    state
        .store
        .list_workspaces(user.effective_owner_user_id(), query.device_id)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

async fn create_workspace(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<(StatusCode, Json<LocalConnectorWorkspace>), ApiError> {
    let device_id = required_text(req.device_id, "device_id")?;
    load_owned_device(&state, &user, device_id.as_str(), true).await?;
    let workspace = LocalConnectorWorkspace::new(
        user.effective_owner_user_id().to_string(),
        device_id,
        required_text(req.display_name, "display_name")?,
        required_text(req.local_path_alias, "local_path_alias")?,
        required_text(req.local_path_fingerprint, "local_path_fingerprint")?,
        normalize_capabilities(req.capabilities.unwrap_or_else(default_capabilities)),
    );
    state
        .store
        .create_workspace(&workspace)
        .await
        .map_err(ApiError::internal)?;
    Ok((StatusCode::CREATED, Json(workspace)))
}

async fn update_workspace(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<Json<LocalConnectorWorkspace>, ApiError> {
    let mut workspace = load_owned_workspace(&state, &user, id.as_str()).await?;
    if let Some(device_id) = normalize_optional_text(req.device_id) {
        load_owned_device(&state, &user, device_id.as_str(), true).await?;
        workspace.device_id = device_id;
    }
    if let Some(display_name) = normalize_optional_text(req.display_name) {
        workspace.display_name = display_name;
    }
    if let Some(alias) = normalize_optional_text(req.local_path_alias) {
        workspace.local_path_alias = alias;
    }
    if let Some(fingerprint) = normalize_optional_text(req.local_path_fingerprint) {
        workspace.local_path_fingerprint = fingerprint;
    }
    if let Some(capabilities) = req.capabilities {
        workspace.capabilities = normalize_capabilities(capabilities);
    }
    if let Some(status) = normalize_optional_text(req.status) {
        workspace.status = normalize_workspace_status(Some(status));
    }
    state
        .store
        .update_workspace(&workspace)
        .await
        .map_err(ApiError::internal)?;
    load_owned_workspace(&state, &user, id.as_str())
        .await
        .map(Json)
}

async fn delete_workspace(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    load_owned_workspace(&state, &user, id.as_str()).await?;
    state
        .store
        .delete_workspace(user.effective_owner_user_id(), id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(json!({ "success": true })))
}

async fn list_project_bindings(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ProjectBindingQuery>,
) -> Result<Json<Vec<LocalConnectorProjectBinding>>, ApiError> {
    let mode = normalize_optional_text(query.mode).map(|value| normalize_binding_mode(Some(value)));
    state
        .store
        .list_project_bindings(
            user.effective_owner_user_id(),
            normalize_optional_text(query.project_id),
            mode,
        )
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

async fn create_project_binding(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(req): Json<CreateProjectBindingRequest>,
) -> Result<(StatusCode, Json<LocalConnectorProjectBinding>), ApiError> {
    let device_id = required_text(req.device_id, "device_id")?;
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let binding = LocalConnectorProjectBinding::new(
        user.effective_owner_user_id().to_string(),
        required_text(req.project_id, "project_id")?,
        device_id,
        workspace_id,
        normalize_binding_mode(req.mode),
        req.enabled.unwrap_or(true),
    );
    let saved = state
        .store
        .upsert_project_binding(&binding)
        .await
        .map_err(ApiError::internal)?;
    Ok((StatusCode::CREATED, Json(saved)))
}

async fn update_project_binding(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectBindingRequest>,
) -> Result<Json<LocalConnectorProjectBinding>, ApiError> {
    let mut binding = load_owned_project_binding(&state, &user, id.as_str()).await?;
    if let Some(device_id) = normalize_optional_text(req.device_id) {
        binding.device_id = device_id;
    }
    if let Some(workspace_id) = normalize_optional_text(req.workspace_id) {
        binding.workspace_id = workspace_id;
    }
    validate_device_workspace(
        &state,
        &user,
        binding.device_id.as_str(),
        binding.workspace_id.as_str(),
    )
    .await?;
    if let Some(enabled) = req.enabled {
        binding.enabled = enabled;
    }
    state
        .store
        .update_project_binding(&binding)
        .await
        .map_err(ApiError::internal)?;
    load_owned_project_binding(&state, &user, id.as_str())
        .await
        .map(Json)
}

async fn delete_project_binding(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    load_owned_project_binding(&state, &user, id.as_str()).await?;
    state
        .store
        .delete_project_binding(user.effective_owner_user_id(), id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(json!({ "success": true })))
}

async fn list_sandbox_pairings(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<SandboxPairingQuery>,
) -> Result<Json<Vec<LocalConnectorSandboxPairing>>, ApiError> {
    state
        .store
        .list_sandbox_pairings(
            user.effective_owner_user_id(),
            normalize_optional_text(query.device_id),
            normalize_optional_text(query.workspace_id),
        )
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

async fn create_sandbox_pairing(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(req): Json<CreateSandboxPairingRequest>,
) -> Result<(StatusCode, Json<LocalConnectorSandboxPairing>), ApiError> {
    let device_id = required_text(req.device_id, "device_id")?;
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let mut pairing = LocalConnectorSandboxPairing::new(
        user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        req.enabled.unwrap_or(false),
        normalize_sandbox_mode(req.sandbox_mode),
        None,
        normalize_optional_text(req.access_client_id),
    );
    pairing.facade_base_url = Some(state.config.sandbox_facade_base_url(pairing.id.as_str()));
    let saved = state
        .store
        .upsert_sandbox_pairing(&pairing)
        .await
        .map_err(ApiError::internal)?;
    Ok((StatusCode::CREATED, Json(saved)))
}

async fn update_sandbox_pairing(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSandboxPairingRequest>,
) -> Result<Json<LocalConnectorSandboxPairing>, ApiError> {
    let mut pairing = load_owned_sandbox_pairing(&state, &user, id.as_str()).await?;
    if let Some(workspace_id) = normalize_optional_text(req.workspace_id) {
        pairing.workspace_id = workspace_id;
    }
    validate_device_workspace(
        &state,
        &user,
        pairing.device_id.as_str(),
        pairing.workspace_id.as_str(),
    )
    .await?;
    if let Some(mode) = normalize_optional_text(req.sandbox_mode) {
        pairing.sandbox_mode = normalize_sandbox_mode(Some(mode));
    }
    if let Some(enabled) = req.enabled {
        pairing.enabled = enabled;
    }
    if let Some(access_client_id) = normalize_optional_text(req.access_client_id) {
        pairing.access_client_id = Some(access_client_id);
    }
    if pairing.facade_base_url.is_none() {
        pairing.facade_base_url = Some(state.config.sandbox_facade_base_url(pairing.id.as_str()));
    }
    state
        .store
        .update_sandbox_pairing(&pairing)
        .await
        .map_err(ApiError::internal)?;
    load_owned_sandbox_pairing(&state, &user, id.as_str())
        .await
        .map(Json)
}

async fn delete_sandbox_pairing(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    load_owned_sandbox_pairing(&state, &user, id.as_str()).await?;
    state
        .store
        .delete_sandbox_pairing(user.effective_owner_user_id(), id.as_str())
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(json!({ "success": true })))
}

async fn mcp_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<McpRelayQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(query.workspace_id, "workspace_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let mut relay_headers = relay_headers(&headers);
    if let Some(cwd) = normalize_optional_text(query.cwd) {
        relay_headers.insert("x-local-connector-cwd".to_string(), cwd);
    }
    let request = RelayRequest {
        message_type: "mcp".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/mcp".to_string(),
        headers: relay_headers,
        body: relay_body(body.as_ref()),
    };
    let response = state
        .relay
        .dispatch(request, state.config.relay_request_timeout)
        .await
        .map_err(relay_error_to_api_error)?;
    Ok(relay_response_to_http(response))
}

async fn terminal_exec_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(req): Json<TerminalExecRelayRequest>,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    let command = required_text(req.command, "command")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;

    let timeout_ms = req
        .timeout_ms
        .unwrap_or(DEFAULT_TERMINAL_EXEC_TIMEOUT_MS)
        .clamp(1_000, MAX_TERMINAL_EXEC_TIMEOUT_MS);
    let relay_timeout = state
        .config
        .relay_request_timeout
        .max(Duration::from_millis(timeout_ms.saturating_add(5_000)));

    let request = RelayRequest {
        message_type: "terminal_exec_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/terminal/exec".to_string(),
        headers: BTreeMap::new(),
        body: json!({
            "command": command,
            "args": req.args.unwrap_or_default(),
            "cwd": normalize_optional_text(req.cwd),
            "timeout_ms": timeout_ms,
            "source": normalize_optional_text(req.source),
        }),
    };

    let response = state
        .relay
        .dispatch(request, relay_timeout)
        .await
        .map_err(relay_error_to_api_error)?;
    Ok(relay_response_to_http(response))
}

async fn terminal_session_create_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(req): Json<TerminalSessionCreateRelayRequest>,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    let terminal_session_id = required_text(req.terminal_session_id, "terminal_session_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;

    let request = RelayRequest {
        message_type: "terminal_session_create_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/terminal/sessions".to_string(),
        headers: BTreeMap::new(),
        body: json!({
            "terminal_session_id": terminal_session_id,
            "cwd": normalize_optional_text(req.cwd),
            "cols": req.cols.unwrap_or(80).max(1),
            "rows": req.rows.unwrap_or(24).max(1),
        }),
    };

    let response = state
        .relay
        .dispatch(request, state.config.relay_request_timeout)
        .await
        .map_err(relay_error_to_api_error)?;
    Ok(relay_response_to_http(response))
}

async fn terminal_input_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Json(req): Json<TerminalInputRelayRequest>,
) -> Result<Json<Value>, ApiError> {
    let workspace_id = required_text(req.workspace_id, "workspace_id")?;
    let terminal_session_id = required_text(req.terminal_session_id, "terminal_session_id")?;
    let data = req.data.unwrap_or_default();
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;

    let request = RelayRequest {
        message_type: "terminal_input".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id,
        workspace_id,
        method: "POST".to_string(),
        path: "/terminal/terminal_input".to_string(),
        headers: BTreeMap::new(),
        body: json!({
            "terminal_session_id": terminal_session_id,
            "data": data,
        }),
    };

    state
        .relay
        .send(request)
        .await
        .map_err(relay_error_to_api_error)?;
    Ok(Json(json!({ "success": true })))
}

async fn terminal_ws_relay(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(device_id): Path<String>,
    Query(query): Query<TerminalWsRelayQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let workspace_id = required_text(query.workspace_id, "workspace_id")?;
    validate_device_workspace(&state, &user, device_id.as_str(), workspace_id.as_str()).await?;
    let terminal_session_id =
        normalize_optional_text(query.terminal_id).unwrap_or_else(|| Uuid::new_v4().to_string());
    let cwd = normalize_optional_text(query.cwd);
    let cols = query.cols.unwrap_or(80).max(1);
    let rows = query.rows.unwrap_or(24).max(1);
    let owner_user_id = user.effective_owner_user_id().to_string();
    Ok(ws
        .on_upgrade(move |socket| {
            handle_terminal_relay_socket(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                terminal_session_id,
                cwd,
                cols,
                rows,
                socket,
            )
        })
        .into_response())
}

async fn handle_terminal_relay_socket(
    state: AppState,
    owner_user_id: String,
    device_id: String,
    workspace_id: String,
    terminal_session_id: String,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    mut socket: WebSocket,
) {
    let mut events = state
        .relay
        .subscribe_terminal_session(terminal_session_id.as_str())
        .await;
    let create_request = RelayRequest {
        message_type: "terminal_session_create_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: owner_user_id.clone(),
        device_id: device_id.clone(),
        workspace_id: workspace_id.clone(),
        method: "POST".to_string(),
        path: "/terminal/sessions".to_string(),
        headers: BTreeMap::new(),
        body: json!({
            "terminal_session_id": terminal_session_id.as_str(),
            "cwd": cwd,
            "cols": cols,
            "rows": rows,
        }),
    };
    let create_response = state
        .relay
        .dispatch(create_request, state.config.relay_request_timeout)
        .await;
    match create_response {
        Ok(response) if (200..300).contains(&response.status) => {
            let snapshot = response
                .body
                .get("snapshot")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !snapshot.is_empty()
                && socket
                    .send(Message::Text(
                        json!({"type": "snapshot", "data": snapshot})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .is_err()
            {
                state
                    .relay
                    .drop_terminal_session(terminal_session_id.as_str())
                    .await;
                return;
            }
            let busy = response
                .body
                .get("busy")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if socket
                .send(Message::Text(
                    json!({"type": "state", "busy": busy, "snapshot_paging": true})
                        .to_string()
                        .into(),
                ))
                .await
                .is_err()
            {
                state
                    .relay
                    .drop_terminal_session(terminal_session_id.as_str())
                    .await;
                return;
            }
        }
        Ok(response) => {
            let message = response
                .body
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Local Connector terminal startup failed");
            let _ = socket
                .send(Message::Text(
                    json!({"type": "error", "error": message})
                        .to_string()
                        .into(),
                ))
                .await;
            state
                .relay
                .drop_terminal_session(terminal_session_id.as_str())
                .await;
            return;
        }
        Err(err) => {
            let _ = socket
                .send(Message::Text(
                    json!({"type": "error", "error": err.message()})
                        .to_string()
                        .into(),
                ))
                .await;
            state
                .relay
                .drop_terminal_session(terminal_session_id.as_str())
                .await;
            return;
        }
    }

    let (mut sender, mut receiver) = socket.split();
    let event_task = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    let payload =
                        terminal_event_to_ws_payload(event.message_type.as_str(), &event.body);
                    let Some(payload) = payload else {
                        continue;
                    };
                    if sender
                        .send(Message::Text(payload.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                    if event.message_type == "terminal_exit" {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(message) = receiver.next().await {
        match message {
            Ok(Message::Text(text)) => {
                if !handle_terminal_ws_input(
                    &state,
                    owner_user_id.as_str(),
                    device_id.as_str(),
                    workspace_id.as_str(),
                    terminal_session_id.as_str(),
                    text.as_str(),
                )
                .await
                {
                    break;
                }
            }
            Ok(Message::Binary(bytes)) => {
                let data = String::from_utf8_lossy(&bytes).to_string();
                if !data.is_empty()
                    && !send_terminal_control(
                        &state,
                        owner_user_id.as_str(),
                        device_id.as_str(),
                        workspace_id.as_str(),
                        "terminal_input",
                        terminal_session_id.as_str(),
                        json!({ "data": data }),
                    )
                    .await
                {
                    break;
                }
            }
            Ok(Message::Ping(_)) => {}
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }

    let _ = send_terminal_control(
        &state,
        owner_user_id.as_str(),
        device_id.as_str(),
        workspace_id.as_str(),
        "terminal_close",
        terminal_session_id.as_str(),
        json!({}),
    )
    .await;
    state
        .relay
        .drop_terminal_session(terminal_session_id.as_str())
        .await;
    event_task.abort();
}

async fn handle_terminal_ws_input(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    workspace_id: &str,
    terminal_session_id: &str,
    text: &str,
) -> bool {
    let parsed = serde_json::from_str::<Value>(text);
    let Ok(value) = parsed else {
        return send_terminal_control(
            state,
            owner_user_id,
            device_id,
            workspace_id,
            "terminal_input",
            terminal_session_id,
            json!({ "data": text }),
        )
        .await;
    };
    let message_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match message_type {
        "input" => {
            let data = value
                .get("data")
                .and_then(Value::as_str)
                .unwrap_or_default();
            send_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                "terminal_input",
                terminal_session_id,
                json!({ "data": data }),
            )
            .await
        }
        "resize" => {
            let cols = value.get("cols").and_then(Value::as_u64).unwrap_or(80);
            let rows = value.get("rows").and_then(Value::as_u64).unwrap_or(24);
            send_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                "terminal_resize",
                terminal_session_id,
                json!({ "cols": cols, "rows": rows }),
            )
            .await
        }
        "snapshot" => {
            let lines = value.get("lines").and_then(Value::as_u64).unwrap_or(500);
            send_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                "terminal_snapshot_request",
                terminal_session_id,
                json!({ "lines": lines }),
            )
            .await
        }
        "ping" | "command" => true,
        _ => true,
    }
}

async fn send_terminal_control(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    workspace_id: &str,
    message_type: &str,
    terminal_session_id: &str,
    mut body: Value,
) -> bool {
    if let Value::Object(ref mut map) = body {
        map.insert(
            "terminal_session_id".to_string(),
            Value::String(terminal_session_id.to_string()),
        );
    }
    let request = RelayRequest {
        message_type: message_type.to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: owner_user_id.to_string(),
        device_id: device_id.to_string(),
        workspace_id: workspace_id.to_string(),
        method: "POST".to_string(),
        path: format!("/terminal/{message_type}"),
        headers: BTreeMap::new(),
        body,
    };
    state.relay.send(request).await.is_ok()
}

fn terminal_event_to_ws_payload(message_type: &str, body: &Value) -> Option<Value> {
    match message_type {
        "terminal_output" => Some(json!({
            "type": "output",
            "data": body.get("data").and_then(Value::as_str).unwrap_or_default(),
        })),
        "terminal_snapshot" => Some(json!({
            "type": "snapshot",
            "data": body.get("data").and_then(Value::as_str).unwrap_or_default(),
        })),
        "terminal_exit" => Some(json!({
            "type": "exit",
            "code": body.get("code").and_then(Value::as_i64).unwrap_or(0),
        })),
        "terminal_state" => Some(json!({
            "type": "state",
            "busy": body.get("busy").and_then(Value::as_bool).unwrap_or(false),
            "snapshot_paging": true,
        })),
        "terminal_error" => Some(json!({
            "type": "error",
            "error": body.get("error").and_then(Value::as_str).unwrap_or("Local Connector terminal error"),
        })),
        _ => None,
    }
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

    let request = RelayRequest {
        message_type: "sandbox_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id: pairing.device_id.clone(),
        workspace_id: pairing.workspace_id.clone(),
        method: method.as_str().to_string(),
        path: normalize_relay_path(path.as_str()),
        headers: relay_headers(&headers),
        body: relay_body(body.as_ref()),
    };

    let response = state
        .relay
        .dispatch(request, state.config.relay_request_timeout)
        .await
        .map_err(relay_error_to_api_error)?;
    Ok(relay_response_to_http(response))
}

async fn handle_connector_socket(
    state: AppState,
    owner_user_id: String,
    device_id: String,
    socket: WebSocket,
) {
    let session = LocalConnectorSession::new(owner_user_id, device_id.clone());
    if let Err(err) = state.store.open_session(&session).await {
        send_startup_error(socket, err).await;
        return;
    }
    let _ = state.store.mark_device_online(device_id.as_str()).await;

    let (mut sender, mut receiver) = socket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<String>(256);
    state
        .relay
        .register_session(
            device_id.clone(),
            session.owner_user_id.clone(),
            session.id.clone(),
            outbound_tx.clone(),
        )
        .await;

    let send_task = tokio::spawn(async move {
        while let Some(text) = outbound_rx.recv().await {
            if sender.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let _ = send_outbound_json(
        &outbound_tx,
        json!({
            "type": "connected",
            "device_id": device_id.as_str(),
            "session_id": session.id.as_str(),
            "connection_id": session.connection_id.as_str(),
            "timestamp": crate::models::now_rfc3339(),
        }),
    )
    .await;

    while let Some(message) = receiver.next().await {
        match message {
            Ok(Message::Text(text)) => {
                if is_heartbeat_message(text.as_str()) {
                    let _ = state
                        .store
                        .heartbeat_session(session.id.as_str(), device_id.as_str())
                        .await;
                    if !send_outbound_json(
                        &outbound_tx,
                        json!({
                            "type": "pong",
                            "session_id": session.id.as_str(),
                            "timestamp": crate::models::now_rfc3339(),
                        }),
                    )
                    .await
                    {
                        break;
                    }
                } else if let Ok(consumed) = state.relay.handle_inbound_text(text.as_str()).await {
                    if !consumed {
                        let _ = send_outbound_json(
                            &outbound_tx,
                            json!({
                                "type": "ack",
                                "message": "control channel message accepted",
                                "timestamp": crate::models::now_rfc3339(),
                            }),
                        )
                        .await;
                    }
                } else {
                    let _ = send_outbound_json(
                        &outbound_tx,
                        json!({
                            "type": "error",
                            "code": "invalid_relay_response",
                            "message": "invalid relay response message",
                            "timestamp": crate::models::now_rfc3339(),
                        }),
                    )
                    .await;
                }
            }
            Ok(Message::Ping(bytes)) => {
                let payload_len = bytes.len();
                let _ = state
                    .store
                    .heartbeat_session(session.id.as_str(), device_id.as_str())
                    .await;
                let _ = send_outbound_json(
                    &outbound_tx,
                    json!({
                        "type": "pong",
                        "payload_bytes": payload_len,
                        "session_id": session.id.as_str(),
                        "timestamp": crate::models::now_rfc3339(),
                    }),
                )
                .await;
            }
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }

    let _ = state
        .store
        .close_session(session.id.as_str(), device_id.as_str())
        .await;
    state
        .relay
        .unregister_session(device_id.as_str(), session.id.as_str())
        .await;
    send_task.abort();
}

async fn load_owned_device(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    reject_revoked: bool,
) -> Result<LocalConnectorDevice, ApiError> {
    let device = state
        .store
        .get_device(id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Local Connector device not found"))?;
    if device.owner_user_id != user.effective_owner_user_id() {
        return Err(ApiError::forbidden(
            "Local Connector device does not belong to current user",
        ));
    }
    if reject_revoked && device.status == DEVICE_STATUS_REVOKED {
        return Err(ApiError::bad_request(
            "Local Connector device has been revoked",
        ));
    }
    Ok(device)
}

async fn load_owned_workspace(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<LocalConnectorWorkspace, ApiError> {
    let workspace = state
        .store
        .get_workspace(id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Local Connector workspace not found"))?;
    if workspace.owner_user_id != user.effective_owner_user_id() {
        return Err(ApiError::forbidden(
            "Local Connector workspace does not belong to current user",
        ));
    }
    Ok(workspace)
}

async fn load_owned_project_binding(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<LocalConnectorProjectBinding, ApiError> {
    let binding = state
        .store
        .get_project_binding(id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Local Connector project binding not found"))?;
    if binding.owner_user_id != user.effective_owner_user_id() {
        return Err(ApiError::forbidden(
            "Local Connector project binding does not belong to current user",
        ));
    }
    Ok(binding)
}

async fn load_owned_sandbox_pairing(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<LocalConnectorSandboxPairing, ApiError> {
    let pairing = state
        .store
        .get_sandbox_pairing(id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Local Connector sandbox pairing not found"))?;
    if pairing.owner_user_id != user.effective_owner_user_id() {
        return Err(ApiError::forbidden(
            "Local Connector sandbox pairing does not belong to current user",
        ));
    }
    Ok(pairing)
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

fn resolve_owner_user_id(
    requested_user_id: Option<String>,
    user: &CurrentUser,
) -> Result<String, ApiError> {
    let owner_user_id = user.effective_owner_user_id();
    if let Some(requested) = normalize_optional_text(requested_user_id) {
        if requested != owner_user_id {
            return Err(ApiError::forbidden("user_id 与登录用户不一致"));
        }
    }
    Ok(owner_user_id.to_string())
}

async fn send_socket_json(socket: &mut WebSocket, value: Value) -> bool {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .is_ok()
}

async fn send_startup_error(mut socket: WebSocket, message: String) {
    let _ = send_socket_json(
        &mut socket,
        json!({
            "type": "error",
            "code": "session_open_failed",
            "message": message,
        }),
    )
    .await;
}

async fn send_outbound_json(sender: &mpsc::Sender<String>, value: Value) -> bool {
    sender.send(value.to_string()).await.is_ok()
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

fn is_heartbeat_message(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("ping") || trimmed.eq_ignore_ascii_case("heartbeat") {
        return true;
    }
    serde_json::from_str::<Value>(trimmed)
        .ok()
        .and_then(|value| {
            value.get("type").and_then(Value::as_str).map(|item| {
                item.eq_ignore_ascii_case("ping") || item.eq_ignore_ascii_case("heartbeat")
            })
        })
        .unwrap_or(false)
}

fn required_text(value: Option<String>, field: &str) -> Result<String, ApiError> {
    normalize_optional_text(value)
        .ok_or_else(|| ApiError::bad_request(format!("{field} is required and cannot be empty")))
}

fn default_capabilities() -> Vec<String> {
    vec![
        "mcp".to_string(),
        "terminal".to_string(),
        "sandbox".to_string(),
    ]
}
