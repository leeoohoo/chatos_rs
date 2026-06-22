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
    Ok(Json(system_config(
        &state.config,
        execution_timeout_ms,
        task_execution_max_iterations,
        tool_result_model_budget_limits,
    )))
}

pub(in crate::api) async fn update_system_config_handler(
    State(state): State<AppState>,
    Json(input): Json<UpdateRuntimeSettingsRequest>,
) -> Result<Json<SystemConfigResponse>, ApiError> {
    let settings = state
        .task_service
        .update_runtime_settings(input)
        .await
        .map_err(ApiError::bad_request)?;
    let execution_timeout_ms = settings
        .execution_timeout_ms
        .filter(|value| *value > 0)
        .unwrap_or(state.config.execution_timeout.as_millis() as u64);
    Ok(Json(system_config(
        &state.config,
        execution_timeout_ms,
        settings.task_execution_max_iterations,
        chatos_ai_runtime::ToolResultModelBudgetLimits::new(
            settings.tool_result_model_max_chars,
            settings.tool_results_model_total_max_chars,
        ),
    )))
}

#[derive(Debug, Deserialize)]
pub(in crate::api) struct TaskRunnerLocaleQuery {
    lang: Option<String>,
}

#[derive(Debug, Serialize)]
pub(in crate::api) struct TaskRunnerSkillResponse {
    name: &'static str,
    locale: &'static str,
    content: &'static str,
}

pub(in crate::api) async fn task_runner_skill_handler(
    Query(query): Query<TaskRunnerLocaleQuery>,
) -> Json<TaskRunnerSkillResponse> {
    let locale = requested_task_runner_locale(query.lang.as_deref());
    Json(if locale.is_english() {
        TaskRunnerSkillResponse {
            name: "task-runner-ai-agent-en-us",
            locale: "en-US",
            content: TASK_RUNNER_SKILL_EN_US,
        }
    } else {
        TaskRunnerSkillResponse {
            name: "task-runner-ai-agent-zh-cn",
            locale: "zh-CN",
            content: TASK_RUNNER_SKILL_ZH_CN,
        }
    })
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
