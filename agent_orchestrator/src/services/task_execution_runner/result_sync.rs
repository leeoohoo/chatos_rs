use serde_json::{json, Value};
use tracing::warn;

use crate::core::messages::MessageOut;
use crate::models::message::Message;
use crate::services::im_task_runtime_bridge::publish_task_notice_message_best_effort;
use crate::services::memory_server_client;
use crate::services::session_event_hub::session_event_hub;
use crate::services::task_service_client::{
    self, TaskExecutionScopeDto, TaskHandoffPayloadDto, TaskRecordDto, UpdateTaskRequestDto,
};

pub(super) async fn save_task_notice_message(
    session_id: Option<&str>,
    notice_type: &str,
    event: &str,
    scope: &TaskExecutionScopeDto,
    task: Option<&TaskRecordDto>,
    content: String,
) -> Result<Option<Message>, String> {
    let Some(session_id) = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
    else {
        return Ok(None);
    };

    let message = Message {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        role: "assistant".to_string(),
        content,
        message_mode: Some("task_notice".to_string()),
        message_source: Some("task_execution_runner".to_string()),
        summary: None,
        tool_calls: None,
        tool_call_id: None,
        reasoning: None,
        metadata: Some(json!({
            "type": notice_type,
            "task_execution": {
                "event": event,
                "scope_key": scope.scope_key,
                "user_id": scope.user_id,
                "contact_agent_id": scope.contact_agent_id,
                "project_id": scope.project_id,
                "task_id": task.map(|item| item.id.clone()),
                "task_title": task.map(|item| item.title.clone()),
            }
        })),
        created_at: crate::core::time::now_rfc3339(),
    };
    let saved = memory_server_client::upsert_message(&message).await?;
    publish_task_notice_message_best_effort(
        session_id.as_str(),
        event,
        saved.content.as_str(),
        task,
        Some(saved.id.as_str()),
    )
    .await;
    let payload = json!({
        "type": "task_execution.notice",
        "timestamp": crate::core::time::now_rfc3339(),
        "event": event,
        "session_id": session_id,
        "message": serde_json::to_value(MessageOut::from(saved.clone())).unwrap_or(Value::Null),
        "task": task,
        "scope": scope,
    });
    session_event_hub().publish(session_id.as_str(), payload);
    Ok(Some(saved))
}

pub(super) async fn persist_task_handoff(
    scope: &TaskExecutionScopeDto,
    task: &TaskRecordDto,
    handoff_kind: &str,
    summary: &str,
    result_message_id: Option<&str>,
    result_brief: Option<&memory_server_client::TaskResultBriefDto>,
    last_error: Option<&str>,
    checkpoint_message_id: Option<&str>,
) -> Result<(), String> {
    let summary = compact_result_summary(summary);
    if summary.trim().is_empty() {
        return Ok(());
    }

    let task_kind = task
        .task_kind
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("task");
    let mut key_changes = Vec::new();
    if handoff_kind == "completed" {
        key_changes.push(format!("{} 已完成：{}", task_kind, task.title));
    } else if handoff_kind == "failed" {
        key_changes.push(format!("{} 执行失败：{}", task_kind, task.title));
    } else if handoff_kind == "checkpoint" {
        key_changes.push(format!("{} 已暂停：{}", task_kind, task.title));
    }

    let verification_suggestions = if !task.acceptance_criteria.is_empty() {
        task.acceptance_criteria.clone()
    } else if handoff_kind == "completed" && task_kind == "implementation" {
        vec!["请针对本次实现补充验证结论。".to_string()]
    } else {
        Vec::new()
    };

    let mut open_risks = Vec::new();
    if let Some(error) = last_error.map(str::trim).filter(|value| !value.is_empty()) {
        open_risks.push(error.to_string());
    }
    if let Some(pause_reason) = task
        .pause_reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        open_risks.push(format!("暂停原因：{}", pause_reason));
    }

    let mut artifact_refs = Vec::new();
    if let Some(session_id) = task
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        artifact_refs.push(format!("session:{}", session_id));
    }
    if let Some(turn_id) = task
        .conversation_turn_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        artifact_refs.push(format!("turn:{}", turn_id));
    }
    if let Some(message_id) = result_message_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        artifact_refs.push(format!("result_message:{}", message_id));
    }

    let checkpoint_message_ids = checkpoint_message_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| vec![value.to_string()])
        .unwrap_or_default();

    task_service_client::update_task_internal(
        task.id.as_str(),
        &UpdateTaskRequestDto {
            handoff_payload: Some(Some(TaskHandoffPayloadDto {
                task_id: task.id.clone(),
                task_plan_id: task.task_plan_id.clone(),
                handoff_kind: handoff_kind.trim().to_string(),
                summary,
                result_summary: task.result_summary.clone(),
                key_changes,
                changed_files: Vec::new(),
                executed_commands: Vec::new(),
                verification_suggestions,
                open_risks,
                artifact_refs,
                checkpoint_message_ids,
                result_brief_id: result_brief.map(|item| item.id.clone()),
                generated_at: crate::core::time::now_rfc3339(),
            })),
            ..UpdateTaskRequestDto::default()
        },
    )
    .await?;

    if let Some(updated_task) = task_service_client::get_task(task.id.as_str()).await? {
        crate::services::im_task_runtime_bridge::publish_task_runtime_update_best_effort(&updated_task)
            .await;
    } else {
        warn!(
            "[TASK-RUNNER] handoff persisted but refreshed task missing: scope={} task_id={}",
            scope.scope_key, task.id
        );
    }
    Ok(())
}

pub(super) async fn sync_task_result_brief(
    scope: &TaskExecutionScopeDto,
    task: &TaskRecordDto,
    task_status: &str,
    result_summary: &str,
    result_message_id: Option<&str>,
) -> Result<Option<memory_server_client::TaskResultBriefDto>, String> {
    let result_summary = result_summary.trim();
    if result_summary.is_empty() {
        return Ok(None);
    }

    let item = memory_server_client::upsert_task_result_brief(
        &memory_server_client::UpsertTaskResultBriefRequestDto {
            task_id: task.id.clone(),
            user_id: scope.user_id.clone(),
            contact_agent_id: scope.contact_agent_id.clone(),
            project_id: scope.project_id.clone(),
            source_session_id: task.session_id.clone(),
            source_turn_id: task.conversation_turn_id.clone(),
            task_title: task.title.clone(),
            task_status: task_status.trim().to_string(),
            result_summary: compact_result_summary(result_summary),
            result_format: task
                .execution_result_contract
                .as_ref()
                .and_then(|item| item.preferred_format.clone()),
            result_message_id: result_message_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string()),
            finished_at: task.finished_at.clone(),
        },
    )
    .await?;
    Ok(Some(item))
}

pub(super) fn compact_result_summary(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= 500 {
        return trimmed.to_string();
    }
    let compact: String = trimmed.chars().take(500).collect();
    format!("{}...", compact)
}
