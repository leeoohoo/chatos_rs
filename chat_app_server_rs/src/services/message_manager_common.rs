use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value;
use tracing::error;

use crate::core::mcp_tools::ToolResult;
use crate::models::message::Message;
use crate::models::session_summary_v2::SessionSummaryV2;
use crate::models::sub_agent_run_message::{SubAgentRunMessage, SubAgentRunMessageService};
use crate::models::sub_agent_run_summary::SubAgentRunSummaryService;
use crate::services::ai_common::build_tool_result_metadata;
use crate::services::memory_server_client;

#[derive(Debug, Default, Clone)]
struct Stats {
    messages_saved: usize,
    messages_retrieved: usize,
    cache_hits: usize,
    cache_misses: usize,
}

#[derive(Debug)]
struct State {
    recent_messages: HashMap<String, Message>,
    pending_saves: VecDeque<Message>,
    stats: Stats,
}

#[derive(Clone)]
pub(crate) struct MessageManagerCore {
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ChatHistoryContext {
    pub merged_summary: Option<String>,
    pub summary_count: usize,
    pub messages: Vec<Message>,
}

impl MessageManagerCore {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                recent_messages: HashMap::new(),
                pending_saves: VecDeque::new(),
                stats: Stats::default(),
            })),
        }
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

        let saved = self.persist_message(message).await?;
        self.cache_message(saved.clone());
        Ok(saved)
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

        let saved = self.persist_message(message).await?;
        self.cache_message(saved.clone());
        Ok(saved)
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

        let saved = self.persist_message(message).await?;
        self.cache_message(saved.clone());
        Ok(saved)
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
        memory_server_client::upsert_message(&message).await
    }

    pub(crate) async fn get_session_messages(
        &self,
        session_id: &str,
        limit: Option<i64>,
    ) -> Vec<Message> {
        let result = if let Some(value) = limit {
            memory_server_client::list_messages(session_id, Some(value), 0, false)
                .await
                .map(|mut items| {
                    items.reverse();
                    items
                })
        } else {
            memory_server_client::list_messages(session_id, None, 0, true).await
        };

        match result {
            Ok(messages) => {
                let mut state = self.state.lock();
                state.stats.messages_retrieved += messages.len();
                messages
            }
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
        let mut summaries = match memory_server_client::list_summaries(
            session_id,
            memory_summary_limit,
            0,
        )
        .await
        {
            Ok(items) => items,
            Err(err) => {
                error!("list_summaries from memory_server failed: {}", err);
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

        let mut messages = match memory_server_client::list_messages(session_id, None, 0, true).await
        {
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
            if let Some(last_idx) = messages.iter().position(|message| message.id == last_message_id) {
                messages = messages.into_iter().skip(last_idx + 1).collect();
            }
        }

        if let Some(v) = limit {
            if v > 0 && messages.len() > v as usize {
                messages = messages[messages.len() - v as usize..].to_vec();
            }
        }

        let mut state = self.state.lock();
        state.stats.messages_retrieved += messages.len();
        (summaries, messages)
    }

    pub(crate) async fn get_memory_chat_history_context(
        &self,
        session_id: &str,
        memory_summary_limit: usize,
    ) -> ChatHistoryContext {
        match try_get_memory_chat_history_context_from_memory_server(
            session_id,
            memory_summary_limit,
        )
        .await
        {
            Ok(context) => context,
            Err(err) => {
                error!(
                    "get_memory_chat_history_context memory_server failed: session_id={} error={}",
                    session_id, err
                );
                ChatHistoryContext {
                    merged_summary: None,
                    summary_count: 0,
                    messages: self.get_session_messages(session_id, None).await,
                }
            }
        }
    }

    pub(crate) async fn get_memory_sub_agent_run_history_context(
        &self,
        run_id: &str,
        memory_summary_limit: usize,
    ) -> ChatHistoryContext {
        let target_summary_limit = memory_summary_limit.max(1);
        let fetch_summary_limit = (target_summary_limit as i64).saturating_mul(10).max(10);
        let mut recent_summary_texts: Vec<String> = Vec::new();

        match SubAgentRunSummaryService::list_by_run(run_id, Some(fetch_summary_limit), 0).await {
            Ok(records) => {
                for record in records {
                    if record.status != "done" {
                        continue;
                    }
                    let text = record.summary_text.trim();
                    if text.is_empty() {
                        continue;
                    }
                    recent_summary_texts.push(text.to_string());
                    if recent_summary_texts.len() >= target_summary_limit {
                        break;
                    }
                }
            }
            Err(err) => {
                error!(
                    "get_memory_sub_agent_run_history_context summaries failed: run_id={} error={}",
                    run_id, err
                );
            }
        }

        if recent_summary_texts.is_empty() {
            let messages = match SubAgentRunMessageService::list_by_run(run_id, None).await {
                Ok(items) => items
                    .into_iter()
                    .map(map_sub_agent_run_message_to_message)
                    .collect(),
                Err(err) => {
                    error!(
                        "get_memory_sub_agent_run_history_context messages failed: run_id={} error={}",
                        run_id, err
                    );
                    Vec::new()
                }
            };
            return ChatHistoryContext {
                merged_summary: None,
                summary_count: 0,
                messages,
            };
        }

        recent_summary_texts.reverse();
        let merged_summary = Some(format!(
            "以下是当前 Sub-Agent 历史总结（按时间从旧到新）：\n\n{}",
            recent_summary_texts.join("\n\n---\n\n")
        ));

        let messages = match SubAgentRunMessageService::get_pending_for_summary(run_id, None).await
        {
            Ok(items) => items
                .into_iter()
                .map(map_sub_agent_run_message_to_message)
                .collect(),
            Err(err) => {
                error!(
                    "get_memory_sub_agent_run_history_context pending failed: run_id={} error={}",
                    run_id, err
                );
                Vec::new()
            }
        };

        ChatHistoryContext {
            merged_summary,
            summary_count: recent_summary_texts.len(),
            messages,
        }
    }

    pub(crate) async fn get_message_by_id(&self, message_id: &str) -> Option<Message> {
        if let Some(cached) = {
            let mut state = self.state.lock();
            let cached = state.recent_messages.get(message_id).cloned();
            if cached.is_some() {
                state.stats.cache_hits += 1;
            }
            cached
        } {
            return Some(cached);
        }

        let result = memory_server_client::get_message_by_id(message_id).await;

        match result {
            Ok(Some(message)) => {
                self.cache_message(message.clone());

                let mut state = self.state.lock();
                state.stats.cache_misses += 1;
                state.stats.messages_retrieved += 1;
                Some(message)
            }
            _ => None,
        }
    }

    pub(crate) fn process_pending_saves(&self) -> Result<usize, String> {
        Ok(0)
    }

    pub(crate) fn get_stats(&self) -> Value {
        let state = self.state.lock();
        serde_json::json!({
            "stats": {
                "messages_saved": state.stats.messages_saved,
                "messages_retrieved": state.stats.messages_retrieved,
                "cache_hits": state.stats.cache_hits,
                "cache_misses": state.stats.cache_misses,
            },
            "cache_size": state.recent_messages.len(),
            "pending_saves": state.pending_saves.len()
        })
    }

    pub(crate) fn get_cache_stats(&self) -> Value {
        let state = self.state.lock();
        let mut by_session: HashMap<String, usize> = HashMap::new();

        for message in state.recent_messages.values() {
            *by_session.entry(message.session_id.clone()).or_insert(0) += 1;
        }

        serde_json::json!({
            "cache_size": state.recent_messages.len(),
            "by_session": by_session
        })
    }

    fn cache_message(&self, message: Message) {
        let mut state = self.state.lock();

        if state.recent_messages.len() >= 100 {
            if let Some(oldest_key) = state.recent_messages.keys().next().cloned() {
                state.recent_messages.remove(&oldest_key);
            }
        }

        state.recent_messages.insert(message.id.clone(), message);
        state.stats.messages_saved += 1;
    }
}

async fn try_get_memory_chat_history_context_from_memory_server(
    session_id: &str,
    memory_summary_limit: usize,
) -> Result<ChatHistoryContext, String> {
    let payload = memory_server_client::compose_context(session_id, memory_summary_limit).await?;
    Ok(ChatHistoryContext {
        merged_summary: payload.0,
        summary_count: payload.1,
        messages: payload.2,
    })
}

fn map_sub_agent_run_message_to_message(source: SubAgentRunMessage) -> Message {
    Message {
        id: source.id,
        session_id: source.run_id,
        role: source.role,
        content: source.content,
        message_mode: None,
        message_source: None,
        summary: None,
        tool_calls: None,
        tool_call_id: source.tool_call_id,
        reasoning: source.reasoning,
        metadata: source.metadata,
        created_at: source.created_at,
    }
}
