// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use crate::auth::SandboxAuthContext;
use crate::error::ApiError;
use crate::models::{
    CreateSandboxLeaseRequest, CreateSandboxLeaseResponse, DestroySandboxResponse,
    HeartbeatRequest, HeartbeatResponse, InitializeSandboxImageRequest, ListSandboxQuery,
    PoolStatusResponse, ReleaseSandboxRequest, ReleaseSandboxResponse, SandboxEventRecord,
    SandboxHealthResponse, SandboxImageCatalogResponse, SandboxImageJobRecord, SandboxLeaseRecord,
    SandboxMcpCallRequest, SandboxMcpCallResponse, SandboxMcpToolsResponse, SystemConfigResponse,
};
use crate::state::AppState;

pub async fn health() -> Json<Value> {
    Json(json!({
        "ok": true,
        "service": "sandbox_manager_service",
    }))
}

pub async fn system_config(
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
) -> Result<Json<SystemConfigResponse>, ApiError> {
    Ok(Json(state.manager.system_config(&auth)?))
}

pub async fn pool_status(
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
) -> Result<Json<PoolStatusResponse>, ApiError> {
    Ok(Json(state.manager.pool_status(&auth).await?))
}

pub async fn list_sandbox_images(
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
) -> Result<Json<SandboxImageCatalogResponse>, ApiError> {
    Ok(Json(state.manager.sandbox_images(&auth).await?))
}

pub async fn list_sandbox_image_jobs(
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
) -> Result<Json<Vec<SandboxImageJobRecord>>, ApiError> {
    Ok(Json(state.manager.sandbox_image_jobs(&auth).await?))
}

pub async fn initialize_sandbox_image(
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
    Json(input): Json<InitializeSandboxImageRequest>,
) -> Result<Json<SandboxImageJobRecord>, ApiError> {
    Ok(Json(
        state.manager.initialize_sandbox_image(&auth, input).await?,
    ))
}

pub async fn create_sandbox_lease(
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
    headers: HeaderMap,
    Json(input): Json<CreateSandboxLeaseRequest>,
) -> Result<Json<CreateSandboxLeaseResponse>, ApiError> {
    Ok(Json(
        state
            .manager
            .create_lease(&auth, input, header_text(&headers, "x-idempotency-key"))
            .await?,
    ))
}

pub async fn list_sandboxes(
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
    Query(query): Query<ListSandboxQuery>,
) -> Result<Json<Vec<SandboxLeaseRecord>>, ApiError> {
    Ok(Json(state.manager.list(&auth, query).await?))
}

pub async fn get_sandbox(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
) -> Result<Json<SandboxLeaseRecord>, ApiError> {
    Ok(Json(state.manager.get(&auth, sandbox_id.as_str()).await?))
}

pub async fn heartbeat_sandbox(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
    Json(input): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, ApiError> {
    Ok(Json(
        state
            .manager
            .heartbeat(&auth, sandbox_id.as_str(), input)
            .await?,
    ))
}

pub async fn health_sandbox(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
) -> Result<Json<SandboxHealthResponse>, ApiError> {
    Ok(Json(
        state.manager.health(&auth, sandbox_id.as_str()).await?,
    ))
}

pub async fn sandbox_mcp_tools(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
) -> Result<Json<SandboxMcpToolsResponse>, ApiError> {
    Ok(Json(
        state.manager.mcp_tools(&auth, sandbox_id.as_str()).await?,
    ))
}

pub async fn sandbox_mcp_call(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
    Json(input): Json<SandboxMcpCallRequest>,
) -> Result<Json<SandboxMcpCallResponse>, ApiError> {
    Ok(Json(
        state
            .manager
            .mcp_call(&auth, sandbox_id.as_str(), input)
            .await?,
    ))
}

pub async fn sandbox_mcp_proxy(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
    Json(input): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        state
            .manager
            .mcp_proxy(&auth, sandbox_id.as_str(), input)
            .await?,
    ))
}

pub async fn release_sandbox(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
    Json(input): Json<ReleaseSandboxRequest>,
) -> Result<Json<ReleaseSandboxResponse>, ApiError> {
    Ok(Json(
        state
            .manager
            .release(&auth, sandbox_id.as_str(), input)
            .await?,
    ))
}

pub async fn destroy_sandbox(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
) -> Result<Json<DestroySandboxResponse>, ApiError> {
    Ok(Json(
        state.manager.destroy(&auth, sandbox_id.as_str()).await?,
    ))
}

pub async fn list_sandbox_events(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
    Extension(auth): Extension<SandboxAuthContext>,
) -> Result<Json<Vec<SandboxEventRecord>>, ApiError> {
    Ok(Json(
        state.manager.events(&auth, sandbox_id.as_str()).await?,
    ))
}

fn header_text(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
