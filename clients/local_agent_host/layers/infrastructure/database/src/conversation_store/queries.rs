fn map_turn_insert_error(error: sqlx::Error, conversation_id: &str) -> ClientStorageError {
    if is_unique_violation(&error) {
        ClientStorageError::Conflict(format!(
            "conversation already has an active Turn or an id was reused: {conversation_id}"
        ))
    } else {
        ClientStorageError::database(error)
    }
}

pub(super) async fn next_message_ordinal(
    connection: &mut SqliteConnection,
    conversation_id: &str,
) -> Result<i64, ClientStorageError> {
    let current = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(ordinal), 0) FROM local_conversation_messages \
         WHERE conversation_id = ?",
    )
    .bind(conversation_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    current
        .checked_add(1)
        .ok_or_else(|| ClientStorageError::InvalidState("message ordinal overflow".to_string()))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn insert_user_message_on(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    turn_id: &str,
    message_id: &str,
    message: &str,
    message_metadata: &serde_json::Value,
    attachments: &[LocalConversationAttachmentSpec],
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let ordinal = next_message_ordinal(connection, conversation_id).await?;
    let inserted = sqlx::query(
        "INSERT INTO local_conversation_messages(\
         message_id, conversation_id, turn_id, ordinal, role, content_json, \
         metadata_json, created_at_unix_ms) VALUES(?, ?, ?, ?, 'user', ?, ?, ?)",
    )
    .bind(message_id)
    .bind(conversation_id)
    .bind(turn_id)
    .bind(ordinal)
    .bind(serde_json::to_string(&json!({"text": message}))?)
    .bind(serde_json::to_string(message_metadata)?)
    .bind(now_unix_ms)
    .execute(&mut *connection)
    .await;
    if let Err(error) = inserted {
        return Err(if is_unique_violation(&error) {
            ClientStorageError::Conflict(format!("message id already exists: {message_id}"))
        } else {
            ClientStorageError::database(error)
        });
    }
    for (index, attachment) in attachments.iter().enumerate() {
        let attachment_ordinal = i64::try_from(index + 1).map_err(|_| {
            ClientStorageError::InvalidState("attachment ordinal exceeds i64".to_string())
        })?;
        let byte_size = i64::try_from(attachment.byte_size).map_err(|_| {
            ClientStorageError::InvalidState("attachment byte_size exceeds i64".to_string())
        })?;
        let inserted = sqlx::query(
            "INSERT INTO local_conversation_message_attachments(\
             attachment_id, conversation_id, turn_id, message_id, ordinal, display_name, \
             media_type, byte_size, sha256, authorized_local_ref, metadata_json, \
             created_at_unix_ms) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&attachment.attachment_id)
        .bind(conversation_id)
        .bind(turn_id)
        .bind(message_id)
        .bind(attachment_ordinal)
        .bind(&attachment.display_name)
        .bind(&attachment.media_type)
        .bind(byte_size)
        .bind(&attachment.sha256)
        .bind(&attachment.authorized_local_ref)
        .bind(serde_json::to_string(&attachment.metadata)?)
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await;
        if let Err(error) = inserted {
            return Err(if is_unique_violation(&error) {
                ClientStorageError::Conflict(format!(
                    "attachment id already exists: {}",
                    attachment.attachment_id
                ))
            } else {
                ClientStorageError::database(error)
            });
        }
    }
    Ok(())
}

pub(super) async fn fetch_conversation(
    connection: &mut SqliteConnection,
    conversation_id: &str,
) -> Result<Option<LocalConversationDetail>, ClientStorageError> {
    let Some(conversation) = fetch_conversation_record(connection, conversation_id).await? else {
        return Ok(None);
    };
    let turns = sqlx::query(
        "SELECT turn_id, conversation_id, user_message_id, run_id, status, \
         created_at_unix_ms, updated_at_unix_ms FROM local_conversation_turns \
         WHERE conversation_id = ? ORDER BY created_at_unix_ms, turn_id",
    )
    .bind(conversation_id)
    .fetch_all(&mut *connection)
    .await
    .db()?
    .into_iter()
    .map(decode_turn)
    .collect::<Result<Vec<_>, _>>()?;
    let messages = sqlx::query(
        "SELECT message_id, conversation_id, turn_id, ordinal, role, content_json, \
         metadata_json, created_at_unix_ms FROM local_conversation_messages \
         WHERE conversation_id = ? ORDER BY ordinal",
    )
    .bind(conversation_id)
    .fetch_all(&mut *connection)
    .await
    .db()?
    .into_iter()
    .map(decode_message)
    .collect::<Result<Vec<_>, _>>()?;
    let attachments = sqlx::query(
        "SELECT a.attachment_id, a.conversation_id, a.turn_id, a.message_id, a.ordinal, \
         a.display_name, a.media_type, a.byte_size, a.sha256, a.authorized_local_ref, \
         a.metadata_json, a.created_at_unix_ms \
         FROM local_conversation_message_attachments a \
         JOIN local_conversation_messages m ON m.message_id = a.message_id \
         WHERE a.conversation_id = ? ORDER BY m.ordinal, a.ordinal",
    )
    .bind(conversation_id)
    .fetch_all(&mut *connection)
    .await
    .db()?
    .into_iter()
    .map(decode_attachment)
    .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(LocalConversationDetail {
        conversation,
        turns,
        messages,
        attachments,
    }))
}

pub(super) async fn fetch_conversation_record(
    connection: &mut SqliteConnection,
    conversation_id: &str,
) -> Result<Option<LocalConversationRecord>, ClientStorageError> {
    sqlx::query(
        "SELECT conversation_id, owner_user_id, title, resource_kind, resource_id, version, created_at_unix_ms, \
         updated_at_unix_ms FROM local_conversations WHERE conversation_id = ?",
    )
    .bind(conversation_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .map(decode_conversation)
    .transpose()
}

pub(super) async fn fetch_conversation_record_for_owner(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<Option<LocalConversationRecord>, ClientStorageError> {
    sqlx::query(
        "SELECT conversation_id, owner_user_id, title, resource_kind, resource_id, version, created_at_unix_ms, \
         updated_at_unix_ms FROM local_conversations \
         WHERE conversation_id = ? AND owner_user_id = ?",
    )
    .bind(conversation_id)
    .bind(owner_user_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .map(decode_conversation)
    .transpose()
}

async fn fetch_conversation_context_for_owner(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<Option<LocalConversationDetail>, ClientStorageError> {
    let Some(conversation) =
        fetch_conversation_record_for_owner(connection, owner_user_id, conversation_id).await?
    else {
        return Ok(None);
    };
    let turns = sqlx::query(
        "SELECT turn_id, conversation_id, user_message_id, run_id, status, \
         created_at_unix_ms, updated_at_unix_ms FROM local_conversation_turns \
         WHERE conversation_id = ? ORDER BY created_at_unix_ms, turn_id",
    )
    .bind(conversation_id)
    .fetch_all(&mut *connection)
    .await
    .db()?
    .into_iter()
    .map(decode_turn)
    .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(LocalConversationDetail {
        conversation,
        turns,
        messages: Vec::new(),
        attachments: Vec::new(),
    }))
}

async fn fetch_conversation_for_owner(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    conversation_id: &str,
) -> Result<Option<LocalConversationDetail>, ClientStorageError> {
    if fetch_conversation_record_for_owner(connection, owner_user_id, conversation_id)
        .await?
        .is_none()
    {
        return Ok(None);
    }
    fetch_conversation(connection, conversation_id).await
}
