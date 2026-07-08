// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::sandbox::docker::docker_status_struct;
use crate::{LocalRuntime, LOCAL_SANDBOX_BACKEND};

use super::super::types::LocalApiError;

pub(crate) async fn local_status(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    Ok(Json(status_payload(&runtime).await))
}

pub(crate) async fn status_payload(runtime: &LocalRuntime) -> Value {
    let state = runtime.state.read().await.clone();
    let connector_running = runtime
        .connector_task
        .lock()
        .await
        .as_ref()
        .map(|handle| !handle.is_finished())
        .unwrap_or(false);
    json!({
        "configured": state.auth.is_some(),
        "connector_running": connector_running,
        "cloud_base_url": state.auth.as_ref().map(|auth| auth.cloud_base_url.as_str()),
        "user_service_base_url": state.auth.as_ref().map(|auth| auth.user_service_base_url.as_str()),
        "device_id": state.device_id,
        "device_name": state.auth.as_ref().map(|auth| auth.device_name.as_str()),
        "user": state.auth.as_ref().and_then(|auth| auth.user.clone()),
        "workspaces": state.workspaces,
        "sandbox": {
            "enabled": state.sandbox.enabled,
            "backend": LOCAL_SANDBOX_BACKEND,
            "isolation": "local_docker",
            "selected_image_ref": state.sandbox.selected_image_ref,
        },
        "docker": docker_status_struct().await,
    })
}
