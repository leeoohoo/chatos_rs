// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;

use crate::api::types::{LocalApiError, UpdateLocalRuntimeSettingsRequest};
use crate::LocalRuntime;

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
    let mut state = runtime.state.write().await;
    if let Some(max_iterations) = req.ai_agent_max_iterations {
        if max_iterations == 0 {
            return Err(LocalApiError::bad_request(
                "ai_agent_max_iterations must be greater than 0",
            ));
        }
        state.runtime_settings.ai_agent_max_iterations = max_iterations;
    }
    state.runtime_settings = state.runtime_settings.clone().normalized();
    state.save(runtime.state_path.as_path())?;
    Ok(Json(state.runtime_settings.clone()))
}
