use super::*;

pub(super) fn delivery_state(turn: &LocalConversationTurnRecord) -> (&'static str, Option<i64>) {
    match turn.status {
        LocalConversationTurnStatus::Running => ("running", None),
        LocalConversationTurnStatus::Succeeded => ("completed", Some(turn.updated_at_unix_ms)),
        LocalConversationTurnStatus::Failed => ("failed", Some(turn.updated_at_unix_ms)),
        LocalConversationTurnStatus::Cancelled => ("cancelled", Some(turn.updated_at_unix_ms)),
    }
}

pub(super) fn projected_message_text(content: &serde_json::Value) -> String {
    if let Some(value) = content.as_str() {
        return value.chars().take(1_000_000).collect();
    }
    let projected = content.as_object().and_then(|object| {
        ["text", "content", "report", "answer", "error", "message"]
            .into_iter()
            .find_map(|key| object.get(key).and_then(serde_json::Value::as_str))
    });
    projected
        .unwrap_or_default()
        .chars()
        .take(1_000_000)
        .collect()
}

pub(super) fn identifier(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

pub(super) fn validate_host_conversation(
    conversation: &chatos_local_agent_protocol::LocalConversationDetail,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<(), String> {
    if conversation.conversation.owner_user_id != owner_user_id
        || conversation.conversation.conversation_id != conversation_id
    {
        return Err("Local Agent Host 返回了不匹配的 Agent 会话。".into());
    }
    Ok(())
}

pub(super) fn validate_team_executor_host_conversation(
    conversation: &LocalConversationDetail,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<(), String> {
    if conversation.conversation.owner_user_id != owner_user_id
        || conversation.conversation.conversation_id != conversation_id
        || conversation.conversation.resource.is_some()
    {
        return Err("Local Agent Host 返回了不匹配的隔离 executor 会话。".into());
    }
    Ok(())
}

pub(super) fn load_direct_room(
    connection: &Connection,
    owner_user_id: &str,
    direct_key: &str,
) -> Result<Option<DirectRoom>, String> {
    connection
        .query_row(
            r#"
            SELECT room.id, member.agent_id, room.name,
                   room.created_at_unix_ms, room.updated_at_unix_ms
            FROM project_agent_rooms room
            JOIN project_agent_room_members member
              ON member.owner_user_id = room.owner_user_id AND member.room_id = room.id
            WHERE room.owner_user_id = ?1 AND room.direct_key = ?2
              AND room.conversation_kind = 'human_agent_direct'
              AND room.status = 'active' AND member.status = 'active'
            ORDER BY member.joined_at_unix_ms, member.agent_id
            LIMIT 1
            "#,
            params![owner_user_id, direct_key],
            |row| {
                Ok(DirectRoom {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    name: row.get(2)?,
                    created_at_unix_ms: row.get(3)?,
                    updated_at_unix_ms: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|_| store_error())
}

pub(super) fn open_database(path: &Path, read_only: bool) -> Result<Connection, String> {
    let flags = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };
    let connection = Connection::open_with_flags(path, flags).map_err(|_| store_error())?;
    connection
        .busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| store_error())?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|_| store_error())?;
    Ok(connection)
}

pub(super) fn confined_connector_directory(app_data_directory: &Path) -> Result<PathBuf, String> {
    connector_storage::confined_connector_directory(app_data_directory).map_err(|_| store_error())
}

pub(super) fn reject_link_or_non_file(path: &Path) -> Result<(), String> {
    connector_storage::reject_link_or_non_file(path).map_err(|_| store_error())
}
