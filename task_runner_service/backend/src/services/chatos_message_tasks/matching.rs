use super::*;

pub(super) struct NormalizedChatosSource {
    source_session_id: String,
    source_user_message_id: Option<String>,
    source_turn_id: Option<String>,
}

pub(super) fn normalize_source_id(value: &str) -> Option<String> {
    normalized_optional(Some(value.to_string()))
}

pub(super) fn normalized_chatos_source(
    source_session_id: &str,
    source_user_message_id: Option<&str>,
    source_turn_id: Option<&str>,
) -> Option<NormalizedChatosSource> {
    let source_session_id = normalize_source_id(source_session_id)?;
    let source_user_message_id = source_user_message_id.and_then(normalize_source_id);
    let source_turn_id = source_turn_id.and_then(normalize_source_id);
    if source_user_message_id.is_none() && source_turn_id.is_none() {
        return None;
    }
    Some(NormalizedChatosSource {
        source_session_id,
        source_user_message_id,
        source_turn_id,
    })
}

pub(super) fn task_matches_source_user_message(
    task: &TaskRecord,
    source_user_message_id: &str,
) -> bool {
    task.source_user_message_id.as_deref() == Some(source_user_message_id)
}

impl NormalizedChatosSource {
    pub(super) fn matches_task(&self, task: &TaskRecord) -> bool {
        if task.source_session_id.as_deref() != Some(self.source_session_id.as_str()) {
            return false;
        }
        if let Some(source_user_message_id) = self.source_user_message_id.as_deref() {
            return task.source_user_message_id.as_deref() == Some(source_user_message_id);
        }
        self.source_turn_id
            .as_deref()
            .is_some_and(|source_turn_id| task.source_turn_id.as_deref() == Some(source_turn_id))
    }
}
