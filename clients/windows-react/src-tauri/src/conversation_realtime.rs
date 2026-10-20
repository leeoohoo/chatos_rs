use chatos_local_agent_protocol::{
    GetConversationCommand, GetEventCursorCommand, HostCommand, HostResult, ListEventsCommand,
    LocalAgentEventPayloadMode,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::State;

use crate::host_lifecycle::HostManager;

const EVENT_PAGE_SIZE: u32 = 100;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRealtimePollInput {
    owner_user_id: String,
    conversation_id: String,
    after_cursor: Option<i64>,
    observed_conversation_version: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRealtimeCursor {
    next_cursor: i64,
    conversation_version: u64,
    affected: bool,
}

#[tauri::command]
pub async fn poll_conversation_realtime(
    manager: State<'_, HostManager>,
    input: ConversationRealtimePollInput,
) -> Result<ConversationRealtimeCursor, String> {
    let owner_user_id = required("owner_user_id", input.owner_user_id)?;
    let conversation_id = required("conversation_id", input.conversation_id)?;
    let context = conversation_context(&manager, &owner_user_id, &conversation_id).await?;
    let conversation_version = context.conversation.version;
    let Some(after_cursor) = input.after_cursor else {
        let result = manager
            .execute(HostCommand::GetEventCursor(GetEventCursorCommand {
                owner_user_id,
            }))
            .await?;
        let HostResult::EventCursor { cursor } = result else {
            return Err("Local Agent Host 返回了错误的事件游标响应。".into());
        };
        return Ok(ConversationRealtimeCursor {
            next_cursor: cursor,
            conversation_version,
            affected: conversation_version > input.observed_conversation_version,
        });
    };
    let result = manager
        .execute(HostCommand::ListEvents(ListEventsCommand {
            owner_user_id,
            after_cursor,
            limit: EVENT_PAGE_SIZE,
            run_id: None,
            event_type: None,
            newest_first: false,
            payload_mode: LocalAgentEventPayloadMode::Routing,
        }))
        .await?;
    let HostResult::Events {
        events,
        next_cursor,
    } = result
    else {
        return Err("Local Agent Host 返回了错误的事件响应。".into());
    };
    let known_run_ids = context
        .turns
        .iter()
        .map(|turn| turn.run_id.as_str())
        .collect::<HashSet<_>>();
    let affected_by_event = events.iter().any(|event| {
        known_run_ids.contains(event.run_id.as_str())
            || event
                .payload
                .as_object()
                .and_then(|payload| payload.get("conversation_id"))
                .and_then(|value| value.as_str())
                .is_some_and(|value| value == conversation_id)
    });
    Ok(ConversationRealtimeCursor {
        next_cursor,
        conversation_version,
        affected: affected_by_event || conversation_version > input.observed_conversation_version,
    })
}

async fn conversation_context(
    manager: &HostManager,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<chatos_local_agent_protocol::LocalConversationDetail, String> {
    let result = manager
        .execute(HostCommand::GetConversationContext(
            GetConversationCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::Conversation { conversation } = result else {
        return Err("Local Agent Host 返回了错误的会话响应。".into());
    };
    if conversation.conversation.owner_user_id != owner_user_id
        || conversation.conversation.conversation_id != conversation_id
    {
        return Err("Local Agent Host 返回了不匹配的会话。".into());
    }
    Ok(conversation)
}

fn required(field: &str, value: String) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.chars().any(char::is_control) {
        Err(format!("{field} is required"))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polling_uses_bounded_event_pages() {
        assert!((1..=200).contains(&EVENT_PAGE_SIZE));
    }

    #[test]
    fn cursor_and_identifiers_are_validated() {
        assert!(required("conversation_id", "  ".into()).is_err());
        assert!(required("conversation_id", "conversation-1".into()).is_ok());
    }
}
