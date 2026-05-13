use serde_json::Value;
use tracing::info;

use crate::core::messages::{
    assistant_message_has_reusable_payload, assistant_message_response_id_candidate,
};
use crate::core::mcp_tools::ToolResult;
use crate::models::message::Message;
use crate::models::session_summary_v2::SessionSummaryV2;
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

    pub async fn save_assistant_response_message(
        &self,
        session_id: &str,
        content: &str,
        reasoning: Option<String>,
        message_mode: Option<String>,
        message_source: Option<String>,
        tool_calls: Option<Value>,
        response_id: Option<&str>,
        turn_id: Option<&str>,
        response_status: Option<&str>,
    ) -> Result<Message, String> {
        self.core
            .save_assistant_response_message(
                session_id,
                content,
                reasoning,
                message_mode,
                message_source,
                tool_calls,
                response_id,
                turn_id,
                response_status,
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
    ) -> (Option<String>, usize, Vec<Message>) {
        let context = self
            .core
            .get_memory_chat_history_context(session_id)
            .await;
        (
            context.merged_summary,
            context.summary_count,
            context.messages,
        )
    }

    pub async fn get_last_response_id(&self, session_id: &str, limit: i64) -> Option<String> {
        let memory_summary_limit = Some(2);
        let (_summaries, messages) = self
            .get_session_memory_history(session_id, Some(limit), memory_summary_limit)
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

            if !assistant_message_has_reusable_payload(message) {
                info!(
                    "[AI_V3][prev-id] skip assistant without reusable payload: session_id={}, message_id={}",
                    session_id,
                    message.id
                );
                continue;
            }

            if let Some(response_id) = assistant_message_response_id_candidate(message) {
                info!(
                    "[AI_V3][prev-id] hit metadata response_id alias: session_id={}, message_id={}, response_id={}",
                    session_id,
                    message.id,
                    response_id
                );
                return Some(response_id.to_string());
            }
        }

        info!(
            "[AI_V3][prev-id] miss: session_id={}, no reusable response_id found",
            session_id
        );
        None
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::assistant_message_response_id_candidate;
    use crate::models::message::Message;

    #[test]
    fn response_id_candidate_accepts_terminal_assistant_with_reusable_payload() {
        let mut message = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "final answer".to_string(),
        );
        message.metadata = Some(json!({
            "response_id": "resp_ok",
            "response_status": "completed",
        }));

        assert_eq!(
            assistant_message_response_id_candidate(&message),
            Some("resp_ok")
        );
    }

    #[test]
    fn response_id_candidate_rejects_tool_calls_non_terminal_and_empty_payloads() {
        let mut tool_call_message = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "final answer".to_string(),
        );
        tool_call_message.tool_calls = Some(json!([{
            "id": "call_1",
            "type": "function",
            "function": {"name": "demo", "arguments": "{}"}
        }]));
        tool_call_message.metadata = Some(json!({
            "response_id": "resp_tool",
            "response_status": "completed",
        }));

        let mut non_terminal = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "working".to_string(),
        );
        non_terminal.metadata = Some(json!({
            "response_id": "resp_pending",
            "response_status": "in_progress",
        }));

        let mut empty_payload = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "   ".to_string(),
        );
        empty_payload.reasoning = Some("   ".to_string());
        empty_payload.metadata = Some(json!({
            "response_id": "resp_empty",
            "response_status": "completed",
        }));

        assert_eq!(assistant_message_response_id_candidate(&tool_call_message), None);
        assert_eq!(assistant_message_response_id_candidate(&non_terminal), None);
        assert_eq!(assistant_message_response_id_candidate(&empty_payload), None);
    }

    #[test]
    fn response_id_candidate_accepts_reasoning_only_payload() {
        let mut message = Message::new(
            "session_1".to_string(),
            "assistant".to_string(),
            "".to_string(),
        );
        message.reasoning = Some("thinking".to_string());
        message.metadata = Some(json!({
            "response_id": "resp_reasoning",
            "response_status": "completed",
        }));

        assert_eq!(
            assistant_message_response_id_candidate(&message),
            Some("resp_reasoning")
        );
    }
}
