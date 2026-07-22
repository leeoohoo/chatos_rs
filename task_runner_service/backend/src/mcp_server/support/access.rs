// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{ExternalMcpConfigRecord, ModelConfigRecord, TaskRecord, TaskStatus};

use super::super::chatos_async_planner::planner_agent_tool_allowed;
use super::super::{McpRequestContext, McpToolProfile};

pub(crate) fn agent_tool_allowed(name: &str) -> bool {
    matches!(
        name,
        "list_tasks"
            | "get_task"
            | "get_task_stats"
            | "create_task"
            | "list_mcp_builtin_catalog"
            | "list_external_mcp_configs"
            | "list_available_skills"
            | "create_tasks_with_prerequisites"
            | "update_task"
            | "set_task_prerequisites"
            | "cancel_task"
            | "wait_for_task_completion"
            | "get_task_dependency_graph"
            | "delete_task"
            | "batch_delete_tasks"
            | "list_runs"
            | "get_run"
            | "start_task_run"
            | "batch_start_task_runs"
            | "get_task_memory_context"
            | "list_task_memory_records"
            | "summarize_task_memory"
            | "cancel_run"
            | "list_run_events"
            | "list_prompts"
            | "get_prompt"
            | "submit_prompt"
            | "cancel_prompt"
    )
}

pub(crate) fn external_mcp_configs_for_user(
    configs: Vec<ExternalMcpConfigRecord>,
    current_user: &CurrentUser,
) -> Vec<Value> {
    configs
        .into_iter()
        .filter(|config| config.enabled)
        .filter(|config| external_mcp_config_visible_to_user(config, current_user))
        .map(external_mcp_config_for_external_mcp)
        .collect()
}

fn external_mcp_config_visible_to_user(
    config: &ExternalMcpConfigRecord,
    current_user: &CurrentUser,
) -> bool {
    let owner = config
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(config.creator_user_id.as_deref());
    current_user.can_access_owned_resource(owner)
}

fn external_mcp_config_for_external_mcp(config: ExternalMcpConfigRecord) -> Value {
    let endpoint = if config.transport == "http" {
        config.url.clone().unwrap_or_default()
    } else {
        std::iter::once(config.command.clone().unwrap_or_default())
            .chain(config.args.clone())
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    json!({
        "id": config.id,
        "name": config.name,
        "transport": config.transport,
        "enabled": config.enabled,
        "endpoint": endpoint,
    })
}

pub(crate) fn agent_tool_allowed_for_profile(name: &str, tool_profile: McpToolProfile) -> bool {
    match tool_profile {
        McpToolProfile::Default => agent_tool_allowed(name),
        McpToolProfile::ChatosAsyncPlanner => planner_agent_tool_allowed(name),
        McpToolProfile::ProjectRequirementExecutionPlanner => matches!(
            name,
            "list_tasks"
                | "get_task"
                | "get_task_dependency_graph"
                | "list_mcp_builtin_catalog"
                | "list_external_mcp_configs"
                | "list_available_skills"
                | "create_project_execution_tasks"
                | "cancel_task"
        ),
    }
}

pub(crate) fn reusable_chatos_async_task(task: &TaskRecord) -> bool {
    matches!(
        task.status,
        TaskStatus::Ready | TaskStatus::Queued | TaskStatus::Running
    )
}

pub(crate) fn ensure_task_startable_from_mcp(
    task: &TaskRecord,
    request_context: &McpRequestContext,
) -> Result<(), String> {
    if !matches!(task.status, TaskStatus::Draft | TaskStatus::Ready) {
        return Err(historical_task_read_only_message());
    }
    if task
        .last_run_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return Err(historical_task_read_only_message());
    }
    if request_has_concrete_source(request_context)
        && !task_matches_request_source(task, request_context)
    {
        return Err(historical_task_read_only_message());
    }
    Ok(())
}

pub(crate) fn ensure_task_status_update_allowed_from_mcp(
    current_user: &CurrentUser,
) -> Result<(), String> {
    if current_user.is_admin() {
        return Ok(());
    }
    Err(
        "Chatos task tools cannot update task execution status directly. Create a new task for new work, or use cancel_task for obsolete tasks."
            .to_string(),
    )
}

fn request_has_concrete_source(request_context: &McpRequestContext) -> bool {
    non_empty(request_context.source_session_id.as_deref()).is_some()
        && (non_empty(request_context.source_user_message_id.as_deref()).is_some()
            || non_empty(request_context.source_turn_id.as_deref()).is_some())
}

fn task_matches_request_source(task: &TaskRecord, request_context: &McpRequestContext) -> bool {
    let Some(session_id) = non_empty(request_context.source_session_id.as_deref()) else {
        return false;
    };
    if non_empty(task.source_session_id.as_deref()) != Some(session_id) {
        return false;
    }
    if let Some(message_id) = non_empty(request_context.source_user_message_id.as_deref()) {
        return non_empty(task.source_user_message_id.as_deref()) == Some(message_id);
    }
    if let Some(turn_id) = non_empty(request_context.source_turn_id.as_deref()) {
        return non_empty(task.source_turn_id.as_deref()) == Some(turn_id);
    }
    false
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn historical_task_read_only_message() -> String {
    "Historical Task Runner tasks are read-only through Chatos task tools. Create a new task for current work, or use cancel_task to stop obsolete work.".to_string()
}

pub(crate) fn effective_owner_user_id(current_user: &CurrentUser) -> Result<&str, String> {
    current_user
        .effective_owner_user_id()
        .ok_or_else(|| "当前登录态缺少用户归属信息".to_string())
}

pub(crate) fn task_creator_filter(current_user: &CurrentUser) -> Result<Option<String>, String> {
    if current_user.is_admin() {
        return Ok(None);
    }
    Ok(Some(effective_owner_user_id(current_user)?.to_string()))
}

pub(crate) fn ensure_task_owner(
    task: &TaskRecord,
    current_user: &CurrentUser,
) -> Result<(), String> {
    if current_user.is_admin() {
        return Ok(());
    }
    let owner_user_id = task
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(task.creator_user_id.as_deref());
    if owner_user_id == Some(effective_owner_user_id(current_user)?) {
        return Ok(());
    }
    Err("当前 agent 无权访问该任务".to_string())
}

pub(crate) fn require_admin_tool(current_user: &CurrentUser) -> Result<(), String> {
    if current_user.is_admin() {
        Ok(())
    } else {
        Err("当前 agent 无权调用管理员工具".to_string())
    }
}

pub(crate) fn tasks_for_external_mcp(tasks: Vec<TaskRecord>) -> Value {
    Value::Array(tasks.into_iter().map(task_for_external_mcp).collect())
}

pub(crate) fn task_for_external_mcp(task: TaskRecord) -> Value {
    let mut value = json!(task);
    remove_internal_task_fields(&mut value);
    value
}

pub(crate) fn remove_internal_task_fields(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                remove_internal_task_fields(item);
            }
        }
        Value::Object(object) => {
            object.remove("process_log");
            object.remove("project_id");
            for item in object.values_mut() {
                remove_internal_task_fields(item);
            }
        }
        _ => {}
    }
}

pub(crate) fn model_configs_for_user(
    models: Vec<ModelConfigRecord>,
    current_user: &CurrentUser,
) -> Vec<Value> {
    enabled_model_configs_for_user(models, current_user)
        .into_iter()
        .map(|model| model_config_for_user(model, current_user))
        .collect()
}

pub(crate) fn model_config_for_user(
    model: ModelConfigRecord,
    _current_user: &CurrentUser,
) -> Value {
    let mut value = json!(model);
    if let Some(object) = value.as_object_mut() {
        object.insert("api_key".to_string(), Value::String(String::new()));
    }
    value
}

pub(crate) fn filter_model_configs_for_user(
    models: Vec<ModelConfigRecord>,
    current_user: &CurrentUser,
) -> Vec<ModelConfigRecord> {
    models
        .into_iter()
        .filter(|model| model_visible_to_user(model, current_user))
        .collect()
}

pub(crate) fn enabled_model_configs_for_user(
    models: Vec<ModelConfigRecord>,
    current_user: &CurrentUser,
) -> Vec<ModelConfigRecord> {
    models
        .into_iter()
        .filter(|model| model_visible_to_user(model, current_user))
        .filter(|model| model.enabled)
        .filter(model_has_cloud_runtime_credentials)
        .collect()
}

pub(crate) fn model_has_cloud_runtime_credentials(model: &ModelConfigRecord) -> bool {
    !model.api_key.trim().is_empty() && !model.base_url.trim().is_empty()
}

pub(crate) fn model_visible_to_user(model: &ModelConfigRecord, current_user: &CurrentUser) -> bool {
    let Some(expected_owner_user_id) = current_user.effective_owner_user_id() else {
        return false;
    };
    model
        .owner_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        == Some(expected_owner_user_id)
}

pub(crate) fn select_model_config_id_for_task(
    models: &[ModelConfigRecord],
    title: &str,
    objective: &str,
    description: Option<&str>,
    tags: &[String],
) -> Option<String> {
    let haystack = task_model_selection_text(title, objective, description, tags);
    let image_task = task_requests_image_generation(haystack.as_str());
    models
        .iter()
        .enumerate()
        .max_by_key(|(index, model)| {
            let image_model = model_is_image_specialized(model);
            (
                model_task_compatibility_score(image_task, image_model),
                model_task_match_score(model, haystack.as_str()),
                model_default_usage_score(model),
                (!model_looks_like_test_config(model)) as usize,
                std::cmp::Reverse(*index),
            )
        })
        .map(|(_, model)| model)
        .map(|model| model.id.clone())
}

fn model_task_compatibility_score(image_task: bool, image_model: bool) -> usize {
    match (image_task, image_model) {
        (true, true) => 2,
        (true, false) | (false, false) => 1,
        (false, true) => 0,
    }
}

fn task_requests_image_generation(haystack: &str) -> bool {
    [
        "image", "images", "dall-e", "图片", "图像", "生图", "绘图", "插画", "海报", "照片",
    ]
    .iter()
    .any(|keyword| haystack.contains(keyword))
}

fn model_is_image_specialized(model: &ModelConfigRecord) -> bool {
    let identity = format!("{} {}", model.name, model.model).to_ascii_lowercase();
    ["image", "dall-e", "imagen"]
        .iter()
        .any(|keyword| identity.contains(keyword))
}

fn model_default_usage_score(model: &ModelConfigRecord) -> usize {
    let usage = model
        .usage_scenario
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    ["常规", "通用", "general", "default"]
        .iter()
        .any(|keyword| usage.contains(keyword)) as usize
}

fn model_looks_like_test_config(model: &ModelConfigRecord) -> bool {
    let name = model.name.trim().to_ascii_lowercase();
    name == "test"
        || name.starts_with("test /")
        || name.starts_with("test-")
        || name.starts_with("test_")
}

fn task_model_selection_text(
    title: &str,
    objective: &str,
    description: Option<&str>,
    tags: &[String],
) -> String {
    let mut parts = vec![title, objective];
    if let Some(description) = description {
        parts.push(description);
    }
    let mut text = parts.join(" ").to_ascii_lowercase();
    for tag in tags {
        text.push(' ');
        text.push_str(tag.as_str());
    }
    text.to_ascii_lowercase()
}

fn model_task_match_score(model: &ModelConfigRecord, haystack: &str) -> usize {
    let usage_score = text_match_score(model.usage_scenario.as_deref(), haystack, 5);
    let name_score = text_match_score(Some(model.name.as_str()), haystack, 2);
    let model_score = text_match_score(Some(model.model.as_str()), haystack, 1);
    let usage_bonus = model
        .usage_scenario
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty()) as usize;
    usage_score + name_score + model_score + usage_bonus
}

fn text_match_score(value: Option<&str>, haystack: &str, weight: usize) -> usize {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return 0;
    };
    value
        .split(|ch: char| !ch.is_alphanumeric())
        .map(str::trim)
        .filter(|token| token.chars().count() >= 2)
        .filter(|token| !model_selection_stop_word(token))
        .filter(|token| haystack.contains(token.to_ascii_lowercase().as_str()))
        .count()
        * weight
}

fn model_selection_stop_word(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "api" | "auto" | "e2e" | "model" | "my" | "preview" | "test"
    )
}

#[cfg(test)]
mod model_selection_tests {
    use super::*;

    fn model(id: &str, name: &str, model: &str, usage_scenario: Option<&str>) -> ModelConfigRecord {
        ModelConfigRecord {
            id: id.to_string(),
            owner_user_id: Some("owner-1".to_string()),
            owner_username: None,
            owner_display_name: None,
            name: name.to_string(),
            provider: "openai".to_string(),
            prompt_vendor: Some("gpt".to_string()),
            base_url: "https://example.test/v1".to_string(),
            api_key: "secret".to_string(),
            model: model.to_string(),
            usage_scenario: usage_scenario.map(ToOwned::to_owned),
            temperature: None,
            max_output_tokens: None,
            model_request_max_retries: 5,
            thinking_level: None,
            supports_responses: true,
            instructions: None,
            request_cwd: None,
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
            enabled: true,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn generic_text_task_prefers_regular_text_model_over_image_test_model() {
        let models = vec![
            model(
                "image-test",
                "test / gemini image preview",
                "gemini-image-preview",
                None,
            ),
            model("regular", "my_api / gpt", "gpt", Some("常规任务")),
            model("complex", "my_api / pro", "pro", Some("复杂任务")),
        ];

        assert_eq!(
            select_model_config_id_for_task(
                models.as_slice(),
                "JDK 21 检查清单",
                "输出三项规划建议",
                None,
                &["planning".to_string()],
            )
            .as_deref(),
            Some("regular"),
        );
    }

    #[test]
    fn automatic_selection_ignores_models_without_cloud_credentials() {
        let mut credentialless = model("local-only", "my_api / gpt", "gpt", Some("常规任务"));
        credentialless.api_key.clear();
        credentialless.base_url.clear();
        let cloud = model("cloud-ready", "my_api / gpt", "gpt", Some("常规任务"));

        let eligible = enabled_model_configs_for_user(
            vec![credentialless, cloud],
            &crate::auth::CurrentUser {
                id: "agent-1".to_string(),
                username: "agent".to_string(),
                display_name: "Agent".to_string(),
                role: crate::models::UserRole::Agent,
                owner_user_id: Some("owner-1".to_string()),
                owner_username: Some("owner".to_string()),
                owner_display_name: Some("Owner".to_string()),
            },
        );

        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible[0].id, "cloud-ready");
    }

    #[test]
    fn image_generation_task_prefers_image_model() {
        let models = vec![
            model("regular", "my_api / gpt", "gpt", Some("常规任务")),
            model("image", "my_api / image", "gpt-image", Some("图片生成")),
        ];

        assert_eq!(
            select_model_config_id_for_task(
                models.as_slice(),
                "生成产品海报",
                "创建一张图片",
                None,
                &[],
            )
            .as_deref(),
            Some("image"),
        );
    }

    #[test]
    fn explicit_complex_usage_match_overrides_regular_default() {
        let models = vec![
            model("regular", "my_api / gpt", "gpt", Some("常规任务")),
            model("complex", "my_api / pro", "pro", Some("复杂任务")),
        ];

        assert_eq!(
            select_model_config_id_for_task(
                models.as_slice(),
                "复杂任务：架构改造",
                "输出详细方案",
                None,
                &[],
            )
            .as_deref(),
            Some("complex"),
        );
    }

    #[test]
    fn auto_model_tag_does_not_accidentally_select_auto_review_model() {
        let models = vec![
            model(
                "review",
                "my_api / codex-auto-review",
                "codex-auto-review",
                Some("审批任务"),
            ),
            model("regular", "my_api / gpt", "gpt", Some("常规任务")),
        ];

        assert_eq!(
            select_model_config_id_for_task(
                models.as_slice(),
                "会议准备事项",
                "由 Task Runner 自动选择模型并输出两条建议",
                None,
                &["e2e".to_string(), "auto-model".to_string()],
            )
            .as_deref(),
            Some("regular"),
        );
    }

    #[test]
    fn test_tag_does_not_accidentally_select_test_config() {
        let models = vec![
            model("test", "test / gpt", "gpt", None),
            model("regular", "my_api / gpt", "gpt", Some("常规任务")),
        ];

        assert_eq!(
            select_model_config_id_for_task(
                models.as_slice(),
                "JDK 21 检查清单",
                "输出只读规划建议",
                None,
                &["e2e-receipt-test".to_string()],
            )
            .as_deref(),
            Some("regular"),
        );
    }
}
