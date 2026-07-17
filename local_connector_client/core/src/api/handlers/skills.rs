// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::api::types::LocalApiError;
use crate::local_runtime::sync_local_capability_snapshots;
use crate::skills::{fetch_user_skill_catalog, sync_skill_inventory, update_user_skill_preference};
use crate::{tracing_stdout, LocalRuntime};

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateLocalSkillPreferenceRequest {
    enabled: bool,
}

pub(crate) async fn local_skills(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    fetch_user_skill_catalog(&runtime)
        .await
        .map(Json)
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))
}

pub(crate) async fn local_update_skill_preference(
    State(runtime): State<LocalRuntime>,
    AxumPath(skill_id): AxumPath<String>,
    Json(request): Json<UpdateLocalSkillPreferenceRequest>,
) -> Result<Json<Value>, LocalApiError> {
    let response = update_user_skill_preference(&runtime, skill_id.as_str(), request.enabled)
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    refresh_capabilities(&runtime).await;
    Ok(Json(response))
}

pub(crate) async fn local_sync_skill_inventory(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    let response = sync_skill_inventory(&runtime)
        .await
        .map_err(|err| LocalApiError::bad_gateway(err.to_string()))?;
    refresh_capabilities(&runtime).await;
    Ok(Json(response))
}

async fn refresh_capabilities(runtime: &LocalRuntime) {
    if let Err(error) = sync_local_capability_snapshots(runtime).await {
        tracing_stdout(
            format!("keep cached capability snapshots after Skill update: {error}").as_str(),
        );
    }
}
