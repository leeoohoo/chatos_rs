pub(super) fn decode_conversation(
    row: SqliteRow,
) -> Result<LocalConversationRecord, ClientStorageError> {
    let resource_kind: Option<String> = row.try_get("resource_kind").db()?;
    let resource_id: Option<String> = row.try_get("resource_id").db()?;
    let resource = match (resource_kind.as_deref(), resource_id) {
        (None, None) => None,
        (Some("contact"), Some(resource_id)) => Some(
            chatos_local_agent_protocol::LocalConversationResourceBinding {
                kind: chatos_local_agent_protocol::LocalConversationResourceKind::Contact,
                resource_id,
            },
        ),
        (Some("project"), Some(resource_id)) => Some(
            chatos_local_agent_protocol::LocalConversationResourceBinding {
                kind: chatos_local_agent_protocol::LocalConversationResourceKind::Project,
                resource_id,
            },
        ),
        _ => {
            return Err(ClientStorageError::InvalidState(
                "Conversation resource binding is incomplete".to_string(),
            ))
        }
    };
    Ok(LocalConversationRecord {
        conversation_id: row.try_get("conversation_id").db()?,
        owner_user_id: row.try_get("owner_user_id").db()?,
        title: row.try_get("title").db()?,
        resource,
        version: decode_u64(&row, "version")?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

pub(super) fn decode_turn(
    row: SqliteRow,
) -> Result<LocalConversationTurnRecord, ClientStorageError> {
    let status: String = row.try_get("status").db()?;
    Ok(LocalConversationTurnRecord {
        turn_id: row.try_get("turn_id").db()?,
        conversation_id: row.try_get("conversation_id").db()?,
        user_message_id: row.try_get("user_message_id").db()?,
        run_id: row.try_get("run_id").db()?,
        status: LocalConversationTurnStatus::from_str(&status)
            .map_err(ClientStorageError::InvalidState)?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

pub(super) fn decode_message(
    row: SqliteRow,
) -> Result<LocalConversationMessageRecord, ClientStorageError> {
    let role: String = row.try_get("role").db()?;
    let content: String = row.try_get("content_json").db()?;
    let metadata: String = row.try_get("metadata_json").db()?;
    Ok(LocalConversationMessageRecord {
        message_id: row.try_get("message_id").db()?,
        conversation_id: row.try_get("conversation_id").db()?,
        turn_id: row.try_get("turn_id").db()?,
        ordinal: decode_u64(&row, "ordinal")?,
        role: LocalConversationMessageRole::from_str(&role)
            .map_err(ClientStorageError::InvalidState)?,
        content: serde_json::from_str(&content)?,
        metadata: serde_json::from_str(&metadata)?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
    })
}

pub(super) fn decode_attachment(
    row: SqliteRow,
) -> Result<LocalConversationAttachmentRecord, ClientStorageError> {
    let metadata: String = row.try_get("metadata_json").db()?;
    Ok(LocalConversationAttachmentRecord {
        attachment_id: row.try_get("attachment_id").db()?,
        conversation_id: row.try_get("conversation_id").db()?,
        turn_id: row.try_get("turn_id").db()?,
        message_id: row.try_get("message_id").db()?,
        ordinal: decode_u64(&row, "ordinal")?,
        display_name: row.try_get("display_name").db()?,
        media_type: row.try_get("media_type").db()?,
        byte_size: decode_u64(&row, "byte_size")?,
        sha256: row.try_get("sha256").db()?,
        authorized_local_ref: row.try_get("authorized_local_ref").db()?,
        metadata: serde_json::from_str(&metadata)?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
    })
}

fn decode_u64(row: &SqliteRow, field: &str) -> Result<u64, ClientStorageError> {
    let value: i64 = row.try_get(field).db()?;
    u64::try_from(value)
        .map_err(|_| ClientStorageError::InvalidState(format!("invalid {field}: {value}")))
}
