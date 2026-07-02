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
    profile: Option<String>,
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
    let is_plan_profile = query
        .profile
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("chatos_plan"));
    Json(match (locale.is_english(), is_plan_profile) {
        (true, true) => TaskRunnerSkillResponse {
            name: "task-runner-plan-task-en-us",
            locale: "en-US",
            content: TASK_RUNNER_PLAN_SKILL_EN_US,
        },
        (false, true) => TaskRunnerSkillResponse {
            name: "task-runner-plan-task-zh-cn",
            locale: "zh-CN",
            content: TASK_RUNNER_PLAN_SKILL_ZH_CN,
        },
        (true, false) => TaskRunnerSkillResponse {
            name: "task-runner-ai-agent-en-us",
            locale: "en-US",
            content: TASK_RUNNER_SKILL_EN_US,
        },
        (false, false) => TaskRunnerSkillResponse {
            name: "task-runner-ai-agent-zh-cn",
            locale: "zh-CN",
            content: TASK_RUNNER_SKILL_ZH_CN,
        },
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

#[cfg(test)]
mod tests {
    use axum::extract::Query;

    use super::*;

    #[tokio::test]
    async fn default_skill_zh_cn_includes_ai_role_sequence_diagram() {
        let Json(response) = task_runner_skill_handler(Query(TaskRunnerLocaleQuery {
            lang: Some("zh-CN".to_string()),
            profile: None,
        }))
        .await;

        assert_eq!(response.name, "task-runner-ai-agent-zh-cn");
        assert!(response.content.contains("你（AI 主对话）"));
        assert!(response.content.contains("Task Runner 任务队列"));
        assert!(response.content.contains("Worker 定时执行器"));
        assert!(response.content.contains("返回任务已安排"));
        assert!(response.content.contains("回调事实结果"));
        assert!(!response.content.contains("目标系统 / 项目管理"));
        assert!(!response.content.contains("participant Facts"));
    }

    #[tokio::test]
    async fn default_skill_en_us_includes_ai_role_sequence_diagram() {
        let Json(response) = task_runner_skill_handler(Query(TaskRunnerLocaleQuery {
            lang: Some("en-US".to_string()),
            profile: None,
        }))
        .await;

        assert_eq!(response.name, "task-runner-ai-agent-en-us");
        assert!(response.content.contains("You (AI main chat)"));
        assert!(response.content.contains("Task Runner task queue"));
        assert!(response.content.contains("Scheduled worker"));
        assert!(response.content.contains("Return arranged task"));
        assert!(response.content.contains("Callback factual result"));
        assert!(!response
            .content
            .contains("Target system / Project Management"));
        assert!(!response.content.contains("participant Facts"));
    }

    #[tokio::test]
    async fn plan_skill_zh_cn_requires_task_creation_for_direct_pm_requests() {
        let Json(response) = task_runner_skill_handler(Query(TaskRunnerLocaleQuery {
            lang: Some("zh-CN".to_string()),
            profile: Some("chatos_plan".to_string()),
        }))
        .await;

        assert_eq!(response.name, "task-runner-plan-task-zh-cn");
        assert!(response.content.contains("视为已经授权创建或调整"));
        assert!(response.content.contains("Task Runner 规划任务"));
        assert!(response.content.contains("主对话不具备项目探索"));
        assert!(response.content.contains("已经成为事实"));
        assert!(response.content.contains("participant You as \"你"));
        assert!(!response.content.contains("participant Facts"));
        assert!(response.content.contains("不要再请求确认"));
        assert!(response.content.contains("task_runner_service_create_task"));
        assert!(response.content.contains("Phase/Epic 层"));
        assert!(response.content.contains("只有粗粒度阶段任务"));
    }

    #[tokio::test]
    async fn plan_skill_en_us_requires_task_creation_for_direct_pm_requests() {
        let Json(response) = task_runner_skill_handler(Query(TaskRunnerLocaleQuery {
            lang: Some("en-US".to_string()),
            profile: Some("chatos_plan".to_string()),
        }))
        .await;

        assert_eq!(response.name, "task-runner-plan-task-en-us");
        assert!(response
            .content
            .contains("project-related execution requests"));
        assert!(response
            .content
            .contains("does not have project-exploration"));
        assert!(response.content.contains("already-established facts"));
        assert!(response.content.contains("participant You as \"You"));
        assert!(!response.content.contains("participant Facts"));
        assert!(response
            .content
            .contains("do not ask for confirmation again"));
        assert!(response.content.contains("task_runner_service_create_task"));
        assert!(response.content.contains("Phase/Epic level"));
        assert!(response
            .content
            .contains("only coarse phase-level work items"));
    }
}
