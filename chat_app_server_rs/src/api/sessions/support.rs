use serde_json::Value;

use crate::core::chat_runtime::{
    contact_agent_id_from_metadata as runtime_contact_agent_id_from_metadata,
    contact_id_from_metadata as runtime_contact_id_from_metadata,
};
use crate::models::message::Message;
use crate::services::memory_server_client;

const FULL_SESSION_MESSAGES_PAGE_SIZE: i64 = 500;

pub(super) async fn list_all_session_messages(session_id: &str) -> Result<Vec<Message>, String> {
    let mut offset = 0i64;
    let mut all_messages: Vec<Message> = Vec::new();

    loop {
        let batch = memory_server_client::list_messages(
            session_id,
            Some(FULL_SESSION_MESSAGES_PAGE_SIZE),
            offset,
            true,
        )
        .await?;

        let batch_len = batch.len();
        if batch_len == 0 {
            break;
        }

        offset += batch_len as i64;
        all_messages.extend(batch);

        if batch_len < FULL_SESSION_MESSAGES_PAGE_SIZE as usize {
            break;
        }
    }

    Ok(all_messages)
}

pub(super) fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn normalize_project_scope(project_id: Option<&str>) -> String {
    normalize_optional_text(project_id).unwrap_or_else(|| "0".to_string())
}

pub(super) fn contact_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    runtime_contact_id_from_metadata(metadata)
}

pub(super) fn contact_agent_id_from_metadata(metadata: Option<&Value>) -> Option<String> {
    runtime_contact_agent_id_from_metadata(metadata)
}
