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
        Self {
            state: Arc::new(Mutex::new(State {
                recent_messages: HashMap::new(),
                pending_saves: VecDeque::new(),
                stats: Stats::default(),
            })),
        }
    }

    pub async fn save_user_message(
        &self,
        session_id: &str,
        content: &str,
        message_id: Option<String>,
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
        message.metadata = metadata;
        let saved = MessageService::create(message).await?;
        self.cache_message(saved.clone());
        Ok(saved)
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
        let mut message = Message::new(
            session_id.to_string(),
            "assistant".to_string(),
            content.to_string(),
        );
        message.summary = summary;
        message.reasoning = reasoning;
        message.metadata = metadata;
        message.tool_calls = tool_calls;
        let saved = MessageService::create(message).await?;
        self.cache_message(saved.clone());
        Ok(saved)
    }

    pub async fn save_tool_message(
        &self,
        session_id: &str,
        content: &str,
        tool_call_id: &str,
        metadata: Option<Value>,
    ) -> Result<Message, String> {
        let mut message = Message::new(
            session_id.to_string(),
            "tool".to_string(),
            content.to_string(),
        );
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

    pub async fn get_session_history_with_summaries(
        &self,
        session_id: &str,
        limit: Option<i64>,
        summary_limit: Option<i64>,
    ) -> (Vec<SessionSummary>, Vec<Message>) {
        let summaries =
            match SessionSummaryService::list_by_session(session_id, summary_limit).await {
                Ok(items) => items,
                Err(err) => {
                    error!("get_session_summaries failed: {}", err);
                    Vec::new()
                }
            };

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

    pub fn get_session_messages_sync(&self, session_id: &str, limit: Option<i64>) -> Vec<Message> {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            return tokio::task::block_in_place(|| {
                handle.block_on(self.get_session_messages(session_id, limit))
            });
        }
        let rt = tokio::runtime::Runtime::new();
        if let Ok(rt) = rt {
            return rt.block_on(self.get_session_messages(session_id, limit));
        }
        Vec::new()
    }

    pub async fn get_message_by_id(&self, message_id: &str) -> Option<Message> {
        if let Some(cached) = self.state.lock().recent_messages.get(message_id).cloned() {
            let mut state = self.state.lock();
            state.stats.cache_hits += 1;
            return Some(cached);
        }
        match MessageService::get_by_id(message_id).await {
            Ok(Some(msg)) => {
                self.cache_message(msg.clone());
                let mut state = self.state.lock();
                state.stats.cache_misses += 1;
                state.stats.messages_retrieved += 1;
                Some(msg)
            }
            _ => None,
        }
    }

    pub fn process_pending_saves(&self) -> Result<usize, String> {
        let mut state = self.state.lock();
        let mut processed = 0;
        while let Some(message) = state.pending_saves.pop_front() {
            if let Ok(saved) = MessageService::create_sync(message) {
                state.recent_messages.insert(saved.id.clone(), saved);
                processed += 1;
            }
        }
        Ok(processed)
    }

    pub fn get_stats(&self) -> Value {
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

    pub fn get_cache_stats(&self) -> Value {
        let state = self.state.lock();
        let mut by_session: HashMap<String, usize> = HashMap::new();
        for msg in state.recent_messages.values() {
            *by_session.entry(msg.session_id.clone()).or_insert(0) += 1;
        }
        serde_json::json!({
            "cache_size": state.recent_messages.len(),
            "by_session": by_session
        })
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
