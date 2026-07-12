// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use chatos_plugin_management_sdk::{
    LocalConnectorMcpListResponse, LocalConnectorMcpStatusBatchRequest,
    LocalConnectorMcpStatusItem, LocalConnectorMcpStatusRequest, LocalConnectorMcpSyncRequest,
    McpRecord, ResourceCheckRecord,
};
use serde::Deserialize;
use serde_json::Value;

use crate::models::CurrentUser;
use crate::state::AppState;

use super::{load_owned_device, ApiError};

#[derive(Debug, Deserialize)]
pub(super) struct LocalMcpListQuery {
    device_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalMcpDeleteQuery {
    device_id: String,
    manifest_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalMcpSyncRequest {
    device_id: String,
    manifest_id: String,
    runtime_kind: String,
    internal_name: String,
    display_name: String,
    description: Option<String>,
    enabled: Option<bool>,
    manifest_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct LocalMcpStatusRequest {
    device_id: String,
    manifest_id: String,
    status: String,
    last_error: Option<String>,
    #[serde(default)]
    tool_snapshot: Vec<Value>,
    manifest_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SocketMcpStatusMessage {
    #[serde(rename = "type")]
    _message_type: String,
    #[serde(default)]
    items: Vec<SocketMcpStatusItem>,
}

#[derive(Debug, Deserialize)]
struct SocketMcpStatusItem {
    plugin_mcp_id: String,
    manifest_id: String,
    status: String,
    last_error: Option<String>,
    #[serde(default)]
    tool_snapshot: Vec<Value>,
    manifest_hash: Option<String>,
}

pub(super) async fn list_local_mcps(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<LocalMcpListQuery>,
) -> Result<Json<LocalConnectorMcpListResponse>, ApiError> {
    require_human_user(&user)?;
    load_owned_device(&state, &user, query.device_id.as_str(), false).await?;
    state
        .plugin_management_client
        .list_local_connector_mcps(user.effective_owner_user_id(), query.device_id.as_str())
        .await
        .map(Json)
        .map_err(plugin_management_error)
}

pub(super) async fn create_local_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(request): Json<LocalMcpSyncRequest>,
) -> Result<(StatusCode, Json<McpRecord>), ApiError> {
    require_human_user(&user)?;
    load_owned_device(&state, &user, request.device_id.as_str(), true).await?;
    let request = plugin_sync_request(&user, request);
    let record = state
        .plugin_management_client
        .sync_local_connector_mcp(&request)
        .await
        .map_err(plugin_management_error)?;
    Ok((StatusCode::CREATED, Json(record)))
}

pub(super) async fn update_local_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
    Json(request): Json<LocalMcpSyncRequest>,
) -> Result<Json<McpRecord>, ApiError> {
    require_human_user(&user)?;
    load_owned_device(&state, &user, request.device_id.as_str(), true).await?;
    let request = plugin_sync_request(&user, request);
    state
        .plugin_management_client
        .update_local_connector_mcp(mcp_id.as_str(), &request)
        .await
        .map(Json)
        .map_err(plugin_management_error)
}

pub(super) async fn delete_local_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
    Query(query): Query<LocalMcpDeleteQuery>,
) -> Result<StatusCode, ApiError> {
    require_human_user(&user)?;
    load_owned_device(&state, &user, query.device_id.as_str(), false).await?;
    state
        .plugin_management_client
        .delete_local_connector_mcp(
            mcp_id.as_str(),
            user.effective_owner_user_id(),
            query.device_id.as_str(),
            query.manifest_id.as_str(),
        )
        .await
        .map_err(plugin_management_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn update_local_mcp_status(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
    Json(request): Json<LocalMcpStatusRequest>,
) -> Result<Json<ResourceCheckRecord>, ApiError> {
    require_human_user(&user)?;
    load_owned_device(&state, &user, request.device_id.as_str(), true).await?;
    let request = LocalConnectorMcpStatusRequest {
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id: request.device_id,
        manifest_id: request.manifest_id,
        status: request.status,
        last_error: request.last_error,
        tool_snapshot: request.tool_snapshot,
        manifest_hash: request.manifest_hash,
    };
    state
        .plugin_management_client
        .update_local_connector_mcp_status(mcp_id.as_str(), &request)
        .await
        .map(Json)
        .map_err(plugin_management_error)
}

pub(super) fn is_mcp_manifest_status_message(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .is_some_and(|value| value == "mcp_manifest_status")
}

pub(super) async fn sync_socket_mcp_statuses(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    text: &str,
) -> Result<usize, String> {
    let message = serde_json::from_str::<SocketMcpStatusMessage>(text)
        .map_err(|err| format!("decode MCP manifest status failed: {err}"))?;
    if message.items.len() > 200 {
        return Err("MCP manifest status exceeds 200 items".to_string());
    }
    let mut items = Vec::with_capacity(message.items.len());
    for item in message.items {
        items.push(LocalConnectorMcpStatusItem {
            mcp_id: item.plugin_mcp_id,
            status: LocalConnectorMcpStatusRequest {
                owner_user_id: owner_user_id.to_string(),
                device_id: device_id.to_string(),
                manifest_id: item.manifest_id,
                status: item.status,
                last_error: item.last_error,
                tool_snapshot: item.tool_snapshot,
                manifest_hash: item.manifest_hash,
            },
        });
    }
    let count = items.len();
    state
        .plugin_management_client
        .update_local_connector_mcp_status_batch(&LocalConnectorMcpStatusBatchRequest { items })
        .await
        .map_err(|err| err.to_string())?;
    Ok(count)
}

pub(super) async fn mark_device_mcps_offline(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
) -> Result<usize, String> {
    let records = state
        .plugin_management_client
        .list_local_connector_mcps(owner_user_id, device_id)
        .await
        .map_err(|err| err.to_string())?;
    let items = records
        .items
        .into_iter()
        .filter_map(|record| {
            let local = record.runtime.local_connector?;
            Some(LocalConnectorMcpStatusItem {
                mcp_id: record.id,
                status: LocalConnectorMcpStatusRequest {
                    owner_user_id: owner_user_id.to_string(),
                    device_id: device_id.to_string(),
                    manifest_id: local.manifest_id?,
                    status: "offline".to_string(),
                    last_error: Some("Local Connector device is offline".to_string()),
                    tool_snapshot: Vec::new(),
                    manifest_hash: None,
                },
            })
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        return Ok(0);
    }
    let count = items.len();
    state
        .plugin_management_client
        .update_local_connector_mcp_status_batch(&LocalConnectorMcpStatusBatchRequest { items })
        .await
        .map_err(|err| err.to_string())?;
    Ok(count)
}

fn plugin_sync_request(
    user: &CurrentUser,
    request: LocalMcpSyncRequest,
) -> LocalConnectorMcpSyncRequest {
    LocalConnectorMcpSyncRequest {
        owner_user_id: user.effective_owner_user_id().to_string(),
        device_id: request.device_id,
        manifest_id: request.manifest_id,
        runtime_kind: request.runtime_kind,
        internal_name: request.internal_name,
        display_name: request.display_name,
        description: request.description,
        enabled: request.enabled.unwrap_or(true),
        manifest_hash: request.manifest_hash,
    }
}

fn require_human_user(user: &CurrentUser) -> Result<(), ApiError> {
    if user.principal_type == "human_user" {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "Local Connector MCP configuration requires a human user",
        ))
    }
}

fn plugin_management_error(
    error: chatos_plugin_management_sdk::PluginManagementClientError,
) -> ApiError {
    match error {
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 400,
            message,
        } => ApiError::bad_request(message),
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 403,
            message,
        } => ApiError::forbidden(message),
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 404,
            message,
        } => ApiError::not_found(message),
        other => ApiError::service_unavailable(other.to_string()),
    }
}
