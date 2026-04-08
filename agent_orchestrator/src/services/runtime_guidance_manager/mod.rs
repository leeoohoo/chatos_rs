use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Serialize;
use uuid::Uuid;

const TURN_KEY_SEPARATOR: &str = "::";
pub const DEFAULT_MAX_QUEUE_SIZE: usize = 20;
pub const DEFAULT_DRAIN_LIMIT: usize = 20;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeGuidanceStatus {
    Queued,
    Applied,
    Dropped,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeGuidanceItem {
    pub guidance_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub content: String,
    pub status: RuntimeGuidanceStatus,
    pub created_at: String,
    pub applied_at: Option<String>,
    pub dropped_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueGuidanceError {
    TurnNotRunning,
}

#[derive(Debug, Default)]
struct ActiveTurnState {
    queue: VecDeque<String>,
    max_queue_size: usize,
}

#[derive(Debug, Default)]
struct State {
    active_turn_by_session: HashMap<String, String>,
    turns: HashMap<String, ActiveTurnState>,
    items: HashMap<String, RuntimeGuidanceItem>,
}

#[derive(Clone)]
pub struct RuntimeGuidanceManager {
    state: Arc<Mutex<State>>,
    default_max_queue_size: usize,
}

impl RuntimeGuidanceManager {
    pub fn new(default_max_queue_size: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            default_max_queue_size: default_max_queue_size.max(1),
        }
    }

    pub fn register_active_turn(&self, session_id: &str, turn_id: &str) {
        let session_id = session_id.trim();
        let turn_id = turn_id.trim();
        if session_id.is_empty() || turn_id.is_empty() {
            return;
        }

        let mut state = self.state.lock();
        if let Some(previous_turn) = state.active_turn_by_session.get(session_id).cloned() {
            if previous_turn != turn_id {
                drop_turn_locked(&mut state, session_id, previous_turn.as_str());
            }
        }

        state
            .active_turn_by_session
            .insert(session_id.to_string(), turn_id.to_string());
        let key = turn_key(session_id, turn_id);
        state.turns.entry(key).or_insert_with(|| ActiveTurnState {
            queue: VecDeque::new(),
            max_queue_size: self.default_max_queue_size,
        });
    }

    pub fn is_active_turn(&self, session_id: &str, turn_id: &str) -> bool {
        let session_id = session_id.trim();
        let turn_id = turn_id.trim();
        if session_id.is_empty() || turn_id.is_empty() {
            return false;
        }
        let state = self.state.lock();
        state
            .active_turn_by_session
            .get(session_id)
            .map(|active_turn| active_turn == turn_id)
            .unwrap_or(false)
    }

    pub fn enqueue_guidance(
        &self,
        session_id: &str,
        turn_id: &str,
        content: &str,
    ) -> Result<RuntimeGuidanceItem, EnqueueGuidanceError> {
        let session_id = session_id.trim();
        let turn_id = turn_id.trim();
        let content = content.trim();
        if session_id.is_empty()
            || turn_id.is_empty()
            || content.is_empty()
            || !self.is_active_turn(session_id, turn_id)
        {
            return Err(EnqueueGuidanceError::TurnNotRunning);
        }

        let now = crate::core::time::now_rfc3339();
        let mut state = self.state.lock();
        let key = turn_key(session_id, turn_id);
        let max_queue_size = state
            .turns
            .entry(key.clone())
            .or_insert_with(|| ActiveTurnState {
                queue: VecDeque::new(),
                max_queue_size: self.default_max_queue_size,
            })
            .max_queue_size
            .max(1);
        let mut dropped_ids = Vec::new();
        loop {
            let should_drop = state
                .turns
                .get(key.as_str())
                .map(|turn| turn.queue.len() >= max_queue_size)
                .unwrap_or(false);
            if !should_drop {
                break;
            }
            let dropped_id = state
                .turns
                .get_mut(key.as_str())
                .and_then(|turn| turn.queue.pop_front());
            if let Some(dropped_id) = dropped_id {
                dropped_ids.push(dropped_id);
            } else {
                break;
            }
        }
        for dropped_id in dropped_ids {
            if let Some(item) = state.items.get_mut(dropped_id.as_str()) {
                item.status = RuntimeGuidanceStatus::Dropped;
                item.dropped_at = Some(now.clone());
            }
        }

        let guidance_id = format!("gd_{}", Uuid::new_v4().simple());
        let item = RuntimeGuidanceItem {
            guidance_id: guidance_id.clone(),
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            content: content.to_string(),
            status: RuntimeGuidanceStatus::Queued,
            created_at: now,
            applied_at: None,
            dropped_at: None,
        };
        state.items.insert(guidance_id.clone(), item.clone());
        if let Some(turn_state) = state.turns.get_mut(key.as_str()) {
            turn_state.queue.push_back(guidance_id);
        }
        Ok(item)
    }

    pub fn drain_guidance(
        &self,
        session_id: &str,
        turn_id: &str,
        limit: usize,
    ) -> Vec<RuntimeGuidanceItem> {
        let session_id = session_id.trim();
        let turn_id = turn_id.trim();
        if session_id.is_empty() || turn_id.is_empty() || limit == 0 {
            return Vec::new();
        }
        if !self.is_active_turn(session_id, turn_id) {
            return Vec::new();
        }

        let mut state = self.state.lock();
        let key = turn_key(session_id, turn_id);
        let Some(turn_state) = state.turns.get_mut(key.as_str()) else {
            return Vec::new();
        };

        let mut drained_ids = Vec::new();
        while drained_ids.len() < limit {
            let Some(guidance_id) = turn_state.queue.pop_front() else {
                break;
            };
            drained_ids.push(guidance_id);
        }

        let mut drained = Vec::new();
        for guidance_id in drained_ids {
            if let Some(item) = state.items.get(guidance_id.as_str()) {
                drained.push(item.clone());
            }
        }
        drained
    }

    pub fn mark_applied(
        &self,
        session_id: &str,
        turn_id: &str,
        guidance_id: &str,
    ) -> Option<RuntimeGuidanceItem> {
        let session_id = session_id.trim();
        let turn_id = turn_id.trim();
        let guidance_id = guidance_id.trim();
        if session_id.is_empty() || turn_id.is_empty() || guidance_id.is_empty() {
            return None;
        }

        let mut state = self.state.lock();
        let item = state.items.get_mut(guidance_id)?;
        if item.session_id != session_id || item.turn_id != turn_id {
            return None;
        }
        item.status = RuntimeGuidanceStatus::Applied;
        item.applied_at = Some(crate::core::time::now_rfc3339());
        Some(item.clone())
    }

    pub fn pending_count(&self, session_id: &str, turn_id: &str) -> usize {
        let session_id = session_id.trim();
        let turn_id = turn_id.trim();
        if session_id.is_empty() || turn_id.is_empty() {
            return 0;
        }
        let state = self.state.lock();
        let key = turn_key(session_id, turn_id);
        state
            .turns
            .get(key.as_str())
            .map(|turn| turn.queue.len())
            .unwrap_or(0)
    }

    pub fn close_turn(&self, session_id: &str, turn_id: &str) {
        let session_id = session_id.trim();
        let turn_id = turn_id.trim();
        if session_id.is_empty() || turn_id.is_empty() {
            return;
        }

        let mut state = self.state.lock();
        if state
            .active_turn_by_session
            .get(session_id)
            .map(|active_turn| active_turn == turn_id)
            .unwrap_or(false)
        {
            state.active_turn_by_session.remove(session_id);
        }
        drop_turn_locked(&mut state, session_id, turn_id);
    }
}

fn turn_key(session_id: &str, turn_id: &str) -> String {
    format!("{session_id}{TURN_KEY_SEPARATOR}{turn_id}")
}

fn drop_turn_locked(state: &mut State, session_id: &str, turn_id: &str) {
    let key = turn_key(session_id, turn_id);
    let Some(mut turn_state) = state.turns.remove(key.as_str()) else {
        return;
    };
    let dropped_at = crate::core::time::now_rfc3339();
    while let Some(guidance_id) = turn_state.queue.pop_front() {
        if let Some(item) = state.items.get_mut(guidance_id.as_str()) {
            item.status = RuntimeGuidanceStatus::Dropped;
            item.dropped_at = Some(dropped_at.clone());
        }
    }
}

static RUNTIME_GUIDANCE_MANAGER: Lazy<RuntimeGuidanceManager> =
    Lazy::new(|| RuntimeGuidanceManager::new(DEFAULT_MAX_QUEUE_SIZE));

pub fn runtime_guidance_manager() -> &'static RuntimeGuidanceManager {
    &RUNTIME_GUIDANCE_MANAGER
}
