use serde_json::Value;

use crate::core::mcp_tools::ToolResult;
use crate::models::message::Message;
use crate::models::session_summary::SessionSummary;
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

    pub async fn get_session_history_with_summaries(
        &self,
        session_id: &str,
        limit: Option<i64>,
        summary_limit: Option<i64>,
    ) -> (Vec<SessionSummary>, Vec<Message>) {
        self.core
            .get_session_history_with_summaries(session_id, limit, summary_limit, true)
            .await
    }

    pub async fn get_chat_history_context(
        &self,
        session_id: &str,
        summary_limit: usize,
    ) -> (Option<String>, usize, Vec<Message>) {
        let context = self
            .core
            .get_chat_history_context(session_id, summary_limit)
            .await;
        (
            context.merged_summary,
            context.summary_count,
            context.messages,
        )
    }

    pub async fn get_last_response_id(&self, session_id: &str, limit: i64) -> Option<String> {
        let summary_limit = Some(2);
        let (_summaries, messages) = self
            .get_session_history_with_summaries(session_id, Some(limit), summary_limit)
            .await;

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
                    continue;
                }
            }

            if let Some(metadata) = &message.metadata {
                if let Some(response_id) =
                    metadata.get("response_id").and_then(|value| value.as_str())
                {
                    return Some(response_id.to_string());
                }
                if let Some(response_id) =
                    metadata.get("responseId").and_then(|value| value.as_str())
                {
                    return Some(response_id.to_string());
                }
            }
        }

        None
    }
}
