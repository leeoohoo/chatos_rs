async fn increment_conversation_version(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    expected_version: u64,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let expected_version = i64::try_from(expected_version).map_err(|_| {
        ClientStorageError::InvalidState("conversation version exceeds i64".to_string())
    })?;
    let updated = sqlx::query(
        "UPDATE local_conversations SET version = version + 1, updated_at_unix_ms = ? \
         WHERE conversation_id = ? AND version = ?",
    )
    .bind(now_unix_ms)
    .bind(conversation_id)
    .bind(expected_version)
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "conversation changed while updating Turn: {conversation_id}"
        )));
    }
    Ok(())
}

async fn build_update(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    turn_id: &str,
    message_id: Option<&str>,
    run: LocalAgentRunRecord,
) -> Result<LocalConversationTurnUpdate, ClientStorageError> {
    let detail = fetch_conversation(connection, conversation_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(conversation_id.to_string()))?;
    Ok(LocalConversationTurnUpdate {
        conversation: detail.conversation,
        turn: detail
            .turns
            .into_iter()
            .find(|record| record.turn_id == turn_id)
            .ok_or_else(|| ClientStorageError::NotFound(turn_id.to_string()))?,
        message: message_id
            .map(|message_id| {
                detail
                    .messages
                    .into_iter()
                    .find(|record| record.message_id == message_id)
                    .ok_or_else(|| ClientStorageError::NotFound(message_id.to_string()))
            })
            .transpose()?,
        attachments: message_id
            .map(|message_id| {
                detail
                    .attachments
                    .into_iter()
                    .filter(|record| record.message_id == message_id)
                    .collect()
            })
            .unwrap_or_default(),
        run,
    })
}
