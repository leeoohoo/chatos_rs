use serde::Serialize;
use serde_json::json;

use crate::models::ImConversation;

use super::SharedState;

pub(super) fn publish_conversation_event(
    state: &SharedState,
    event_type: &str,
    conversation: &ImConversation,
) {
    state.event_hub.publish_to_user(
        conversation.owner_user_id.as_str(),
        json!({
            "type": event_type,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "conversation": conversation,
        }),
    );
}

pub(super) fn publish_conversation_scoped_event<T: Serialize>(
    state: &SharedState,
    owner_user_id: &str,
    event_type: &str,
    conversation_id: &str,
    field_name: &str,
    payload: &T,
) {
    state.event_hub.publish_to_user(
        owner_user_id,
        json!({
            "type": event_type,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "conversation_id": conversation_id,
            field_name: payload,
        }),
    );
}
