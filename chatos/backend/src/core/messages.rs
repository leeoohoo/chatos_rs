// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use serde_json::Value;

use crate::core::tool_call::extract_message_tool_calls;
use crate::models::message::Message;
use crate::services::chatos_sessions;
use crate::services::session_title::maybe_rename_session_title;

#[derive(Debug, Clone, Default)]
pub struct NewMessageFields {
    pub role: Option<String>,
    pub content: Option<String>,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct MessageOut {
    pub id: String,
    pub conversation_id: String,
    #[serde(rename = "conversationId")]
    pub conversation_id_camel: String,
    pub role: String,
    pub content: String,
    pub message_mode: Option<String>,
    pub message_source: Option<String>,
    pub summary: Option<String>,
    #[serde(rename = "toolCalls")]
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub reasoning: Option<String>,
    pub metadata: Option<Value>,
    pub summary_status: String,
    pub summary_id: Option<String>,
    pub summarized_at: Option<String>,
    pub created_at: String,
}

impl From<Message> for MessageOut {
    fn from(msg: Message) -> Self {
        let content = message_content_for_display(&msg);
        MessageOut {
            id: msg.id,
            conversation_id: msg.session_id.clone(),
            conversation_id_camel: msg.session_id,
            role: msg.role,
            content,
            message_mode: msg.message_mode,
            message_source: msg.message_source,
            summary: msg.summary,
            tool_calls: msg.tool_calls,
            tool_call_id: msg.tool_call_id,
            reasoning: msg.reasoning,
            metadata: msg.metadata,
            summary_status: msg.summary_status,
            summary_id: msg.summary_id,
            summarized_at: msg.summarized_at,
            created_at: msg.created_at,
        }
    }
}

fn message_content_for_display(message: &Message) -> String {
    if message.role.trim() != "user" {
        return message.content.clone();
    }

    let execution_metadata = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("project_requirement_execution"));
    if let Some(content) = execution_metadata
        .and_then(|metadata| metadata.get("user_visible_content"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|content| !content.is_empty())
    {
        return content.to_string();
    }

    let leaked_internal_prompt =
        looks_like_requirement_execution_internal_prompt(message.content.as_str());
    let task_runner_source = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("task_runner_async"))
        .and_then(|metadata| metadata.get("source"))
        .and_then(Value::as_str);
    let is_requirement_execution = execution_metadata.is_some()
        || message.message_mode.as_deref() == Some("project_requirement_execution")
        || task_runner_source == Some("project_requirement_execute_button")
        || leaked_internal_prompt;
    if !is_requirement_execution {
        return message.content.clone();
    }

    let prompt_payload = leaked_internal_prompt
        .then(|| parse_requirement_execution_prompt_payload(message.content.as_str()))
        .flatten();
    let requirement_title = execution_metadata
        .and_then(|metadata| metadata.get("requirement_title"))
        .and_then(Value::as_str)
        .or_else(|| {
            prompt_payload
                .as_ref()
                .and_then(|payload| payload.get("requirement"))
                .and_then(|requirement| requirement.get("title"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|title| !title.is_empty());
    let task_count = execution_metadata
        .and_then(|metadata| metadata.get("project_task_ids"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .or_else(|| {
            prompt_payload
                .as_ref()
                .and_then(|payload| payload.get("selected_project_tasks"))
                .and_then(Value::as_array)
                .map(Vec::len)
        });

    match (requirement_title, task_count) {
        (Some(title), Some(count)) => format!("执行需求「{title}」的 {count} 个关联任务。"),
        (Some(title), None) => format!("执行需求「{title}」的关联任务。"),
        (None, Some(count)) => format!("执行 {count} 个关联任务。"),
        (None, None) => "执行所选关联任务。".to_string(),
    }
}

fn looks_like_requirement_execution_internal_prompt(content: &str) -> bool {
    content.contains("project_requirement_execution_planning")
}

fn parse_requirement_execution_prompt_payload(content: &str) -> Option<Value> {
    let json_start = content.find('{')?;
    serde_json::from_str(&content[json_start..]).ok()
}

pub fn build_message(session_id: String, fields: NewMessageFields, default_role: &str) -> Message {
    let role = fields.role.unwrap_or_else(|| default_role.to_string());
    let content = fields.content.unwrap_or_default();

    let mut message = Message::new(session_id, role, content);
    message.message_mode = fields.message_mode;
    message.message_source = fields.message_source;
    message.tool_calls = fields.tool_calls;
    message.tool_call_id = fields.tool_call_id;
    message.reasoning = fields.reasoning;
    message.metadata = fields.metadata;
    message
}

pub fn ensure_message_metadata_object(
    message: &mut Message,
) -> &mut serde_json::Map<String, Value> {
    if !matches!(message.metadata, Some(Value::Object(_))) {
        message.metadata = Some(Value::Object(serde_json::Map::new()));
    }

    match message.metadata {
        Some(Value::Object(ref mut map)) => map,
        _ => unreachable!("metadata should be object"),
    }
}

pub async fn set_task_runner_async_overall_status_for_session(
    session_id: &str,
    message_id: &str,
    overall_status: &str,
) -> Result<Option<Message>, String> {
    let normalized_session_id = session_id.trim();
    let normalized_message_id = message_id.trim();
    if normalized_session_id.is_empty() || normalized_message_id.is_empty() {
        return Ok(None);
    }

    let Some(session) = chatos_sessions::get_session_by_id(normalized_session_id).await? else {
        return Ok(None);
    };
    let Some(mut message) = chatos_sessions::get_message_by_id_in_session_including_hidden(
        &session,
        normalized_message_id,
    )
    .await?
    else {
        return Ok(None);
    };
    apply_task_runner_async_overall_status(&mut message, overall_status);
    let saved = chatos_sessions::upsert_message_in_session(&session, &message).await?;
    Ok(Some(saved))
}

pub async fn set_task_runner_async_execution_paused_for_session(
    session_id: &str,
    message_id: &str,
    paused: bool,
) -> Result<Option<Message>, String> {
    let normalized_session_id = session_id.trim();
    let normalized_message_id = message_id.trim();
    if normalized_session_id.is_empty() || normalized_message_id.is_empty() {
        return Ok(None);
    }
    let Some(session) = chatos_sessions::get_session_by_id(normalized_session_id).await? else {
        return Ok(None);
    };
    let Some(mut message) = chatos_sessions::get_message_by_id_in_session_including_hidden(
        &session,
        normalized_message_id,
    )
    .await?
    else {
        return Ok(None);
    };
    let metadata = ensure_message_metadata_object(&mut message);
    let task_runner_async = metadata
        .entry("task_runner_async".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !task_runner_async.is_object() {
        *task_runner_async = Value::Object(serde_json::Map::new());
    }
    if let Value::Object(task_runner_async) = task_runner_async {
        task_runner_async.insert("execution_paused".to_string(), Value::Bool(paused));
        task_runner_async.insert(
            "overall_status".to_string(),
            Value::String(if paused { "paused" } else { "processing" }.to_string()),
        );
    }
    let saved = chatos_sessions::upsert_message_in_session(&session, &message).await?;
    Ok(Some(saved))
}

fn apply_task_runner_async_overall_status(message: &mut Message, overall_status: &str) {
    let metadata = ensure_message_metadata_object(message);
    let task_runner_async = metadata
        .entry("task_runner_async".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !task_runner_async.is_object() {
        *task_runner_async = Value::Object(serde_json::Map::new());
    }
    if let Value::Object(task_runner_async_map) = task_runner_async {
        let requested_overall_status = if overall_status.trim().eq_ignore_ascii_case("completed")
            && task_runner_async_has_pending_created_tasks(task_runner_async_map)
        {
            "processing"
        } else {
            overall_status
        };
        let current_overall_status = task_runner_async_map
            .get("overall_status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if task_runner_async_status_is_stop_locked(current_overall_status)
            && !task_runner_async_status_is_stop_locked(requested_overall_status)
        {
            return;
        }
        if task_runner_async_callback_is_terminal(task_runner_async_map)
            && !current_overall_status.eq_ignore_ascii_case(requested_overall_status)
        {
            return;
        }
        task_runner_async_map
            .entry("mode".to_string())
            .or_insert_with(|| Value::String("contact_async".to_string()));
        task_runner_async_map.insert(
            "overall_status".to_string(),
            Value::String(requested_overall_status.to_string()),
        );
        if matches!(
            requested_overall_status
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "failed" | "error" | "stopping" | "stopped" | "cancelled" | "canceled"
        ) {
            task_runner_async_map.insert(
                "confirmation_status".to_string(),
                Value::String(requested_overall_status.to_string()),
            );
        }
    }
}

fn task_runner_async_has_pending_created_tasks(
    task_runner_async: &serde_json::Map<String, Value>,
) -> bool {
    let created = task_runner_async
        .get("created_task_ids")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    if created.is_empty() {
        return false;
    }
    let terminal = task_runner_async
        .get("terminal_task_ids")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    created.iter().any(|task_id| !terminal.contains(task_id))
}

fn task_runner_async_callback_is_terminal(
    task_runner_async: &serde_json::Map<String, Value>,
) -> bool {
    let created_count = task_runner_async
        .get("created_task_ids")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    created_count > 0 && !task_runner_async_has_pending_created_tasks(task_runner_async)
}

fn task_runner_async_status_is_stop_locked(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "stopping" | "stopped" | "cancelled" | "canceled"
    )
}

pub fn text_has_content(value: &str) -> bool {
    !value.trim().is_empty()
}

#[cfg(test)]
pub fn optional_text_has_content(value: Option<&str>) -> bool {
    value.map(text_has_content).unwrap_or(false)
}

#[cfg(test)]
pub fn owned_non_empty_text(value: &str) -> Option<String> {
    text_has_content(value).then(|| value.to_string())
}

pub fn message_has_text_content(message: &Message) -> bool {
    text_has_content(&message.content)
}

pub fn is_session_summary_message(message: &Message) -> bool {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("type"))
        .and_then(|value| value.as_str())
        == Some("session_summary")
}

pub fn is_runtime_guidance_message(message: &Message) -> bool {
    message
        .message_mode
        .as_deref()
        .map(str::trim)
        .is_some_and(|mode| mode == "runtime_guidance")
        || message
            .message_source
            .as_deref()
            .map(str::trim)
            .is_some_and(|source| source == "runtime_guidance")
        || message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("runtime_guidance"))
            .is_some()
}

pub fn is_runtime_guidance_user_message(message: &Message) -> bool {
    message.role == "user" && is_runtime_guidance_message(message)
}

pub fn message_is_hidden(message: &Message) -> bool {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("hidden"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub fn extract_message_tool_calls_for_display(message: &Message) -> Vec<Value> {
    extract_message_tool_calls(message.tool_calls.as_ref(), message.metadata.as_ref())
}

#[cfg(test)]
pub fn select_preferred_text<'a>(content: &'a str, reasoning: Option<&'a str>) -> Option<&'a str> {
    chatos_ai_runtime::select_preferred_response_text(content, reasoning)
}

pub async fn create_message_and_maybe_rename(message: Message) -> Result<Message, String> {
    let session_id = message.session_id.clone();
    let role = message.role.clone();
    let content = message.content.clone();

    let saved = chatos_sessions::upsert_message(&message).await?;
    if role == "user" {
        let _ = maybe_rename_session_title(&session_id, &content, 30).await;
    }
    Ok(saved)
}

pub fn object_string_alias<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    let object = value.as_object()?;
    keys.iter()
        .find_map(|key| object.get(*key))
        .and_then(Value::as_str)
}

pub fn message_metadata_string_alias<'a>(message: &'a Message, keys: &[&str]) -> Option<&'a str> {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| object_string_alias(metadata, keys))
}

pub fn message_turn_id(message: &Message) -> Option<&str> {
    message_metadata_string_alias(message, &["conversation_turn_id", "conversationTurnId"])
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        apply_task_runner_async_overall_status, ensure_message_metadata_object,
        extract_message_tool_calls_for_display, is_session_summary_message,
        message_metadata_string_alias, message_turn_id, object_string_alias,
        optional_text_has_content, owned_non_empty_text, select_preferred_text, text_has_content,
        MessageOut,
    };
    use crate::models::message::Message;

    #[test]
    fn requirement_execution_message_uses_persisted_user_visible_content() {
        let mut message = Message::new(
            "session_1".to_string(),
            "user".to_string(),
            "internal execution_contract payload".to_string(),
        );
        message.metadata = Some(json!({
            "project_requirement_execution": {
                "requirement_title": "JDK 21 upgrade",
                "project_task_ids": ["task-1", "task-2"],
                "user_visible_content": "执行需求「JDK 21 upgrade」的 2 个关联任务。"
            }
        }));

        let output = MessageOut::from(message);

        assert_eq!(
            output.content,
            "执行需求「JDK 21 upgrade」的 2 个关联任务。"
        );
        assert!(!output.content.contains("execution_contract"));
    }

    #[test]
    fn legacy_requirement_execution_prompt_is_summarized_without_metadata() {
        let message = Message::new(
            "session_1".to_string(),
            "user".to_string(),
            concat!(
                "这是用户点击‘执行关联任务’产生的强制执行请求。\n\n",
                "{\n",
                "  \"mode\": \"project_requirement_execution_planning\",\n",
                "  \"requirement\": {\"title\": \"JDK 21 upgrade\"},\n",
                "  \"selected_project_tasks\": [{\"id\": \"task-1\"}, {\"id\": \"task-2\"}]\n",
                "}"
            )
            .to_string(),
        );

        let output = MessageOut::from(message);

        assert_eq!(
            output.content,
            "执行需求「JDK 21 upgrade」的 2 个关联任务。"
        );
        assert!(!output
            .content
            .contains("project_requirement_execution_planning"));
    }

    #[test]
    fn detects_non_empty_text_content() {
        assert!(text_has_content(" hello "));
        assert!(!text_has_content("   "));
        assert!(optional_text_has_content(Some("world")));
        assert!(!optional_text_has_content(Some("\n\t")));
        assert!(!optional_text_has_content(None));
        assert_eq!(owned_non_empty_text(" hello "), Some(" hello ".to_string()));
        assert_eq!(owned_non_empty_text("   "), None);
    }

    #[test]
    fn identifies_session_summary_messages() {
        let mut summary = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "summary".to_string(),
        );
        summary.metadata = Some(json!({"type": "session_summary"}));

        let normal = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "reply".to_string(),
        );

        assert!(is_session_summary_message(&summary));
        assert!(!is_session_summary_message(&normal));
    }

    #[test]
    fn selects_content_then_reasoning_text() {
        assert_eq!(
            select_preferred_text("hello", Some("thinking")),
            Some("hello")
        );
        assert_eq!(
            select_preferred_text("   ", Some("thinking")),
            Some("thinking")
        );
        assert_eq!(select_preferred_text("   ", Some("   ")), None);
        assert_eq!(select_preferred_text("", None), None);
    }

    #[test]
    fn ensures_message_metadata_object_and_preserves_existing_map() {
        let mut message = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "".to_string(),
        );
        ensure_message_metadata_object(&mut message).insert(
            "conversation_turn_id".to_string(),
            Value::String("turn_1".to_string()),
        );

        let metadata = ensure_message_metadata_object(&mut message);
        assert_eq!(
            metadata.get("conversation_turn_id").and_then(Value::as_str),
            Some("turn_1")
        );
    }

    #[test]
    fn resolves_object_and_message_metadata_aliases() {
        let metadata = json!({
            "responseId": "resp_1",
            "conversationTurnId": "turn_1"
        });
        assert_eq!(
            object_string_alias(&metadata, &["response_id", "responseId"]),
            Some("resp_1")
        );

        let mut message = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "".to_string(),
        );
        message.metadata = Some(metadata);
        assert_eq!(
            message_metadata_string_alias(&message, &["response_id", "responseId"]),
            Some("resp_1")
        );
        assert_eq!(message_turn_id(&message), Some("turn_1"));
    }

    #[test]
    fn extracts_message_tool_calls_from_message_or_metadata() {
        let mut message = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "tool".to_string(),
        );
        message.metadata = Some(json!({
            "toolCalls": [{"id":"call_1"}]
        }));

        let calls = extract_message_tool_calls_for_display(&message);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].get("id").and_then(Value::as_str), Some("call_1"));
    }

    #[test]
    fn terminal_planning_status_updates_confirmation_without_clobbering_mode() {
        let mut message = Message::new(
            "session_1".to_string(),
            "user".to_string(),
            "plan".to_string(),
        );
        message.metadata = Some(json!({
            "task_runner_async": {
                "mode": "project_requirement_execution",
                "overall_status": "awaiting_confirmation",
                "confirmation_status": "awaiting_confirmation"
            }
        }));

        apply_task_runner_async_overall_status(&mut message, "stopped");

        let task_runner = message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("task_runner_async"))
            .expect("task runner metadata");
        assert_eq!(
            task_runner.get("mode").and_then(Value::as_str),
            Some("project_requirement_execution")
        );
        assert_eq!(
            task_runner
                .get("confirmation_status")
                .and_then(Value::as_str),
            Some("stopped")
        );
    }

    #[test]
    fn stopped_task_runner_async_status_is_not_overwritten_by_late_failure() {
        let mut message = Message::new(
            "session_1".to_string(),
            "user".to_string(),
            "plan".to_string(),
        );
        message.metadata = Some(json!({
            "task_runner_async": {
                "mode": "project_requirement_execution",
                "overall_status": "stopped",
                "confirmation_status": "stopped"
            }
        }));

        apply_task_runner_async_overall_status(&mut message, "failed");

        let task_runner = message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("task_runner_async"))
            .expect("task runner metadata");
        assert_eq!(
            task_runner.get("overall_status").and_then(Value::as_str),
            Some("stopped")
        );
        assert_eq!(
            task_runner
                .get("confirmation_status")
                .and_then(Value::as_str),
            Some("stopped")
        );
    }

    #[test]
    fn planner_completion_keeps_message_processing_while_created_tasks_are_active() {
        let mut message = Message::new(
            "session_1".to_string(),
            "user".to_string(),
            "plan".to_string(),
        );
        message.metadata = Some(json!({
            "task_runner_async": {
                "overall_status": "processing",
                "created_task_ids": ["task-1", "task-2"],
                "terminal_task_ids": ["task-1"]
            }
        }));

        apply_task_runner_async_overall_status(&mut message, "completed");

        assert_eq!(
            message.metadata.as_ref().unwrap()["task_runner_async"]["overall_status"],
            "processing"
        );
    }

    #[test]
    fn planner_finalize_does_not_overwrite_terminal_callback_failure() {
        let mut message = Message::new(
            "session_1".to_string(),
            "user".to_string(),
            "plan".to_string(),
        );
        message.metadata = Some(json!({
            "task_runner_async": {
                "overall_status": "failed",
                "created_task_ids": ["task-1"],
                "terminal_task_ids": ["task-1"],
                "failed_task_ids": ["task-1"]
            }
        }));

        apply_task_runner_async_overall_status(&mut message, "completed");

        assert_eq!(
            message.metadata.as_ref().unwrap()["task_runner_async"]["overall_status"],
            "failed"
        );
    }
}
