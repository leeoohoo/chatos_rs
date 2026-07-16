// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::Json;

use crate::api::types::LocalApiError;
use crate::local_runtime::{
    agent_prompt_status, check_agent_prompt_updates, update_agent_prompt_bundle,
    LocalAgentPromptStatus,
};
use crate::LocalRuntime;

pub(crate) async fn local_agent_prompt_status(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalAgentPromptStatus>, LocalApiError> {
    agent_prompt_status(&runtime)
        .await
        .map(Json)
        .map_err(LocalApiError::from)
}

pub(crate) async fn local_check_agent_prompt_updates(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalAgentPromptStatus>, LocalApiError> {
    check_agent_prompt_updates(&runtime)
        .await
        .map(Json)
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))
}

pub(crate) async fn local_update_agent_prompt_bundle(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalAgentPromptStatus>, LocalApiError> {
    update_agent_prompt_bundle(&runtime)
        .await
        .map(Json)
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))
}
