// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::Json;
use serde_json::{json, Value};

use crate::chrome_bridge::{
    connect_chrome_native_host, disconnect_chrome_native_host, next_chrome_native_command,
    receive_chrome_native_event,
};
use crate::chrome_integration::{
    chrome_integration_status, disable_chrome_integration, enable_chrome_integration,
};

use super::super::types::{
    ChromeIntegrationEnableRequest, ChromeNativeConnectRequest, ChromeNativeConnectionRequest,
    ChromeNativeEventRequest, LocalApiError,
};

pub(crate) async fn local_chrome_integration_status() -> Json<Value> {
    Json(json!(chrome_integration_status()))
}

pub(crate) async fn local_enable_chrome_integration(
    Json(request): Json<ChromeIntegrationEnableRequest>,
) -> Result<Json<Value>, LocalApiError> {
    enable_chrome_integration(request.acknowledge_sensitive_browser_access)
        .map(|status| Json(json!(status)))
        .map_err(|error| LocalApiError::bad_request(error.to_string()))
}

pub(crate) async fn local_disable_chrome_integration() -> Result<Json<Value>, LocalApiError> {
    disable_chrome_integration()
        .map(|status| Json(json!(status)))
        .map_err(|error| LocalApiError::bad_request(error.to_string()))
}

pub(crate) async fn local_chrome_native_connect(
    Json(request): Json<ChromeNativeConnectRequest>,
) -> Result<Json<Value>, LocalApiError> {
    connect_chrome_native_host(
        request.connection_id.as_str(),
        request.origin.as_str(),
        request.protocol_version,
    )
    .map(|status| Json(json!({"success": true, "status": status})))
    .map_err(|error| LocalApiError::bad_request(error.to_string()))
}

pub(crate) async fn local_chrome_native_event(
    Json(request): Json<ChromeNativeEventRequest>,
) -> Result<Json<Value>, LocalApiError> {
    receive_chrome_native_event(request.connection_id.as_str(), request.event)
        .map(|status| Json(json!({"success": true, "status": status})))
        .map_err(|error| LocalApiError::bad_request(error.to_string()))
}

pub(crate) async fn local_chrome_native_next(
    Json(request): Json<ChromeNativeConnectionRequest>,
) -> Result<Json<Value>, LocalApiError> {
    next_chrome_native_command(request.connection_id.as_str())
        .await
        .map(|command| Json(json!({"success": true, "command": command})))
        .map_err(|error| LocalApiError::bad_request(error.to_string()))
}

pub(crate) async fn local_chrome_native_disconnect(
    Json(request): Json<ChromeNativeConnectionRequest>,
) -> Result<Json<Value>, LocalApiError> {
    disconnect_chrome_native_host(request.connection_id.as_str())
        .map(|disconnected| Json(json!({"success": true, "disconnected": disconnected})))
        .map_err(|error| LocalApiError::bad_request(error.to_string()))
}
