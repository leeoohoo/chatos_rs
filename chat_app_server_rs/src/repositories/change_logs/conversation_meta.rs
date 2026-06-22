use std::collections::{HashMap, HashSet};

use crate::services::chatos_sessions;

#[derive(Debug, Clone, Default)]
pub(super) struct ConversationMetaLite {
    pub(super) title: Option<String>,
    pub(super) project_id: Option<String>,
}

pub(super) async fn load_conversation_meta_map(
    conversation_ids: &HashSet<String>,
) -> HashMap<String, ConversationMetaLite> {
    let mut out: HashMap<String, ConversationMetaLite> = HashMap::new();
    if conversation_ids.is_empty() {
        return out;
    }

    for conversation_id in conversation_ids {
        match chatos_sessions::get_session_by_id(conversation_id).await {
            Ok(Some(conversation)) => {
                let title = conversation.title.trim().to_string();
                let project_id = conversation
                    .project_id
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                out.insert(
                    conversation_id.clone(),
                    ConversationMetaLite {
                        title: if title.is_empty() { None } else { Some(title) },
                        project_id,
                    },
                );
            }
            Ok(None) => {}
            Err(_) => {}
        }
    }

    out
}
