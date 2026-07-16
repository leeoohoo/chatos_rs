// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use serde_json::Value;

use crate::local_runtime::chat::prepare_local_chat_tools;
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;

#[derive(Debug, Serialize)]
pub(super) struct LocalAgentToolsResponse {
    tools: Vec<Value>,
    unavailable_tools: Vec<Value>,
    owner: &'static str,
    service: &'static str,
}

pub(super) async fn get_agent_tools(
    Path(session_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalAgentToolsResponse>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let session_id = required(session_id)?;
    let database = runtime.local_database()?;
    let session = database
        .get_session(session_id.as_str(), owner.owner_user_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_runtime_session_not_found",
                "Local runtime session was not found",
            )
        })?;
    let project = database
        .get_project(session.project_id.as_str(), owner.owner_user_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_runtime_project_not_found",
                "Local runtime project was not found",
            )
        })?;
    let settings = database
        .get_runtime_settings(owner.owner_user_id.as_str(), session_id.as_str())
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_runtime_settings_not_found",
                "Local runtime settings were not found",
            )
        })?;
    let request_id = format!("local_tool_catalog_{session_id}");
    let prepared = prepare_local_chat_tools(
        &runtime,
        owner.owner_user_id.as_str(),
        request_id.as_str(),
        &project,
        &settings,
    )
    .await
    .map_err(|error| LocalRuntimeApiError::conflict("local_runtime_tools_unavailable", error))?;
    Ok(Json(LocalAgentToolsResponse {
        tools: prepared.available_tools,
        unavailable_tools: prepared.unavailable_tools,
        owner: "local_runtime",
        service: "local_connector",
    }))
}

fn required(value: String) -> Result<String, LocalRuntimeApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_invalid_request",
            "session_id is required",
        ));
    }
    Ok(value)
}
