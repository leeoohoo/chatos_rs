// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;
use serde_json::Value;

use crate::core::chat_stream::build_chat_turn_persisted_messages_payload;
use crate::core::time::now_rfc3339;
use crate::models::pet_activity_inbox::{
    PetActivityInboxRecord, PetActivityInboxUpsert, PetActivityRoute,
};
use crate::repositories::pet_activity_inbox::{update_pet_activity_detail, upsert_pet_activity};
use crate::services::realtime::{publish_pet_activity_inbox_updated, AskUserPromptRealtimePayload};
use crate::services::task_manager::TaskRecord;

pub fn project_task_board_event(
    user_id: &str,
    conversation_id: &str,
    conversation_turn_id: Option<&str>,
    review_id: Option<&str>,
    task_id: Option<&str>,
    action: &str,
    task: Option<TaskRecord>,
) {
    let Some(handle) = tokio::runtime::Handle::try_current().ok() else {
        return;
    };
    let input = task_board_upsert(
        user_id,
        conversation_id,
        conversation_turn_id,
        review_id,
        task_id,
        action,
        task,
    );
    let Some(input) = input else { return };
    handle.spawn(persist_and_publish(input));
}

pub fn project_ask_user_prompt_event(user_id: &str, update: &AskUserPromptRealtimePayload) {
    let Some(handle) = tokio::runtime::Handle::try_current().ok() else {
        return;
    };
    let status = update
        .status
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let action = update.action.to_ascii_lowercase();
    let resolved = action.contains("resolved")
        || action.contains("cancel")
        || matches!(
            status.as_str(),
            "ok" | "submitted" | "completed" | "cancelled" | "canceled" | "timeout" | "expired"
        );
    let title = update
        .title
        .as_deref()
        .and_then(trimmed_non_empty)
        .unwrap_or("AI 正在等待你的输入")
        .to_string();
    let input = PetActivityInboxUpsert {
        user_id: user_id.to_string(),
        activity_key: format!("ask-user:{}", update.prompt_id),
        activity_version: "1".to_string(),
        source: "ask_user_prompt".to_string(),
        kind: "waiting_for_user".to_string(),
        title,
        detail: update
            .message
            .as_deref()
            .and_then(trimmed_non_empty)
            .map(ToOwned::to_owned),
        route: PetActivityRoute {
            project_id: update.project_id.clone(),
            conversation_id: Some(update.conversation_id.clone()),
            turn_id: update.conversation_turn_id.clone(),
            prompt_id: Some(update.prompt_id.clone()),
            ..Default::default()
        },
        business_status: if status.is_empty() {
            update.action.clone()
        } else {
            status
        },
        requires_action: !resolved,
        event_id: None,
        event_sequence: None,
        metadata: Some(json!({
            "action": update.action,
            "prompt_kind": update.prompt_kind,
            "tool_call_id": update.tool_call_id,
            "allow_cancel": update.allow_cancel,
            "timeout_ms": update.timeout_ms,
        })),
        occurred_at: now_rfc3339(),
        expires_at: None,
        resolved,
    };
    handle.spawn(persist_and_publish(input));
}

pub fn project_chat_stream_event(
    user_id: &str,
    conversation_id: &str,
    conversation_turn_id: Option<&str>,
    project_id: Option<&str>,
    user_message_id: Option<&str>,
    event: &str,
    stream_type: &str,
    raw: &Value,
) {
    let Some(handle) = tokio::runtime::Handle::try_current().ok() else {
        return;
    };
    let raw_type = raw
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or(stream_type)
        .to_ascii_lowercase();
    if raw_type == "task_runner_callback" || event == "chat.task_runner.updated" {
        return;
    }
    let event_lower = event.to_ascii_lowercase();
    let mapping = if raw_type == "start" || event_lower.contains("turn.started") {
        Some((
            "working",
            "AI 已开始处理任务".to_string(),
            Some("正在读取需求和项目上下文".to_string()),
            false,
        ))
    } else if raw_type.contains("thinking") {
        Some(("working", "AI 正在分析需求".to_string(), None, false))
    } else if raw_type.contains("turn_phase") || raw_type == "phase" {
        Some((
            "reviewing",
            "AI 进入新的处理阶段".to_string(),
            safe_chat_detail(raw, &["phase", "status", "name"]),
            false,
        ))
    } else if raw_type.contains("tools_start") || event_lower.contains("tool.started") {
        Some(("working", "AI 正在调用工具".to_string(), None, false))
    } else if raw_type.contains("tools_end") || event_lower.contains("tool.completed") {
        Some((
            "working",
            "工具调用已完成".to_string(),
            Some("AI 正在继续处理结果".to_string()),
            false,
        ))
    } else if raw_type.contains("complete")
        || raw_type.contains("finish")
        || event_lower.contains("completed")
        || event_lower.contains("finished")
    {
        Some((
            "succeeded",
            "AI 已完成本轮任务".to_string(),
            safe_chat_result_detail(raw),
            false,
        ))
    } else if raw_type.contains("fail")
        || raw_type.contains("error")
        || event_lower.contains("failed")
    {
        Some((
            "failed",
            "AI 执行失败".to_string(),
            safe_chat_error_detail(raw),
            true,
        ))
    } else if raw_type.contains("cancel") || event_lower.contains("cancelled") {
        Some(("cancelled", "AI 执行已取消".to_string(), None, false))
    } else {
        None
    };
    let Some((kind, title, detail, requires_action)) = mapping else {
        return;
    };
    let version = conversation_turn_id
        .and_then(trimmed_non_empty)
        .or_else(|| user_message_id.and_then(trimmed_non_empty))
        .unwrap_or("conversation")
        .to_string();
    let input = PetActivityInboxUpsert {
        user_id: user_id.to_string(),
        activity_key: format!("chat:{conversation_id}:{version}"),
        activity_version: "1".to_string(),
        source: "chat".to_string(),
        kind: kind.to_string(),
        title,
        detail,
        route: PetActivityRoute {
            project_id: project_id.map(ToOwned::to_owned),
            conversation_id: Some(conversation_id.to_string()),
            turn_id: conversation_turn_id.map(ToOwned::to_owned),
            message_id: user_message_id.map(ToOwned::to_owned),
            ..Default::default()
        },
        business_status: raw_type,
        requires_action,
        event_id: None,
        event_sequence: None,
        metadata: Some(json!({ "event": event, "stream_type": stream_type })),
        occurred_at: now_rfc3339(),
        expires_at: None,
        resolved: false,
    };
    handle.spawn(persist_and_publish(input));
}

pub async fn hydrate_missing_chat_result_details(
    user_id: &str,
    activities: &mut [PetActivityInboxRecord],
) {
    for activity in activities {
        if activity.source != "chat"
            || activity.kind != "succeeded"
            || activity
                .detail
                .as_deref()
                .and_then(trimmed_non_empty)
                .is_some()
        {
            continue;
        }
        let Some(conversation_id) = activity
            .route
            .conversation_id
            .as_deref()
            .and_then(trimmed_non_empty)
        else {
            continue;
        };
        let Some(payload) = build_chat_turn_persisted_messages_payload(
            conversation_id,
            activity.route.turn_id.as_deref(),
            activity.route.message_id.as_deref(),
        )
        .await
        else {
            continue;
        };
        let Some(detail) = safe_chat_result_detail(&payload) else {
            continue;
        };
        if let Err(err) = update_pet_activity_detail(user_id, &activity.id, &detail).await {
            tracing::warn!(
                user_id,
                activity_id = activity.id,
                error = err,
                "persist hydrated pet activity detail failed"
            );
        }
        activity.detail = Some(detail);
    }
}

#[derive(Debug, Clone)]
pub struct TaskRunnerPetActivityInput {
    pub user_id: String,
    pub conversation_id: String,
    pub project_id: Option<String>,
    pub turn_id: Option<String>,
    pub message_id: Option<String>,
    pub task_id: String,
    pub run_id: Option<String>,
    pub event: String,
    pub status: String,
    pub task_title: String,
    pub detail: Option<String>,
    pub occurred_at: Option<String>,
}

pub async fn record_task_runner_activity(input: TaskRunnerPetActivityInput) -> Result<(), String> {
    let event = input.event.to_ascii_lowercase();
    let (kind, title, requires_action) = match event.as_str() {
        "task.created" | "task.run.queued" => (
            "working",
            format!(
                "任务「{}」已进入执行队列",
                display_task_title(&input.task_title)
            ),
            false,
        ),
        "task.run.started" => (
            "working",
            format!("任务「{}」正在执行", display_task_title(&input.task_title)),
            false,
        ),
        "task.completed" => (
            "succeeded",
            format!("任务「{}」已完成", display_task_title(&input.task_title)),
            false,
        ),
        "task.failed" => (
            "failed",
            format!("任务「{}」执行失败", display_task_title(&input.task_title)),
            true,
        ),
        "task.blocked" => (
            "blocked",
            format!("任务「{}」被阻塞", display_task_title(&input.task_title)),
            true,
        ),
        "task.cancelled" => (
            "cancelled",
            format!("任务「{}」已取消", display_task_title(&input.task_title)),
            false,
        ),
        _ => return Ok(()),
    };
    let version = input
        .run_id
        .as_deref()
        .and_then(trimmed_non_empty)
        .unwrap_or("legacy")
        .to_string();
    let record = upsert_pet_activity(PetActivityInboxUpsert {
        user_id: input.user_id.clone(),
        activity_key: format!("task-runner:{}", input.task_id),
        activity_version: version,
        source: "task_runner".to_string(),
        kind: kind.to_string(),
        title,
        detail: input
            .detail
            .as_deref()
            .and_then(trimmed_non_empty)
            .map(ToOwned::to_owned),
        route: PetActivityRoute {
            project_id: input.project_id,
            conversation_id: Some(input.conversation_id),
            turn_id: input.turn_id,
            message_id: input.message_id,
            task_id: Some(input.task_id.clone()),
            run_id: input.run_id,
            ..Default::default()
        },
        business_status: if input.status.trim().is_empty() {
            input.event.clone()
        } else {
            input.status
        },
        requires_action,
        event_id: None,
        event_sequence: None,
        metadata: Some(json!({ "event": input.event })),
        occurred_at: input.occurred_at.unwrap_or_else(now_rfc3339),
        expires_at: None,
        resolved: false,
    })
    .await?;
    publish_pet_activity_inbox_updated(&input.user_id, "upserted", &record);
    Ok(())
}

async fn persist_and_publish(input: PetActivityInboxUpsert) {
    let user_id = input.user_id.clone();
    match upsert_pet_activity(input).await {
        Ok(record) => publish_pet_activity_inbox_updated(&user_id, "upserted", &record),
        Err(err) => tracing::warn!(
            user_id = user_id,
            error = err,
            "persist pet activity failed"
        ),
    }
}

fn task_board_upsert(
    user_id: &str,
    conversation_id: &str,
    conversation_turn_id: Option<&str>,
    review_id: Option<&str>,
    task_id: Option<&str>,
    action: &str,
    task: Option<TaskRecord>,
) -> Option<PetActivityInboxUpsert> {
    let occurred_at = task
        .as_ref()
        .map(|record| record.updated_at.clone())
        .unwrap_or_else(now_rfc3339);
    if let Some(review_id) = review_id.and_then(trimmed_non_empty) {
        let resolved = matches!(action, "review_confirmed" | "review_cancelled");
        return Some(PetActivityInboxUpsert {
            user_id: user_id.to_string(),
            activity_key: format!("task-review:{review_id}"),
            activity_version: "1".to_string(),
            source: "task_board".to_string(),
            kind: "waiting_for_user".to_string(),
            title: "执行计划等待确认".to_string(),
            detail: Some("请检查任务节点和依赖关系".to_string()),
            route: PetActivityRoute {
                conversation_id: Some(conversation_id.to_string()),
                turn_id: conversation_turn_id.map(ToOwned::to_owned),
                ..Default::default()
            },
            business_status: action.to_string(),
            requires_action: !resolved,
            event_id: None,
            event_sequence: None,
            metadata: None,
            occurred_at,
            expires_at: None,
            resolved,
        });
    }

    let record = task?;
    let task_id = task_id
        .and_then(trimmed_non_empty)
        .unwrap_or(record.id.as_str());
    let status = record.status.to_ascii_lowercase();
    let title = display_task_title(&record.title);
    let (kind, display_title, requires_action) = match status.as_str() {
        "running" | "processing" | "in_progress" | "doing" => {
            ("working", format!("任务「{title}」正在执行"), false)
        }
        "completed" | "done" | "succeeded" | "success" => {
            ("succeeded", format!("任务「{title}」已完成"), false)
        }
        "failed" | "error" => ("failed", format!("任务「{title}」执行失败"), true),
        "blocked" => ("blocked", format!("任务「{title}」被阻塞"), true),
        "cancelled" | "canceled" | "stopped" => {
            ("cancelled", format!("任务「{title}」已取消"), false)
        }
        _ => return None,
    };
    let detail = match kind {
        "blocked" => trimmed_non_empty(&record.blocker_reason)
            .or_else(|| trimmed_non_empty(&record.resume_hint)),
        "failed" => trimmed_non_empty(&record.outcome_summary)
            .or_else(|| trimmed_non_empty(&record.details)),
        "succeeded" => trimmed_non_empty(&record.outcome_summary),
        _ => trimmed_non_empty(&record.details),
    }
    .map(ToOwned::to_owned);
    Some(PetActivityInboxUpsert {
        user_id: user_id.to_string(),
        activity_key: format!("task-board:{task_id}"),
        activity_version: conversation_turn_id
            .and_then(trimmed_non_empty)
            .unwrap_or(record.conversation_turn_id.as_str())
            .to_string(),
        source: "task_board".to_string(),
        kind: kind.to_string(),
        title: display_title,
        detail,
        route: PetActivityRoute {
            conversation_id: Some(conversation_id.to_string()),
            turn_id: Some(record.conversation_turn_id.clone()),
            task_id: Some(task_id.to_string()),
            ..Default::default()
        },
        business_status: status,
        requires_action,
        event_id: None,
        event_sequence: None,
        metadata: Some(json!({ "action": action })),
        occurred_at,
        expires_at: None,
        resolved: false,
    })
}

fn display_task_title(value: &str) -> &str {
    trimmed_non_empty(value).unwrap_or("任务")
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn safe_chat_detail(raw: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = raw
            .get("data")
            .and_then(|data| data.get(*key))
            .and_then(Value::as_str)
            .and_then(trimmed_non_empty)
        {
            return Some(value.chars().take(240).collect());
        }
    }
    None
}

fn safe_chat_error_detail(raw: &Value) -> Option<String> {
    for value in [
        raw.get("error").and_then(Value::as_str),
        raw.get("message").and_then(Value::as_str),
        raw.get("data")
            .and_then(|data| data.get("error"))
            .and_then(Value::as_str),
        raw.get("data")
            .and_then(|data| data.get("message"))
            .and_then(Value::as_str),
    ] {
        if let Some(value) = value.and_then(trimmed_non_empty) {
            return Some(value.chars().take(600).collect());
        }
    }
    None
}

fn safe_chat_result_detail(raw: &Value) -> Option<String> {
    let result = raw.get("result").unwrap_or(raw);
    let candidates = [
        result
            .get("persisted_assistant_message")
            .and_then(|message| message.get("content")),
        result.get("content"),
        result.get("message").and_then(|message| {
            message
                .get("content")
                .or_else(|| message.as_str().map(|_| message))
        }),
        result.get("value").and_then(|value| value.get("content")),
        result.as_str().map(|_| result),
    ];

    candidates
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find_map(|value| bounded_visible_text(value, 4_000))
}

fn bounded_visible_text(value: &str, max_chars: usize) -> Option<String> {
    let value = trimmed_non_empty(value)?;
    let mut chars = value.chars();
    let mut text: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        text.push('…');
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::{safe_chat_result_detail, task_board_upsert};
    use crate::services::task_manager::TaskRecord;
    use serde_json::json;

    fn task(status: &str) -> TaskRecord {
        TaskRecord {
            id: "task-1".to_string(),
            conversation_id: "conversation-1".to_string(),
            conversation_turn_id: "turn-1".to_string(),
            title: "整理发布说明".to_string(),
            details: String::new(),
            priority: "medium".to_string(),
            status: status.to_string(),
            tags: vec![],
            due_at: None,
            outcome_summary: "已整理完成".to_string(),
            outcome_items: vec![],
            resume_hint: String::new(),
            blocker_reason: "等待用户选择发布渠道".to_string(),
            blocker_needs: vec![],
            blocker_kind: String::new(),
            completed_at: None,
            last_outcome_at: None,
            created_at: "2026-08-28T00:00:00Z".to_string(),
            updated_at: "2026-08-28T00:01:00Z".to_string(),
        }
    }

    #[test]
    fn completed_task_is_persistent_until_user_disposition() {
        let activity = task_board_upsert(
            "user-1",
            "conversation-1",
            Some("turn-1"),
            None,
            Some("task-1"),
            "task_updated",
            Some(task("completed")),
        )
        .expect("activity");
        assert_eq!(activity.kind, "succeeded");
        assert_eq!(activity.expires_at, None);
        assert!(!activity.resolved);
    }

    #[test]
    fn blocked_task_keeps_actionable_detail() {
        let activity = task_board_upsert(
            "user-1",
            "conversation-1",
            Some("turn-1"),
            None,
            Some("task-1"),
            "task_updated",
            Some(task("blocked")),
        )
        .expect("activity");
        assert_eq!(activity.kind, "blocked");
        assert!(activity.requires_action);
        assert_eq!(activity.detail.as_deref(), Some("等待用户选择发布渠道"));
    }

    #[test]
    fn chat_completion_uses_persisted_assistant_content() {
        let raw = json!({
            "type": "complete",
            "result": {
                "content": "流式结果",
                "persisted_assistant_message": {
                    "content": "最终保存的回答",
                    "reasoning": "不应展示的推理"
                }
            }
        });

        assert_eq!(
            safe_chat_result_detail(&raw).as_deref(),
            Some("最终保存的回答")
        );
    }

    #[test]
    fn chat_completion_falls_back_to_result_content_without_exposing_reasoning() {
        let raw = json!({
            "type": "complete",
            "result": {
                "content": "可见回答",
                "thinking": "隐藏思考",
                "reasoning": "隐藏推理"
            }
        });

        assert_eq!(safe_chat_result_detail(&raw).as_deref(), Some("可见回答"));
    }

    #[test]
    fn chat_completion_without_visible_answer_has_no_detail() {
        let raw = json!({
            "type": "complete",
            "result": {
                "thinking": "只有隐藏思考",
                "reasoning": "只有隐藏推理"
            }
        });

        assert_eq!(safe_chat_result_detail(&raw), None);
    }
}
