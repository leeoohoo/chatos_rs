// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::Json;
use serde_json::{json, Value};

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
) -> Result<Json<SystemConfigResponse>, ApiError> {
    Ok(Json(state.manager.system_config()))
}

pub async fn pool_status(
    State(state): State<AppState>,
) -> Result<Json<PoolStatusResponse>, ApiError> {
    Ok(Json(state.manager.pool_status()))
}

pub async fn list_sandbox_images(
    State(state): State<AppState>,
) -> Result<Json<SandboxImageCatalogResponse>, ApiError> {
    Ok(Json(state.manager.sandbox_images().await?))
}

pub async fn list_sandbox_image_jobs(
    State(state): State<AppState>,
) -> Result<Json<Vec<SandboxImageJobRecord>>, ApiError> {
    Ok(Json(state.manager.sandbox_image_jobs().await?))
}

pub async fn initialize_sandbox_image(
    State(state): State<AppState>,
    Json(input): Json<InitializeSandboxImageRequest>,
) -> Result<Json<SandboxImageJobRecord>, ApiError> {
    Ok(Json(state.manager.initialize_sandbox_image(input).await?))
}

pub async fn create_sandbox_lease(
    State(state): State<AppState>,
    Json(input): Json<CreateSandboxLeaseRequest>,
) -> Result<Json<CreateSandboxLeaseResponse>, ApiError> {
    Ok(Json(state.manager.create_lease(input).await?))
}

pub async fn list_sandboxes(
    State(state): State<AppState>,
    Query(query): Query<ListSandboxQuery>,
) -> Result<Json<Vec<SandboxLeaseRecord>>, ApiError> {
    Ok(Json(state.manager.list(query).await?))
}

pub async fn get_sandbox(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<SandboxLeaseRecord>, ApiError> {
    Ok(Json(state.manager.get(sandbox_id.as_str()).await?))
}

pub async fn heartbeat_sandbox(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, ApiError> {
    Ok(Json(
        state.manager.heartbeat(sandbox_id.as_str(), input).await?,
    ))
}

pub async fn health_sandbox(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<SandboxHealthResponse>, ApiError> {
    Ok(Json(state.manager.health(sandbox_id.as_str()).await?))
}

pub async fn sandbox_mcp_tools(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<SandboxMcpToolsResponse>, ApiError> {
    Ok(Json(state.manager.mcp_tools(sandbox_id.as_str()).await?))
}

pub async fn sandbox_mcp_call(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<SandboxMcpCallRequest>,
) -> Result<Json<SandboxMcpCallResponse>, ApiError> {
    Ok(Json(
        state.manager.mcp_call(sandbox_id.as_str(), input).await?,
    ))
}

pub async fn release_sandbox(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<ReleaseSandboxRequest>,
) -> Result<Json<ReleaseSandboxResponse>, ApiError> {
    Ok(Json(
        state.manager.release(sandbox_id.as_str(), input).await?,
    ))
}

pub async fn destroy_sandbox(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<DestroySandboxResponse>, ApiError> {
    Ok(Json(state.manager.destroy(sandbox_id.as_str()).await?))
}

pub async fn list_sandbox_events(
    Path(sandbox_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<SandboxEventRecord>>, ApiError> {
    Ok(Json(state.manager.events(sandbox_id.as_str()).await?))
}
