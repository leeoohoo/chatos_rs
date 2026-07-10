// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use serde_json::{json, Value};

use crate::api::types::LocalApiError;
use crate::mcp::configs::{
    delete_local_mcp_config, get_local_mcp_config, list_local_mcp_configs, save_local_mcp_config,
    set_local_mcp_enabled, sync_local_mcp_config, test_local_mcp_config,
};
use crate::mcp::manifest::{LocalMcpConfigDraft, LocalMcpManifestPublic};
use crate::LocalRuntime;

pub(crate) async fn local_mcp_configs(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Vec<LocalMcpManifestPublic>>, LocalApiError> {
    Ok(Json(list_local_mcp_configs(&runtime).await))
}

pub(crate) async fn local_get_mcp_config(
    State(runtime): State<LocalRuntime>,
    AxumPath(manifest_id): AxumPath<String>,
) -> Result<Json<LocalMcpManifestPublic>, LocalApiError> {
    get_local_mcp_config(&runtime, manifest_id.as_str())
        .await
        .map(Json)
        .map_err(|err| LocalApiError::bad_request(err.to_string()))
}

pub(crate) async fn local_save_mcp_config(
    State(runtime): State<LocalRuntime>,
    Json(draft): Json<LocalMcpConfigDraft>,
) -> Result<Json<LocalMcpManifestPublic>, LocalApiError> {
    save_local_mcp_config(&runtime, draft)
        .await
        .map(Json)
        .map_err(|err| LocalApiError::bad_request(err.to_string()))
}

pub(crate) async fn local_update_mcp_config(
    State(runtime): State<LocalRuntime>,
    AxumPath(manifest_id): AxumPath<String>,
    Json(mut draft): Json<LocalMcpConfigDraft>,
) -> Result<Json<LocalMcpManifestPublic>, LocalApiError> {
    draft.manifest_id = Some(manifest_id);
    local_save_mcp_config(State(runtime), Json(draft)).await
}

pub(crate) async fn local_test_mcp_config(
    State(runtime): State<LocalRuntime>,
    AxumPath(manifest_id): AxumPath<String>,
) -> Result<Json<LocalMcpManifestPublic>, LocalApiError> {
    test_local_mcp_config(&runtime, manifest_id.as_str())
        .await
        .map(Json)
        .map_err(|err| LocalApiError::bad_request(err.to_string()))
}

pub(crate) async fn local_enable_mcp_config(
    State(runtime): State<LocalRuntime>,
    AxumPath(manifest_id): AxumPath<String>,
) -> Result<Json<LocalMcpManifestPublic>, LocalApiError> {
    set_local_mcp_enabled(&runtime, manifest_id.as_str(), true)
        .await
        .map(Json)
        .map_err(|err| LocalApiError::bad_request(err.to_string()))
}

pub(crate) async fn local_disable_mcp_config(
    State(runtime): State<LocalRuntime>,
    AxumPath(manifest_id): AxumPath<String>,
) -> Result<Json<LocalMcpManifestPublic>, LocalApiError> {
    set_local_mcp_enabled(&runtime, manifest_id.as_str(), false)
        .await
        .map(Json)
        .map_err(|err| LocalApiError::bad_request(err.to_string()))
}

pub(crate) async fn local_sync_mcp_config(
    State(runtime): State<LocalRuntime>,
    AxumPath(manifest_id): AxumPath<String>,
) -> Result<Json<LocalMcpManifestPublic>, LocalApiError> {
    sync_local_mcp_config(&runtime, manifest_id.as_str())
        .await
        .map(Json)
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))
}

pub(crate) async fn local_delete_mcp_config(
    State(runtime): State<LocalRuntime>,
    AxumPath(manifest_id): AxumPath<String>,
) -> Result<Json<Value>, LocalApiError> {
    delete_local_mcp_config(&runtime, manifest_id.as_str())
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}
