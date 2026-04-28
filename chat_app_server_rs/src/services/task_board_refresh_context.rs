use serde_json::Value;

use crate::services::task_board_prompt::{
    build_runtime_prefixed_input_items, build_runtime_prefixed_messages,
};

use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub(crate) struct TaskBoardRefreshContext {
    pub(crate) session_id: String,
    pub(crate) turn_id: Option<String>,
    pub(crate) contact_system_prompt: Option<String>,
    pub(crate) builtin_mcp_system_prompt: Option<String>,
    pub(crate) command_system_prompt: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TaskBoardRefreshContextStore {
    inner: Arc<Mutex<Option<TaskBoardRefreshContext>>>,
}

impl TaskBoardRefreshContextStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn set(
        &self,
        session_id: Option<String>,
        turn_id: Option<String>,
        contact_system_prompt: Option<String>,
        builtin_mcp_system_prompt: Option<String>,
        command_system_prompt: Option<String>,
    ) {
        if let Ok(mut slot) = self.inner.lock() {
            *slot = session_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .map(|value| TaskBoardRefreshContext {
                    session_id: value,
                    turn_id: turn_id
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty()),
                    contact_system_prompt,
                    builtin_mcp_system_prompt,
                    command_system_prompt,
                });
        }
    }

    pub(crate) fn snapshot(&self) -> Option<TaskBoardRefreshContext> {
        self.inner.lock().ok().and_then(|slot| slot.clone())
    }

    pub(crate) async fn load_prefixed_messages(&self) -> Option<Vec<Value>> {
        let context = self.snapshot()?;
        build_runtime_prefixed_messages(
            &context.session_id,
            context.turn_id.as_deref(),
            context.contact_system_prompt.as_deref(),
            context.builtin_mcp_system_prompt.as_deref(),
            context.command_system_prompt.as_deref(),
        )
        .await
    }

    pub(crate) async fn load_prefixed_input_items(&self) -> Option<Vec<Value>> {
        let context = self.snapshot()?;
        build_runtime_prefixed_input_items(
            &context.session_id,
            context.turn_id.as_deref(),
            context.contact_system_prompt.as_deref(),
            context.builtin_mcp_system_prompt.as_deref(),
            context.command_system_prompt.as_deref(),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::TaskBoardRefreshContextStore;

    #[test]
    fn ignores_empty_session_ids() {
        let store = TaskBoardRefreshContextStore::new();
        store.set(
            Some("   ".to_string()),
            Some("turn".to_string()),
            Some("contact".to_string()),
            None,
            None,
        );

        assert!(store.snapshot().is_none());
    }

    #[test]
    fn trims_session_and_turn_ids() {
        let store = TaskBoardRefreshContextStore::new();
        store.set(
            Some("  session-1  ".to_string()),
            Some("  turn-1  ".to_string()),
            Some("contact".to_string()),
            Some("builtin".to_string()),
            Some("command".to_string()),
        );

        let snapshot = store.snapshot().expect("context should be present");
        assert_eq!(snapshot.session_id, "session-1");
        assert_eq!(snapshot.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(snapshot.contact_system_prompt.as_deref(), Some("contact"));
        assert_eq!(
            snapshot.builtin_mcp_system_prompt.as_deref(),
            Some("builtin")
        );
        assert_eq!(snapshot.command_system_prompt.as_deref(), Some("command"));
    }

    #[test]
    fn drops_empty_turn_id_but_keeps_context() {
        let store = TaskBoardRefreshContextStore::new();
        store.set(
            Some("session-1".to_string()),
            Some("   ".to_string()),
            None,
            None,
            None,
        );

        let snapshot = store.snapshot().expect("context should be present");
        assert_eq!(snapshot.session_id, "session-1");
        assert!(snapshot.turn_id.is_none());
    }
}
