// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub(crate) struct LocalMemoryJobRegistry {
    active_sessions: Arc<Mutex<HashSet<String>>>,
}

impl LocalMemoryJobRegistry {
    pub(crate) fn register(&self, session_id: &str) -> Result<LocalMemoryJobGuard, String> {
        let mut active = self
            .active_sessions
            .lock()
            .map_err(|_| "local memory job registry is unavailable".to_string())?;
        if !active.insert(session_id.to_string()) {
            return Err("local memory review is already running".to_string());
        }
        Ok(LocalMemoryJobGuard {
            registry: self.clone(),
            session_id: session_id.to_string(),
        })
    }

    pub(crate) fn is_running(&self, session_id: &str) -> bool {
        self.active_sessions
            .lock()
            .map(|active| active.contains(session_id))
            .unwrap_or(false)
    }

    fn release(&self, session_id: &str) {
        if let Ok(mut active) = self.active_sessions.lock() {
            active.remove(session_id);
        }
    }
}

pub(crate) struct LocalMemoryJobGuard {
    registry: LocalMemoryJobRegistry,
    session_id: String,
}

impl Drop for LocalMemoryJobGuard {
    fn drop(&mut self) {
        self.registry.release(self.session_id.as_str());
    }
}
