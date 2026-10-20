use chatos_local_agent_protocol::{
    GetConversationHistoryCommand, HostCommand, HostResult, LocalConversationHistoryPage,
};
use tauri::State;

use crate::host_lifecycle::HostManager;

const CONVERSATION_HISTORY_PAGE_SIZE: u32 = 10;

#[tauri::command]
pub async fn load_conversation_history(
    manager: State<'_, HostManager>,
    owner_user_id: String,
    conversation_id: String,
    before_ordinal: Option<u64>,
) -> Result<LocalConversationHistoryPage, String> {
    let owner_user_id = required("owner_user_id", owner_user_id)?;
    let conversation_id = required("conversation_id", conversation_id)?;
    let result = manager
        .execute(HostCommand::GetConversationHistory(
            GetConversationHistoryCommand {
                owner_user_id: owner_user_id.clone(),
                conversation_id: conversation_id.clone(),
                before_ordinal,
                limit: CONVERSATION_HISTORY_PAGE_SIZE,
            },
        ))
        .await?;
    let HostResult::ConversationHistory { page } = result else {
        return Err("Local Agent Host returned an unexpected history result".into());
    };
    if page.conversation.owner_user_id != owner_user_id
        || page.conversation.conversation_id != conversation_id
    {
        return Err("Local Agent Host returned mismatched conversation history".into());
    }
    Ok(*page)
}

fn required(field: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_page_size_matches_macos_source_of_truth() {
        assert_eq!(CONVERSATION_HISTORY_PAGE_SIZE, 10);
    }

    #[test]
    fn identifiers_must_not_be_empty() {
        assert!(required("conversation_id", "  ".into()).is_err());
        assert_eq!(
            required("conversation_id", " conversation-1 ".into()).unwrap(),
            "conversation-1"
        );
    }
}
