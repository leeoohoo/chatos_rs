// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;

use crate::api::types::{LocalApiError, UpdateLocalRuntimeSettingsRequest};
use crate::config::ClientConfig;
use crate::registration::disconnect_device;
use crate::{tracing_stdout, LocalRuntime};

pub(crate) async fn local_runtime_settings(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<crate::state::LocalRuntimeSettings>, LocalApiError> {
    let state = runtime.state.read().await;
    Ok(Json(state.runtime_settings.clone().normalized()))
}

pub(crate) async fn local_update_runtime_settings(
    State(runtime): State<LocalRuntime>,
    Json(req): Json<UpdateLocalRuntimeSettingsRequest>,
) -> Result<Json<crate::state::LocalRuntimeSettings>, LocalApiError> {
    if req.ai_agent_max_iterations == Some(0) {
        return Err(LocalApiError::bad_request(
            "ai_agent_max_iterations must be greater than 0",
        ));
    }
    let mode_changed = {
        let state = runtime.state.read().await;
        req.developer_mode
            .is_some_and(|developer_mode| developer_mode != state.runtime_settings.developer_mode)
    };
    if mode_changed {
        let disconnect = {
            let state = runtime.state.read().await;
            ClientConfig::from_state(&state, runtime.state_path.clone())
                .zip(state.device_id.clone())
        };
        {
            let mut task = runtime.connector_task.lock().await;
            if let Some(handle) = task.take() {
                handle.abort();
            }
        }
        if let Some((config, device_id)) = disconnect {
            if let Err(err) =
                disconnect_device(&runtime.http_client, &config, device_id.as_str()).await
            {
                tracing_stdout(
                    format!("disconnect previous developer-mode endpoint failed: {err}").as_str(),
                );
            }
        }
    }
    let mut state = runtime.state.write().await;
    if let Some(max_iterations) = req.ai_agent_max_iterations {
        state.runtime_settings.ai_agent_max_iterations = max_iterations;
    }
    if let Some(developer_mode) = req.developer_mode {
        state.runtime_settings.developer_mode = developer_mode;
    }
    state.runtime_settings = state.runtime_settings.clone().normalized();
    state.save(runtime.state_path.as_path())?;
    Ok(Json(state.runtime_settings.clone()))
}
