// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_store::decode_conversation, ClientStorageError, SqliteClientStorage,
    SqliteResultExt,
};
use chatos_local_agent_protocol::LocalConversationPage;

pub(super) async fn list_conversations(
    storage: &SqliteClientStorage,
    owner_user_id: &str,
    before_updated_at_unix_ms: Option<i64>,
    before_conversation_id: Option<&str>,
    limit: u32,
) -> Result<LocalConversationPage, ClientStorageError> {
    validate_page(
        owner_user_id,
        before_updated_at_unix_ms,
        before_conversation_id,
        limit,
    )?;
    let cursor_filter = if before_updated_at_unix_ms.is_some() {
        " AND (updated_at_unix_ms < ? OR (updated_at_unix_ms = ? AND conversation_id > ?))"
    } else {
        ""
    };
    let sql = format!(
        "SELECT conversation_id, owner_user_id, title, resource_kind, resource_id, version, created_at_unix_ms, \
         updated_at_unix_ms FROM local_conversations WHERE owner_user_id = ?{cursor_filter} \
         ORDER BY updated_at_unix_ms DESC, conversation_id ASC LIMIT ?"
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(owner_user_id);
    if let (Some(timestamp), Some(conversation_id)) =
        (before_updated_at_unix_ms, before_conversation_id)
    {
        query = query.bind(timestamp).bind(timestamp).bind(conversation_id);
    }
    let mut connection = storage.pool.acquire().await.db()?;
    let rows = query
        .bind(i64::from(limit) + 1)
        .fetch_all(&mut *connection)
        .await
        .db()?;
    let mut conversations = rows
        .into_iter()
        .map(decode_conversation)
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = conversations.len() > limit as usize;
    conversations.truncate(limit as usize);
    let (next_before_updated_at_unix_ms, next_before_conversation_id) = if has_more {
        let last = conversations.last().expect("positive validated page limit");
        (
            Some(last.updated_at_unix_ms),
            Some(last.conversation_id.clone()),
        )
    } else {
        (None, None)
    };
    Ok(LocalConversationPage {
        conversations,
        next_before_updated_at_unix_ms,
        next_before_conversation_id,
    })
}

fn validate_page(
    owner_user_id: &str,
    before_updated_at_unix_ms: Option<i64>,
    before_conversation_id: Option<&str>,
    limit: u32,
) -> Result<(), ClientStorageError> {
    let owner_valid = !owner_user_id.trim().is_empty()
        && owner_user_id.len() <= 256
        && !owner_user_id.chars().any(char::is_control);
    let cursor_valid = match (before_updated_at_unix_ms, before_conversation_id) {
        (None, None) => true,
        (Some(timestamp), Some(conversation_id)) => {
            timestamp >= 0
                && !conversation_id.trim().is_empty()
                && conversation_id.len() <= 256
                && !conversation_id.chars().any(char::is_control)
        }
        _ => false,
    };
    if !owner_valid || !cursor_valid || !(1..=200).contains(&limit) {
        return Err(ClientStorageError::InvalidState(
            "invalid Conversation list page request".to_string(),
        ));
    }
    Ok(())
}
