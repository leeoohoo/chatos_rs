use serde_json::Value;
use tracing::info;

use crate::core::mcp_tools::ToolResult;
use crate::models::message::Message;
use crate::models::session_summary_v2::SessionSummaryV2;
use crate::services::memory_server_client::TaskExecutionScopeBinding;
use crate::services::message_manager_common::MessageManagerCore;

#[derive(Clone)]
pub struct MessageManager {
    core: MessageManagerCore,
}

impl MessageManager {
    pub fn new() -> Self {
        Self {
            core: MessageManagerCore::new(),
        }
    }

    pub fn new_task_execution(scope: TaskExecutionScopeBinding) -> Self {
        Self {
            core: MessageManagerCore::new_task_execution(scope),
        }
    }

    pub async fn save_user_message(
        &self,
        session_id: &str,
        content: &str,
        message_id: Option<String>,
        message_mode: Option<String>,
        message_source: Option<String>,
        metadata: Option<Value>,
    ) -> Result<Message, String> {
        self.core
            .save_user_message(
                session_id,
                content,
                message_id,
                message_mode,
                message_source,
                metadata,
            )
            .await
    }

    pub async fn save_assistant_message(
        &self,
        session_id: &str,
        content: &str,
        summary: Option<String>,
        reasoning: Option<String>,
        message_mode: Option<String>,
        message_source: Option<String>,
        metadata: Option<Value>,
        tool_calls: Option<Value>,
    ) -> Result<Message, String> {
        self.core
            .save_assistant_message(
                session_id,
                content,
                summary,
                reasoning,
                message_mode,
                message_source,
                metadata,
                tool_calls,
            )
            .await
    }

    pub async fn save_tool_message(
        &self,
        session_id: &str,
        content: &str,
        tool_call_id: &str,
        message_mode: Option<String>,
        message_source: Option<String>,
        metadata: Option<Value>,
    ) -> Result<Message, String> {
        self.core
            .save_tool_message(
                session_id,
                content,
                tool_call_id,
                message_mode,
                message_source,
                metadata,
            )
            .await
    }

    pub async fn save_tool_results(&self, session_id: &str, results: &[ToolResult]) {
        self.core.save_tool_results(session_id, results).await;
    }

    pub async fn get_session_messages(&self, session_id: &str, limit: Option<i64>) -> Vec<Message> {
        self.core.get_session_messages(session_id, limit).await
    }

    pub async fn get_session_memory_history(
        &self,
        session_id: &str,
        limit: Option<i64>,
        memory_summary_limit: Option<i64>,
    ) -> (Vec<SessionSummaryV2>, Vec<Message>) {
        self.core
            .get_session_memory_history(session_id, limit, memory_summary_limit, true)
            .await
    }

    pub async fn get_memory_chat_history_context(
        &self,
        session_id: &str,
        memory_summary_limit: usize,
    ) -> (Option<String>, usize, Vec<Message>) {
        let context = self
            .core
            .get_memory_chat_history_context(session_id, memory_summary_limit)
            .await;
        (
            context.merged_summary,
            context.summary_count,
            context.messages,
        )
    }

    pub async fn get_last_response_id(&self, session_id: &str, limit: i64) -> Option<String> {
        let memory_summary_limit = Some(2);
        let messages = self
            .core
            .get_history_messages_after_summary(session_id, Some(limit), memory_summary_limit)
            .await;
        info!(
            "[AI_V3][prev-id] scan start: session_id={}, limit={}, message_count={}",
            session_id,
            limit,
            messages.len()
        );

        for message in messages.iter().rev() {
            if message.role != "assistant" {
                continue;
            }

            let mut tool_calls = message.tool_calls.clone().or_else(|| {
                message
                    .metadata
                    .clone()
                    .and_then(|meta| meta.get("toolCalls").cloned())
            });
            if let Some(Value::String(raw)) = tool_calls.clone() {
                if let Ok(parsed) = serde_json::from_str::<Value>(&raw) {
                    tool_calls = Some(parsed);
                }
            }

            if let Some(tool_calls) = tool_calls {
                if tool_calls
                    .as_array()
                    .map(|array| !array.is_empty())
                    .unwrap_or(false)
                {
                    info!(
                        "[AI_V3][prev-id] skip assistant with tool_calls: session_id={}, message_id={}",
                        session_id,
                        message.id
                    );
                    continue;
                }
            }

            let response_status = message
                .metadata
                .as_ref()
                .and_then(extract_response_status_from_metadata);
            if is_non_terminal_response_status(response_status) {
                info!(
                    "[AI_V3][prev-id] skip assistant with non-terminal response status: session_id={}, message_id={}, status={}",
                    session_id,
                    message.id,
                    response_status.unwrap_or("unknown")
                );
                continue;
            }

            let has_content = !message.content.trim().is_empty();
            let has_reasoning = message
                .reasoning
                .as_deref()
                .map(str::trim)
                .map(|value| !value.is_empty())
                .unwrap_or(false);
            if !has_content && !has_reasoning {
                info!(
                    "[AI_V3][prev-id] skip assistant without reusable payload: session_id={}, message_id={}",
                    session_id,
                    message.id
                );
                continue;
            }

            if let Some(metadata) = &message.metadata {
                if let Some(response_id) =
                    metadata.get("response_id").and_then(|value| value.as_str())
                {
                    info!(
                        "[AI_V3][prev-id] hit metadata.response_id: session_id={}, message_id={}, response_id={}",
                        session_id,
                        message.id,
                        response_id
                    );
                    return Some(response_id.to_string());
                }
                if let Some(response_id) =
                    metadata.get("responseId").and_then(|value| value.as_str())
                {
                    info!(
                        "[AI_V3][prev-id] hit metadata.responseId: session_id={}, message_id={}, response_id={}",
                        session_id,
                        message.id,
                        response_id
                    );
                    return Some(response_id.to_string());
                }
            }
        }

        info!(
            "[AI_V3][prev-id] miss: session_id={}, no reusable response_id found",
            session_id
        );
        None
    }
}

fn extract_response_status_from_metadata(metadata: &Value) -> Option<&str> {
    metadata
        .get("response_status")
        .or_else(|| metadata.get("responseStatus"))
        .or_else(|| metadata.get("finish_reason"))
        .or_else(|| metadata.get("finishReason"))
        .or_else(|| metadata.get("status"))
        .and_then(|value| value.as_str())
}

fn is_non_terminal_response_status(status: Option<&str>) -> bool {
    let normalized = status
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    matches!(
        normalized.as_deref(),
        Some("in_progress") | Some("queued") | Some("pending") | Some("incomplete")
    )
}
