use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value;
use tracing::error;

use crate::models::message::{Message, MessageService};
use crate::models::session_summary::{SessionSummary, SessionSummaryService};

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
pub struct MessageManager {
    state: Arc<Mutex<State>>,
}

impl MessageManager {
    pub fn new() -> Self {
        Self { state: Arc::new(Mutex::new(State { recent_messages: HashMap::new(), pending_saves: VecDeque::new(), stats: Stats::default() })) }
    }

    pub async fn save_user_message(&self, session_id: &str, content: &str, message_id: Option<String>, metadata: Option<Value>) -> Result<Message, String> {
        let mut message = Message::new(session_id.to_string(), "user".to_string(), content.to_string());
        if let Some(id) = message_id { message.id = id; }
        message.metadata = metadata;
        let saved = MessageService::create(message).await?;
        self.cache_message(saved.clone());
        Ok(saved)
    }

    pub async fn save_assistant_message(&self, session_id: &str, content: &str, summary: Option<String>, reasoning: Option<String>, metadata: Option<Value>, tool_calls: Option<Value>) -> Result<Message, String> {
        let mut message = Message::new(session_id.to_string(), "assistant".to_string(), content.to_string());
        message.summary = summary;
        message.reasoning = reasoning;
        message.metadata = metadata;
        message.tool_calls = tool_calls;
        let saved = MessageService::create(message).await?;
        self.cache_message(saved.clone());
        Ok(saved)
    }

    pub async fn save_tool_message(&self, session_id: &str, content: &str, tool_call_id: &str, metadata: Option<Value>) -> Result<Message, String> {
        let mut message = Message::new(session_id.to_string(), "tool".to_string(), content.to_string());
        message.tool_call_id = Some(tool_call_id.to_string());
        message.metadata = metadata;
        let saved = MessageService::create(message).await?;
        self.cache_message(saved.clone());
        Ok(saved)
    }

    pub async fn get_session_messages(&self, session_id: &str, limit: Option<i64>) -> Vec<Message> {
        let result = if let Some(l) = limit {
            MessageService::get_recent_by_session(session_id, l, 0).await
        } else {
            MessageService::get_by_session(session_id, None, 0).await
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

    pub async fn get_session_history_with_summaries(&self, session_id: &str, limit: Option<i64>, summary_limit: Option<i64>) -> (Vec<SessionSummary>, Vec<Message>) {
        let summaries = match SessionSummaryService::list_by_session(session_id, summary_limit).await {
            Ok(items) => items,
            Err(err) => {
                error!("get_session_summaries failed: {}", err);
                Vec::new()
            }
        };

        let summaries: Vec<SessionSummary> = summaries
            .into_iter()
            .filter(|s| !s.summary_text.trim().is_empty())
            .collect();

        if summaries.is_empty() {
            let messages = self.get_session_messages(session_id, limit).await;
            return (Vec::new(), messages);
        }

        let last = summaries.last().cloned();
        let messages_result = if let Some(last_summary) = last {
            if let Some(cutoff) = last_summary.last_message_created_at.clone() {
                MessageService::get_by_session_after(session_id, &cutoff, limit).await
            } else {
                MessageService::get_by_session(session_id, limit, 0).await
            }
        } else {
            MessageService::get_by_session(session_id, limit, 0).await
        };

        match messages_result {
            Ok(messages) => {
                let mut state = self.state.lock();
                state.stats.messages_retrieved += messages.len();
                (summaries, messages)
            }
            Err(err) => {
                error!("get_session_history_with_summaries failed: {}", err);
                (summaries, Vec::new())
            }
        }
    }

    pub async fn get_last_response_id(&self, session_id: &str, limit: i64) -> Option<String> {
        let summary_limit = Some(2);
        let (_summaries, messages) = self.get_session_history_with_summaries(session_id, Some(limit), summary_limit).await;
        for msg in messages.iter().rev() {
            if msg.role != "assistant" { continue; }
            let mut tool_calls = msg.tool_calls.clone().or_else(|| msg.metadata.clone().and_then(|m| m.get("toolCalls").cloned()));
            if let Some(Value::String(raw)) = tool_calls.clone() {
                if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                    tool_calls = Some(v);
                }
            }
            if let Some(tc) = tool_calls {
                if tc.as_array().map(|a| !a.is_empty()).unwrap_or(false) { continue; }
            }
            if let Some(meta) = &msg.metadata {
                if let Some(id) = meta.get("response_id").and_then(|v| v.as_str()) {
                    return Some(id.to_string());
                }
                if let Some(id) = meta.get("responseId").and_then(|v| v.as_str()) {
                    return Some(id.to_string());
                }
            }
        }
        None
    }

    fn cache_message(&self, message: Message) {
        let mut state = self.state.lock();
        if state.recent_messages.len() >= 100 {
            if let Some(key) = state.recent_messages.keys().next().cloned() {
                state.recent_messages.remove(&key);
            }
        }
        state.recent_messages.insert(message.id.clone(), message);
        state.stats.messages_saved += 1;
    }
}

