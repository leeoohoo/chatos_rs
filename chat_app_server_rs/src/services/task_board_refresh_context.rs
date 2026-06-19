use serde_json::Value;

#[cfg(test)]
use crate::modules::conversation_runtime::task_board::build_runtime_context;
use crate::modules::conversation_runtime::task_board::{
    TaskBoardRuntimeContext, load_prefixed_input_items,
};

use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub(crate) struct TaskBoardRefreshContextStore {
    inner: Arc<Mutex<Option<TaskBoardRuntimeContext>>>,
}

impl TaskBoardRefreshContextStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn set(
        &self,
        session_id: Option<String>,
        turn_id: Option<String>,
        locale: crate::core::internal_context_locale::InternalContextLocale,
        contact_system_prompt: Option<String>,
        builtin_mcp_system_prompt: Option<String>,
        command_system_prompt: Option<String>,
        task_runner_skill_prompt: Option<String>,
    ) {
        if let Ok(mut slot) = self.inner.lock() {
            *slot = build_runtime_context(
                session_id,
                turn_id,
                locale,
                contact_system_prompt,
                builtin_mcp_system_prompt,
                command_system_prompt,
                task_runner_skill_prompt,
            );
        }
    }

    pub(crate) fn snapshot(&self) -> Option<TaskBoardRuntimeContext> {
        self.inner.lock().ok().and_then(|slot| slot.clone())
    }

    pub(crate) async fn load_prefixed_input_items(&self) -> Option<Vec<Value>> {
        let context = self.snapshot()?;
        load_prefixed_input_items(&context).await
    }
}

#[cfg(test)]
mod tests {
    use super::TaskBoardRefreshContextStore;
    use crate::core::internal_context_locale::InternalContextLocale;

    #[test]
    fn ignores_empty_session_ids() {
        let store = TaskBoardRefreshContextStore::new();
        store.set(
            Some("   ".to_string()),
            Some("turn".to_string()),
            InternalContextLocale::ZhCn,
            Some("contact".to_string()),
            None,
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
            InternalContextLocale::EnUs,
            Some("contact".to_string()),
            Some("builtin".to_string()),
            Some("command".to_string()),
            Some("task runner skill".to_string()),
        );

        let snapshot = store.snapshot().expect("context should be present");
        assert_eq!(snapshot.session_id, "session-1");
        assert_eq!(snapshot.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(snapshot.locale, InternalContextLocale::EnUs);
        assert_eq!(snapshot.contact_system_prompt.as_deref(), Some("contact"));
        assert_eq!(
            snapshot.builtin_mcp_system_prompt.as_deref(),
            Some("builtin")
        );
        assert_eq!(snapshot.command_system_prompt.as_deref(), Some("command"));
        assert_eq!(
            snapshot.task_runner_skill_prompt.as_deref(),
            Some("task runner skill")
        );
    }

    #[test]
    fn drops_empty_turn_id_but_keeps_context() {
        let store = TaskBoardRefreshContextStore::new();
        store.set(
            Some("session-1".to_string()),
            Some("   ".to_string()),
            InternalContextLocale::ZhCn,
            None,
            None,
            None,
            None,
        );

        let snapshot = store.snapshot().expect("context should be present");
        assert_eq!(snapshot.session_id, "session-1");
        assert!(snapshot.turn_id.is_none());
    }
}
