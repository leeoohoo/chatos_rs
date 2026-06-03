use serde_json::Value;
use tracing::error;

use crate::core::mcp_tools::ToolResult;
use crate::models::message::Message;
use crate::models::session::Session;
use crate::models::session_summary_v2::SessionSummaryV2;
use crate::services::ai_common::{build_assistant_message_metadata, build_tool_result_metadata};
use crate::services::{chatos_memory_engine, chatos_sessions};

#[derive(Clone, Default)]
pub(crate) struct MessageManagerCore;

#[derive(Debug, Clone, Default)]
pub(crate) struct ChatHistoryContext {
    pub merged_summary: Option<String>,
    pub summary_count: usize,
    pub messages: Vec<Message>,
}

impl MessageManagerCore {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn save_user_message(
        &self,
        session_id: &str,
        content: &str,
        message_id: Option<String>,
        message_mode: Option<String>,
        message_source: Option<String>,
        metadata: Option<Value>,
    ) -> Result<Message, String> {
        let mut message = Message::new(
            session_id.to_string(),
            "user".to_string(),
            content.to_string(),
        );
        if let Some(id) = message_id {
            message.id = id;
        }
        message.message_mode = message_mode;
        message.message_source = message_source;
        message.metadata = metadata;
        self.persist_message(message).await
    }

    pub(crate) async fn save_assistant_message(
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
        let mut message = Message::new(
            session_id.to_string(),
            "assistant".to_string(),
            content.to_string(),
        );
        message.summary = summary;
        message.reasoning = reasoning;
        message.message_mode = message_mode;
        message.message_source = message_source;
        message.metadata = metadata;
        message.tool_calls = tool_calls;
        self.persist_message(message).await
    }

    pub(crate) async fn save_assistant_response_message(
        &self,
        session_id: &str,
        content: &str,
        reasoning: Option<String>,
        message_mode: Option<String>,
        message_source: Option<String>,
        metadata: Option<Value>,
        tool_calls: Option<Value>,
        response_id: Option<&str>,
        turn_id: Option<&str>,
        response_status: Option<&str>,
    ) -> Result<Message, String> {
        let metadata = build_assistant_message_metadata(
            tool_calls.as_ref(),
            response_id,
            turn_id,
            response_status,
            metadata.as_ref(),
        );
        self.save_assistant_message(
            session_id,
            content,
            None,
            reasoning,
            message_mode,
            message_source,
            metadata,
            tool_calls,
        )
        .await
    }

    pub(crate) async fn save_tool_message(
        &self,
        session_id: &str,
        content: &str,
        tool_call_id: &str,
        message_mode: Option<String>,
        message_source: Option<String>,
        metadata: Option<Value>,
    ) -> Result<Message, String> {
        let mut message = Message::new(
            session_id.to_string(),
            "tool".to_string(),
            content.to_string(),
        );
        message.tool_call_id = Some(tool_call_id.to_string());
        message.message_mode = message_mode;
        message.message_source = message_source;
        message.metadata = metadata;
        self.persist_message(message).await
    }

    pub(crate) async fn save_tool_results(&self, session_id: &str, results: &[ToolResult]) {
        for result in results {
            let metadata = build_tool_result_metadata(result);
            let _ = self
                .save_tool_message(
                    session_id,
                    &result.content,
                    &result.tool_call_id,
                    None,
                    None,
                    Some(metadata),
                )
                .await;
        }
    }

    async fn persist_message(&self, message: Message) -> Result<Message, String> {
        chatos_sessions::upsert_message(&message).await
    }

    pub(crate) async fn get_session_messages(
        &self,
        session_id: &str,
        limit: Option<i64>,
    ) -> Vec<Message> {
        let result = if let Some(value) = limit {
            chatos_sessions::list_messages(session_id, Some(value), 0, false)
                .await
                .map(|mut items| {
                    items.reverse();
                    items
                })
        } else {
            chatos_sessions::list_messages(session_id, None, 0, true).await
        };

        match result {
            Ok(messages) => messages,
            Err(err) => {
                error!("get_session_messages failed: {}", err);
                Vec::new()
            }
        }
    }

    pub(crate) async fn get_session_memory_history(
        &self,
        session_id: &str,
        limit: Option<i64>,
        memory_summary_limit: Option<i64>,
        filter_empty_summaries: bool,
    ) -> (Vec<SessionSummaryV2>, Vec<Message>) {
        let mut summaries =
            match chatos_sessions::list_summaries(session_id, memory_summary_limit, 0).await {
                Ok(items) => items,
                Err(err) => {
                    error!("list_summaries from chatos session store failed: {}", err);
                    Vec::new()
                }
            };

        if filter_empty_summaries {
            summaries.retain(|summary| !summary.summary_text.trim().is_empty());
        }

        if summaries.is_empty() {
            let messages = self.get_session_messages(session_id, limit).await;
            return (Vec::new(), messages);
        }

        let mut messages = match chatos_sessions::list_messages(session_id, None, 0, true).await {
            Ok(items) => items,
            Err(err) => {
                error!("get_session_memory_history list_messages failed: {}", err);
                Vec::new()
            }
        };

        if let Some(last_message_id) = summaries
            .last()
            .and_then(|summary| summary.source_end_message_id.clone())
        {
            if let Some(last_idx) = messages
                .iter()
                .position(|message| message.id == last_message_id)
            {
                messages = messages.into_iter().skip(last_idx + 1).collect();
            }
        }

        if let Some(v) = limit {
            if v > 0 && messages.len() > v as usize {
                messages = messages[messages.len() - v as usize..].to_vec();
            }
        }
        (summaries, messages)
    }

    pub(crate) async fn get_memory_chat_history_context(
        &self,
        session_id: &str,
    ) -> ChatHistoryContext {
        match try_get_memory_chat_history_context_from_memory_engine(session_id).await {
            Ok(context) => context,
            Err(err) => {
                error!(
                    "get_memory_chat_history_context memory_engine failed: session_id={} error={}",
                    session_id, err
                );
                panic!(
                    "memory engine context unavailable for session {}: {}",
                    session_id, err
                );
            }
        }
    }
}

async fn try_get_memory_chat_history_context_from_memory_engine(
    session_id: &str,
) -> Result<ChatHistoryContext, String> {
    let session = chatos_sessions::get_session_by_id(session_id)
        .await?
        .ok_or_else(|| format!("session not found: {session_id}"))?;
    try_get_memory_chat_history_context_via_sdk(&session).await
}

async fn try_get_memory_chat_history_context_via_sdk(
    session: &Session,
) -> Result<ChatHistoryContext, String> {
    let payload = chatos_memory_engine::compose_chatos_context(session, true).await?;
    Ok(ChatHistoryContext {
        merged_summary: payload.merged_summary,
        summary_count: payload.summary_count,
        messages: payload.messages,
    })
}
