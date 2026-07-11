// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path as AxumPath, State};
use axum::Json;

use crate::api::types::LocalApiError;
use crate::system_permissions::{
    open_system_permission_settings, system_permissions_response, SystemPermissionsResponse,
};
use crate::LocalRuntime;

pub(crate) async fn local_system_permissions(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<SystemPermissionsResponse>, LocalApiError> {
    let state = runtime.state.read().await;
    Ok(Json(system_permissions_response(&state).await))
}

pub(crate) async fn local_request_system_permission(
    State(runtime): State<LocalRuntime>,
    AxumPath(permission_id): AxumPath<String>,
) -> Result<Json<SystemPermissionsResponse>, LocalApiError> {
    open_system_permission_settings(permission_id.as_str())
        .await
        .map_err(|err| LocalApiError::bad_request(err.to_string()))?;
    let state = runtime.state.read().await;
    Ok(Json(system_permissions_response(&state).await))
}
