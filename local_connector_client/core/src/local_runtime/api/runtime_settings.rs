// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::local_runtime::storage::{LocalRuntimeSettingsRecord, SaveLocalRuntimeSettingsInput};
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;

#[path = "runtime_settings_policy.rs"]
mod policy;

use policy::{clamp_memory_character_threshold, clamp_memory_message_threshold};

#[derive(Debug, Serialize)]
pub(super) struct LocalRuntimeSettingsResponse {
    session_id: String,
    user_id: String,
    selected_model_id: Option<String>,
    selected_model_name: Option<String>,
    selected_thinking_level: Option<String>,
    remote_connection_id: Option<String>,
    workspace_root: Option<String>,
    reasoning_enabled: bool,
    plan_mode_enabled: bool,
    mcp_enabled: bool,
    enabled_mcp_ids: Vec<String>,
    selected_skill_ids: Vec<String>,
    auto_create_task: bool,
    memory_auto_summary_enabled: bool,
    memory_summary_message_threshold: i64,
    memory_summary_character_threshold: i64,
    memory_recall_limit: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct UpdateLocalRuntimeSettingsRequest {
    selected_model_id: Option<String>,
    selected_model_name: Option<String>,
    selected_thinking_level: Option<String>,
    workspace_root: Option<String>,
    reasoning_enabled: Option<bool>,
    plan_mode_enabled: Option<bool>,
    mcp_enabled: Option<bool>,
    enabled_mcp_ids: Option<Vec<String>>,
    selected_skill_ids: Option<Vec<String>>,
    auto_create_task: Option<bool>,
    memory_auto_summary_enabled: Option<bool>,
    memory_summary_message_threshold: Option<i64>,
    memory_summary_character_threshold: Option<i64>,
    memory_recall_limit: Option<i64>,
}

pub(super) async fn get_runtime_settings(
    Path(session_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<LocalRuntimeSettingsResponse>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let settings =
        load_settings(&runtime, owner.owner_user_id.as_str(), session_id.as_str()).await?;
    Ok(Json(settings_response(owner.owner_user_id, settings)))
}

pub(super) async fn update_runtime_settings(
    Path(session_id): Path<String>,
    State(runtime): State<LocalRuntime>,
    Json(request): Json<UpdateLocalRuntimeSettingsRequest>,
) -> Result<Json<LocalRuntimeSettingsResponse>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let current =
        load_settings(&runtime, owner.owner_user_id.as_str(), session_id.as_str()).await?;
    let saved = runtime
        .local_database()?
        .save_runtime_settings(
            owner.owner_user_id.as_str(),
            SaveLocalRuntimeSettingsInput {
                session_id: current.session_id.clone(),
                selected_model_id: normalize_optional(request.selected_model_id)
                    .or(current.selected_model_id),
                selected_model_name: normalize_optional(request.selected_model_name)
                    .or(current.selected_model_name),
                selected_thinking_level: normalize_optional(request.selected_thinking_level)
                    .or(current.selected_thinking_level),
                workspace_root: normalize_optional(request.workspace_root)
                    .or(current.workspace_root),
                reasoning_enabled: request
                    .reasoning_enabled
                    .unwrap_or(current.reasoning_enabled),
                plan_mode_enabled: request
                    .plan_mode_enabled
                    .unwrap_or(current.plan_mode_enabled),
                mcp_enabled: request.mcp_enabled.unwrap_or(current.mcp_enabled),
                enabled_mcp_ids_json: serde_json::to_string(
                    &request
                        .enabled_mcp_ids
                        .unwrap_or_else(|| parse_list(&current.enabled_mcp_ids_json)),
                )
                .map_err(|error| LocalRuntimeApiError::internal(error.to_string()))?,
                selected_skill_ids_json: serde_json::to_string(
                    &request
                        .selected_skill_ids
                        .unwrap_or_else(|| parse_list(&current.selected_skill_ids_json)),
                )
                .map_err(|error| LocalRuntimeApiError::internal(error.to_string()))?,
                auto_create_task: request.auto_create_task.unwrap_or(current.auto_create_task),
                memory_auto_summary_enabled: request
                    .memory_auto_summary_enabled
                    .unwrap_or(current.memory_auto_summary_enabled),
                memory_summary_message_threshold: request
                    .memory_summary_message_threshold
                    .map(clamp_memory_message_threshold)
                    .unwrap_or(current.memory_summary_message_threshold),
                memory_summary_character_threshold: request
                    .memory_summary_character_threshold
                    .map(clamp_memory_character_threshold)
                    .unwrap_or(current.memory_summary_character_threshold),
                memory_recall_limit: request
                    .memory_recall_limit
                    .map(policy::clamp_memory_recall_limit)
                    .unwrap_or(current.memory_recall_limit),
            },
        )
        .await?;
    Ok(Json(settings_response(owner.owner_user_id, saved)))
}

async fn load_settings(
    runtime: &LocalRuntime,
    owner_user_id: &str,
    session_id: &str,
) -> Result<LocalRuntimeSettingsRecord, LocalRuntimeApiError> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_invalid_request",
            "session_id is required",
        ));
    }
    runtime
        .local_database()?
        .get_runtime_settings(owner_user_id, session_id)
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found(
                "local_runtime_settings_not_found",
                "Local runtime settings were not found",
            )
        })
}

fn settings_response(
    owner_user_id: String,
    record: LocalRuntimeSettingsRecord,
) -> LocalRuntimeSettingsResponse {
    LocalRuntimeSettingsResponse {
        session_id: record.session_id,
        user_id: owner_user_id,
        selected_model_id: record.selected_model_id,
        selected_model_name: record.selected_model_name,
        selected_thinking_level: record.selected_thinking_level,
        remote_connection_id: None,
        workspace_root: record.workspace_root,
        reasoning_enabled: record.reasoning_enabled,
        plan_mode_enabled: record.plan_mode_enabled,
        mcp_enabled: record.mcp_enabled,
        enabled_mcp_ids: parse_list(record.enabled_mcp_ids_json.as_str()),
        selected_skill_ids: parse_list(record.selected_skill_ids_json.as_str()),
        auto_create_task: record.auto_create_task,
        memory_auto_summary_enabled: record.memory_auto_summary_enabled,
        memory_summary_message_threshold: record.memory_summary_message_threshold,
        memory_summary_character_threshold: record.memory_summary_character_threshold,
        memory_recall_limit: record.memory_recall_limit,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn parse_list(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
