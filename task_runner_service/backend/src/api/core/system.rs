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
    let task_execution_max_iterations = state
        .task_service
        .effective_task_execution_max_iterations()
        .await
        .map_err(ApiError::bad_request)?;
    let tool_result_model_budget_limits = state
        .task_service
        .effective_tool_result_model_budget_limits()
        .await
        .map_err(ApiError::bad_request)?;
    let execution_environment_mode = state
        .task_service
        .effective_execution_environment_mode()
        .await
        .map_err(ApiError::bad_request)?;
    let sandbox_enabled = state
        .task_service
        .effective_sandbox_enabled()
        .await
        .map_err(ApiError::bad_request)?;
    let sandbox_manager_base_url = state
        .task_service
        .effective_sandbox_manager_base_url()
        .await
        .map_err(ApiError::bad_request)?;
    let sandbox_lease_ttl_seconds = state
        .task_service
        .effective_sandbox_lease_ttl_seconds()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(system_config(
        &state.config,
        execution_timeout_ms,
        task_execution_max_iterations,
        tool_result_model_budget_limits,
        execution_environment_mode,
        sandbox_enabled,
        sandbox_manager_base_url,
        sandbox_lease_ttl_seconds,
    )))
}

pub(in crate::api) async fn update_system_config_handler(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<UpdateRuntimeSettingsRequest>,
) -> Result<Json<SystemConfigResponse>, ApiError> {
    require_admin_user(&current_user)?;
    let settings = state
        .task_service
        .update_runtime_settings(input)
        .await
        .map_err(ApiError::bad_request)?;
    let execution_timeout_ms = settings
        .execution_timeout_ms
        .filter(|value| *value > 0)
        .unwrap_or(state.config.execution_timeout.as_millis() as u64);
    let execution_environment_mode = state
        .task_service
        .effective_execution_environment_mode()
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(system_config(
        &state.config,
        execution_timeout_ms,
        settings.task_execution_max_iterations,
        chatos_ai_runtime::ToolResultModelBudgetLimits::new(
            settings.tool_result_model_max_chars,
            settings.tool_results_model_total_max_chars,
        ),
        execution_environment_mode,
        settings.sandbox_enabled,
        settings.sandbox_manager_base_url,
        settings.sandbox_lease_ttl_seconds,
    )))
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
