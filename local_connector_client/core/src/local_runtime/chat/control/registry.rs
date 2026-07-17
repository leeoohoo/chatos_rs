// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use tokio_util::sync::CancellationToken;

use super::LocalRuntimeGuidance;

#[derive(Debug, Clone, Default)]
pub(crate) struct LocalTurnControlRegistry {
    inner: Arc<Mutex<HashMap<String, ActiveTurnEntry>>>,
}

#[derive(Debug, Clone)]
struct ActiveTurnEntry {
    turn_id: String,
    token: CancellationToken,
    guidance: VecDeque<LocalRuntimeGuidance>,
}

#[derive(Debug)]
pub(crate) struct ActiveLocalTurnGuard {
    registry: LocalTurnControlRegistry,
    session_id: String,
    turn_id: String,
    token: CancellationToken,
}

impl LocalTurnControlRegistry {
    pub(crate) fn register(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<ActiveLocalTurnGuard, String> {
        let mut active = self
            .inner
            .lock()
            .map_err(|_| "local turn control registry is poisoned".to_string())?;
        if let Some(current) = active.get(session_id) {
            return Err(format!(
                "Local session already has an active turn: {}",
                current.turn_id
            ));
        }
        let token = CancellationToken::new();
        active.insert(
            session_id.to_string(),
            ActiveTurnEntry {
                turn_id: turn_id.to_string(),
                token: token.clone(),
                guidance: VecDeque::new(),
            },
        );
        Ok(ActiveLocalTurnGuard {
            registry: self.clone(),
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            token,
        })
    }

    pub(crate) fn cancel(&self, session_id: &str, turn_id: Option<&str>) -> bool {
        let Ok(active) = self.inner.lock() else {
            return false;
        };
        let Some(entry) = active.get(session_id) else {
            return false;
        };
        if turn_id.is_some_and(|turn_id| turn_id != entry.turn_id) {
            return false;
        }
        entry.token.cancel();
        true
    }

    pub(crate) fn is_running(&self, session_id: &str) -> bool {
        self.inner
            .lock()
            .map(|active| active.contains_key(session_id))
            .unwrap_or(false)
    }

    pub(crate) fn enqueue_guidance(&self, item: LocalRuntimeGuidance) -> Result<(), String> {
        let mut active = self
            .inner
            .lock()
            .map_err(|_| "local turn control registry is poisoned".to_string())?;
        let entry = active
            .get_mut(item.session_id.as_str())
            .filter(|entry| entry.turn_id == item.turn_id)
            .ok_or_else(|| "Local runtime turn is not running".to_string())?;
        entry.guidance.push_back(item);
        Ok(())
    }

    pub(crate) fn remove_guidance(&self, session_id: &str, turn_id: &str, guidance_id: &str) {
        let Ok(mut active) = self.inner.lock() else {
            return;
        };
        let Some(entry) = active
            .get_mut(session_id)
            .filter(|entry| entry.turn_id == turn_id)
        else {
            return;
        };
        entry
            .guidance
            .retain(|item| item.guidance_id != guidance_id);
    }

    pub(crate) fn drain_guidance(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Vec<LocalRuntimeGuidance> {
        let Ok(mut active) = self.inner.lock() else {
            return Vec::new();
        };
        let Some(entry) = active
            .get_mut(session_id)
            .filter(|entry| entry.turn_id == turn_id)
        else {
            return Vec::new();
        };
        entry.guidance.drain(..).collect()
    }

    fn close(&self, session_id: &str, turn_id: &str) {
        let Ok(mut active) = self.inner.lock() else {
            return;
        };
        if active
            .get(session_id)
            .is_some_and(|entry| entry.turn_id == turn_id)
        {
            active.remove(session_id);
        }
    }
}

impl ActiveLocalTurnGuard {
    pub(crate) fn token(&self) -> CancellationToken {
        self.token.clone()
    }
}

impl Drop for ActiveLocalTurnGuard {
    fn drop(&mut self) {
        self.registry
            .close(self.session_id.as_str(), self.turn_id.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::LocalTurnControlRegistry;

    #[test]
    fn cancellation_only_targets_the_active_turn() {
        let registry = LocalTurnControlRegistry::default();
        let guard = registry
            .register("session-1", "turn-1")
            .expect("register turn");

        assert!(!registry.cancel("session-1", Some("turn-old")));
        assert!(!guard.token().is_cancelled());
        assert!(registry.cancel("session-1", Some("turn-1")));
        assert!(guard.token().is_cancelled());
    }

    #[test]
    fn dropping_guard_releases_the_session() {
        let registry = LocalTurnControlRegistry::default();
        let guard = registry
            .register("session-1", "turn-1")
            .expect("register first turn");
        assert!(registry.register("session-1", "turn-2").is_err());
        drop(guard);
        assert!(registry.register("session-1", "turn-2").is_ok());
    }
}
