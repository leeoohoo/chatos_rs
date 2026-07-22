// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use super::{
    apply_task_runner_callback_to_user_message,
    build_task_runner_callback_assistant_message_with_contact,
    build_task_runner_callback_message_id, is_task_runner_terminal_event,
    TaskRunnerCallbackRequest,
};
use crate::models::message::Message;

fn sample_callback_payload() -> TaskRunnerCallbackRequest {
    TaskRunnerCallbackRequest {
        event: "task.completed".to_string(),
        task_id: "task-1".to_string(),
        run_id: Some("run-1".to_string()),
        status: "succeeded".to_string(),
        task_title: "Demo task".to_string(),
        task_objective: "Complete the requested demo work.".to_string(),
        fallback_locale: "zh-CN".to_string(),
        project_id: Some("project-1".to_string()),
        task_status: Some("succeeded".to_string()),
        result_summary: Some("done".to_string()),
        error_message: None,
        report_content: None,
        source_session_id: Some("session-1".to_string()),
        source_turn_id: Some("turn-1".to_string()),
        source_user_message_id: Some("user-1".to_string()),
        parent_task_id: None,
        source_run_id: None,
        schedule_mode: Some("once".to_string()),
        prompt: None,
        callback_at: Some("2026-06-10T10:00:00Z".to_string()),
    }
}

fn build_task_runner_callback_assistant_message(
    session_id: &str,
    payload: &TaskRunnerCallbackRequest,
) -> Message {
    build_task_runner_callback_assistant_message_with_contact(session_id, payload, None)
}

#[test]
fn callback_message_id_is_deterministic_for_same_run() {
    let payload = sample_callback_payload();
    let id = build_task_runner_callback_message_id(&payload);
    assert_eq!(
        id,
        "task_runner_callback::user-1::task-1::task.completed::run-1"
    );
}

#[test]
fn callback_assistant_message_carries_idempotent_identity_and_async_metadata() {
    let payload = sample_callback_payload();
    let message = build_task_runner_callback_assistant_message("session-1", &payload);

    assert_eq!(
        message.id,
        "task_runner_callback::user-1::task-1::task.completed::run-1"
    );
    assert_eq!(message.created_at, "2026-06-10T10:00:00Z");
    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("mode"))
            .and_then(|value| value.as_str()),
        Some("contact_async")
    );
    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("conversation_turn_id"))
            .and_then(|value| value.as_str()),
        Some("turn-1")
    );
    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("source_turn_id"))
            .and_then(|value| value.as_str()),
        Some("turn-1")
    );
    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("callback_at"))
            .and_then(|value| value.as_str()),
        Some("2026-06-10T10:00:00Z")
    );
}

#[test]
fn callback_updates_task_tracking_without_overwriting_existing_message_status() {
    let mut message = Message::new(
        "session-1".to_string(),
        "user".to_string(),
        "please handle this".to_string(),
    );
    message.id = "user-1".to_string();
    message.metadata = Some(json!({
        "task_runner_async": {
            "overall_status": "completed"
        }
    }));

    let mut payload = sample_callback_payload();
    payload.event = "task.created".to_string();
    payload.task_id = "task-1".to_string();
    apply_task_runner_callback_to_user_message(&mut message, &payload);

    payload.event = "task.completed".to_string();
    apply_task_runner_callback_to_user_message(&mut message, &payload);

    let task_runner_async = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
        .cloned()
        .unwrap_or_else(|| json!({}));

    assert_eq!(
        task_runner_async
            .get("overall_status")
            .and_then(|value| value.as_str()),
        Some("completed")
    );
    assert_eq!(
        task_runner_async
            .get("created_task_ids")
            .and_then(|value| value.as_array())
            .map(|value| value.len()),
        Some(1)
    );
    assert_eq!(
        task_runner_async
            .get("succeeded_task_ids")
            .and_then(|value| value.as_array())
            .map(|value| value.len()),
        Some(1)
    );
}

#[test]
fn terminal_callback_marks_source_user_message_completed() {
    let mut message = Message::new(
        "session-1".to_string(),
        "user".to_string(),
        "please handle this".to_string(),
    );
    message.id = "user-1".to_string();
    message.metadata = Some(json!({
        "task_runner_async": {
            "overall_status": "processing"
        }
    }));

    let payload = sample_callback_payload();
    apply_task_runner_callback_to_user_message(&mut message, &payload);

    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("overall_status"))
            .and_then(|value| value.as_str()),
        Some("completed")
    );
}

#[test]
fn terminal_callback_keeps_group_processing_until_all_created_tasks_finish() {
    let mut message = Message::new(
        "session-1".to_string(),
        "user".to_string(),
        "please handle this".to_string(),
    );
    message.id = "user-1".to_string();
    message.metadata = Some(json!({
        "task_runner_async": {
            "overall_status": "processing",
            "created_task_ids": ["task-1", "task-2"],
            "running_task_ids": ["task-1", "task-2"],
            "terminal_task_ids": []
        }
    }));

    let mut payload = sample_callback_payload();
    apply_task_runner_callback_to_user_message(&mut message, &payload);
    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("overall_status"))
            .and_then(|value| value.as_str()),
        Some("processing")
    );

    payload.task_id = "task-2".to_string();
    payload.run_id = Some("run-2".to_string());
    apply_task_runner_callback_to_user_message(&mut message, &payload);
    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("overall_status"))
            .and_then(|value| value.as_str()),
        Some("completed")
    );
}

#[test]
fn task_runner_terminal_event_includes_failed_blocked_and_cancelled() {
    assert!(is_task_runner_terminal_event("task.completed"));
    assert!(is_task_runner_terminal_event("task.failed"));
    assert!(is_task_runner_terminal_event("task.blocked"));
    assert!(is_task_runner_terminal_event("task.cancelled"));
    assert!(!is_task_runner_terminal_event("task.created"));
}

#[test]
fn failed_callback_assistant_message_keeps_error_detail() {
    let mut payload = sample_callback_payload();
    payload.event = "task.failed".to_string();
    payload.status = "failed".to_string();
    payload.result_summary = None;
    payload.error_message = Some("memory batch sync failed".to_string());

    let message = build_task_runner_callback_assistant_message("session-1", &payload);

    assert!(message.content.contains("Task “Demo task” failed"));
    assert!(message.content.contains("Error:"));
    assert!(message.content.contains("memory batch sync failed"));
    assert_eq!(
        message
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("event"))
            .and_then(|value| value.as_str()),
        Some("task.failed")
    );
}

#[test]
fn failed_callback_hides_transient_provider_url_from_chinese_user() {
    let mut payload = sample_callback_payload();
    payload.task_title = "规划复杂需求".to_string();
    payload.task_objective = "生成中文需求、文档和任务。".to_string();
    payload.event = "task.failed".to_string();
    payload.status = "failed".to_string();
    payload.result_summary = None;
    payload.error_message = Some(
        "AI 请求失败：error sending request for url (https://internal.example/v1/responses)"
            .to_string(),
    );

    let message = build_task_runner_callback_assistant_message("session-1", &payload);

    assert!(message.content.contains("服务暂时不可用，请稍后重试。"));
    assert!(!message.content.contains("internal.example"));
    assert!(!message.content.contains("error sending request for url"));
    let metadata_error = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
        .and_then(|value| value.get("error_message"))
        .and_then(|value| value.as_str());
    assert_eq!(metadata_error, Some("服务暂时不可用，请稍后重试。"));
}

#[test]
fn failed_callback_hides_internal_prompt_resolution_details() {
    let mut payload = sample_callback_payload();
    payload.task_title = "实现复杂领域契约".to_string();
    payload.task_objective = "完成中文项目实施任务。".to_string();
    payload.event = "task.failed".to_string();
    payload.status = "failed".to_string();
    payload.result_summary = Some(
        "task_runner_run_phase failed: resolve published prompt for vendor gpt failed: plugin management request was rejected with status 409: agent_prompt_checksum_invalid"
            .to_string(),
    );
    payload.error_message = Some("agent_prompt_checksum_invalid".to_string());

    let message = build_task_runner_callback_assistant_message("session-1", &payload);

    assert!(message.content.contains("任务暂时无法启动，请稍后重试。"));
    assert!(!message.content.contains("task_runner_run_phase"));
    assert!(!message.content.contains("vendor gpt"));
    assert!(!message.content.contains("checksum"));
    let metadata_summary = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
        .and_then(|value| value.get("result_summary"))
        .and_then(|value| value.as_str());
    assert_eq!(metadata_summary, Some("任务暂时无法启动，请稍后重试。"));
}

#[test]
fn chinese_callback_wrapper_follows_task_language() {
    let mut payload = sample_callback_payload();
    payload.task_title = "创建中文回归产物".to_string();
    payload.task_objective = "创建中文需求、技术文档和项目任务。".to_string();
    payload.result_summary = Some("全部中文产物均已创建。".to_string());

    let message = build_task_runner_callback_assistant_message("session-1", &payload);

    assert!(message.content.contains("任务「创建中文回归产物」已完成"));
    assert!(message.content.contains("结果摘要："));
}

#[test]
fn english_callback_wrapper_overrides_chinese_ui_fallback() {
    let payload = sample_callback_payload();

    let message = build_task_runner_callback_assistant_message("session-1", &payload);

    assert!(message.content.contains("Task “Demo task” completed"));
    assert!(message.content.contains("Result summary:"));
    assert!(!message.content.contains("结果摘要"));
}

#[test]
fn completed_callback_uses_task_objective_when_report_has_no_readable_summary() {
    let mut payload = sample_callback_payload();
    payload.result_summary = None;
    payload.report_content = Some("A".repeat(5_000));
    let message = build_task_runner_callback_assistant_message("session-1", &payload);
    let task_runner_async = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
        .cloned()
        .unwrap_or_else(|| json!({}));

    assert!(message.content.contains("Task “Demo task” completed"));
    assert!(message
        .content
        .contains("Complete the requested demo work."));
    assert!(message
        .content
        .contains("More implementation details are available in the task details."));
    assert!(message.content.chars().count() < 400);
    assert_eq!(
        task_runner_async
            .get("detail_source")
            .and_then(|value| value.as_str()),
        Some("task_objective")
    );
    assert!(task_runner_async
        .get("detail_preview")
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.chars().count() < 200));
}

#[test]
fn verbose_chinese_completion_report_becomes_concise_receipt() {
    let mut payload = sample_callback_payload();
    payload.task_title = "实现组合筛选与中文活动时间线".to_string();
    payload.task_objective = "完成中文项目实施任务。".to_string();
    payload.result_summary = Some(
        "## 已完成\n\n- 修改 src/service.js\n- 调用 /api/dashboard\n- 执行 npm test\n- VALIDATION_ERROR 已覆盖"
            .to_string(),
    );

    let message = build_task_runner_callback_assistant_message("session-1", &payload);

    assert!(message
        .content
        .contains("任务「实现组合筛选与中文活动时间线」已完成"));
    assert!(message
        .content
        .contains("已完成相关功能实现与接口联调，并通过相关检查与测试验证。"));
    assert!(message.content.contains("更多实施细节可在任务详情中查看"));
    assert!(!message.content.contains("src/service.js"));
    assert!(!message.content.contains("/api/dashboard"));
    assert!(!message.content.contains("VALIDATION_ERROR"));
}

#[test]
fn completed_callback_shows_a_short_user_facing_result_in_chat() {
    let mut payload = sample_callback_payload();
    payload.task_title = "梳理项目用途与核心模块".to_string();
    payload.task_objective = "帮助用户快速了解当前项目。".to_string();
    payload.result_summary = Some(
        "## 结果摘要\n\n已梳理该项目的主要用途、核心业务流程和关键模块。\n- 明确了项目面向的用户场景。\n- 总结了主要功能边界和技术组成。"
            .to_string(),
    );

    let message = build_task_runner_callback_assistant_message("session-1", &payload);

    assert!(message
        .content
        .contains("已梳理该项目的主要用途、核心业务流程和关键模块。"));
    assert!(message.content.contains("明确了项目面向的用户场景"));
    assert!(message.content.contains("总结了主要功能边界和技术组成"));
    assert!(message.content.contains("更多实施细节可在任务详情中查看"));
}

#[test]
fn callback_message_hides_internal_ids_and_normalizes_raw_enums() {
    let mut payload = sample_callback_payload();
    payload.event = "task.failed".to_string();
    payload.status = "failed".to_string();
    payload.task_title = "创建中文规划产物".to_string();
    payload.task_objective = "创建中文需求、技术文档和项目任务。".to_string();
    payload.result_summary = Some(
        "已创建需求：任务回执可靠性监控与补偿\n\
- requirement_id: 6f7854a9-7a6e-4aef-887b-9de81198f349\n\
- 技术文档：任务回执可靠性监控与补偿技术规划概览\n\
- document_id: a2e4e958-4b0f-4092-8856-eb523716484c\n\
- 文档类型：technical_overview\n\
- 已确认 blocks 顺序关系落库。"
            .to_string(),
    );

    let message = build_task_runner_callback_assistant_message("session-1", &payload);

    assert!(message.content.contains("任务回执可靠性监控与补偿"));
    assert!(message.content.contains("技术概览"));
    assert!(message.content.contains("前置依赖"));
    assert!(message.content.contains("顺序关系"));
    assert!(!message.content.contains("requirement_id"));
    assert!(!message.content.contains("document_id"));
    assert!(!message.content.contains("6f7854a9"));
    assert!(!message.content.contains("technical_overview"));
    assert!(!message.content.contains("blocks"));

    let detail_preview = message
        .metadata
        .as_ref()
        .and_then(|value| value.get("task_runner_async"))
        .and_then(|value| value.get("detail_preview"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    assert!(!detail_preview.contains("requirement_id"));
    assert!(!detail_preview.contains("6f7854a9"));
}

#[test]
fn callback_message_strips_nonstandard_ids_and_internal_runtime_terms() {
    let mut payload = sample_callback_payload();
    payload.event = "task.failed".to_string();
    payload.status = "failed".to_string();
    payload.task_title = "创建实施任务".to_string();
    payload.task_objective = "创建中文实施任务和依赖关系。".to_string();
    payload.result_summary = Some(
        "已完成本轮项目管理落库。\n\
- `3b7f760-d068-4596-99be-944fe29c975` 改造任务回执发送主链路\n\
- 已重新拉取项目依赖图，返回 `ready=true`\n\
- `get_project_dependency_graph()`：确认 contains / blocks 关系落库\n\
- 将该 requirement 从 `draft` 推进到 `reviewing` 或 `approved`\n\
- 当前项目运行环境仍为 `pending`\n\
- 继续补充 implementation plan 文档"
            .to_string(),
    );

    let message = build_task_runner_callback_assistant_message("session-1", &payload);

    assert!(message.content.contains("改造任务回执发送主链路"));
    assert!(message.content.contains("依赖关系已就绪"));
    assert!(message.content.contains("项目依赖关系检查"));
    assert!(message.content.contains("包含关系"));
    assert!(message.content.contains("前置依赖"));
    assert!(message.content.contains("将该需求从"));
    assert!(message.content.contains("草稿"));
    assert!(message.content.contains("评审中"));
    assert!(message.content.contains("已批准"));
    assert!(message.content.contains("待准备"));
    assert!(message.content.contains("实施计划"));
    assert!(!message.content.contains("3b7f760"));
    assert!(!message.content.contains("ready=true"));
    assert!(!message.content.contains("get_project_dependency_graph"));
    assert!(!message.content.contains("contains / blocks"));
    assert!(!message.content.contains("requirement"));
    assert!(!message.content.contains("`draft`"));
    assert!(!message.content.contains("pending"));
    assert!(!message.content.contains("implementation plan"));
}
