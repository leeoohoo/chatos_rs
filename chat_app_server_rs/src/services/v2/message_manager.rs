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
        metadata: Option<Value>,
    ) -> Result<Message, String> {
        self.core
            .save_user_message(session_id, content, message_id, metadata)
            .await
    }

    pub async fn save_assistant_message(
        &self,
        session_id: &str,
        content: &str,
        summary: Option<String>,
        reasoning: Option<String>,
        metadata: Option<Value>,
        tool_calls: Option<Value>,
    ) -> Result<Message, String> {
        self.core
            .save_assistant_message(
                session_id, content, summary, reasoning, metadata, tool_calls,
            )
            .await
    }

    pub async fn save_tool_message(
        &self,
        session_id: &str,
        content: &str,
        tool_call_id: &str,
        metadata: Option<Value>,
    ) -> Result<Message, String> {
        self.core
            .save_tool_message(session_id, content, tool_call_id, metadata)
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
            .get_session_history_with_summaries(session_id, limit, summary_limit, false)
            .await
    }

    pub fn get_session_messages_sync(&self, session_id: &str, limit: Option<i64>) -> Vec<Message> {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            return tokio::task::block_in_place(|| {
                handle.block_on(self.get_session_messages(session_id, limit))
            });
        }

        let runtime = tokio::runtime::Runtime::new();
        if let Ok(runtime) = runtime {
            return runtime.block_on(self.get_session_messages(session_id, limit));
        }

        Vec::new()
    }

    pub async fn get_message_by_id(&self, message_id: &str) -> Option<Message> {
        self.core.get_message_by_id(message_id).await
    }

    pub fn process_pending_saves(&self) -> Result<usize, String> {
        self.core.process_pending_saves()
    }

    pub fn get_stats(&self) -> Value {
        self.core.get_stats()
    }

    pub fn get_cache_stats(&self) -> Value {
        self.core.get_cache_stats()
    }
}
