// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::task_runner_internal_prompt_preview;
use chatos_mcp_runtime::BuiltinMcpPromptLocale;

pub(in crate::api) async fn health_handler() -> Json<HealthResponse> {
    Json(health())
}

pub(in crate::api) async fn system_config_handler(
    State(state): State<AppState>,
) -> Result<Json<SystemConfigResponse>, ApiError> {
    let execution_timeout_ms = state
        .task_service
        .effective_execution_timeout_ms()
        .await
        .map_err(ApiError::bad_request)?;
    let task_runner_runtime_settings = state
        .task_service
        .effective_task_runner_runtime_settings()
        .await
        .map_err(ApiError::bad_request)?;
    let tool_result_model_budget_limits = state
        .task_service
        .effective_tool_result_model_budget_limits()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(system_config(
        &state.config,
        &state.task_queue_topology,
        execution_timeout_ms,
        task_runner_runtime_settings,
        tool_result_model_budget_limits,
    )))
}

pub(in crate::api) async fn update_system_config_handler(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<UpdateRuntimeSettingsRequest>,
) -> Result<Json<SystemConfigResponse>, ApiError> {
    require_admin_user(&current_user)?;
    state
        .task_service
        .update_runtime_settings(input)
        .await
        .map_err(ApiError::bad_request)?;
    system_config_handler(State(state)).await
}

#[derive(Debug, Deserialize)]
pub(in crate::api) struct TaskRunnerLocaleQuery {
    lang: Option<String>,
}

pub(in crate::api) async fn task_runner_internal_prompt_preview_handler(
    Query(query): Query<TaskRunnerLocaleQuery>,
) -> Json<TaskRunnerInternalPromptPreviewResponse> {
    Json(task_runner_internal_prompt_preview(
        requested_task_runner_locale(query.lang.as_deref()),
    ))
}

fn requested_task_runner_locale(lang: Option<&str>) -> BuiltinMcpPromptLocale {
    match lang
        .map(str::trim)
        .unwrap_or(BuiltinMcpPromptLocale::DEFAULT_KEY)
        .to_ascii_lowercase()
        .as_str()
    {
        "en" | "en-us" | "english" => BuiltinMcpPromptLocale::EnUs,
        _ => BuiltinMcpPromptLocale::ZhCn,
    }
}
