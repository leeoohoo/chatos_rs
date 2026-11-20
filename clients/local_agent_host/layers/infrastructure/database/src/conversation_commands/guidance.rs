async fn enforce_pending_guidance_limit(
    connection: &mut SqliteConnection,
    run_id: &str,
    additional_bytes: usize,
) -> Result<(), ClientStorageError> {
    let pending_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(length(CAST(payload_json AS BLOB))), 0) \
         FROM local_conversation_guidance \
         WHERE run_id = ? AND delivered_run_version IS NULL",
    )
    .bind(run_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    let pending_bytes = usize::try_from(pending_bytes).map_err(|_| {
        ClientStorageError::InvalidState("pending guidance size is invalid".to_string())
    })?;
    let continuation: Option<String> =
        sqlx::query_scalar("SELECT continuation_input_json FROM local_agent_runs WHERE run_id = ?")
            .bind(run_id)
            .fetch_one(&mut *connection)
            .await
            .db()?;
    let continuation_guidance_bytes = continuation_guidance_bytes(continuation.as_deref())?;
    if pending_bytes
        .saturating_add(continuation_guidance_bytes)
        .saturating_add(additional_bytes)
        > LOCAL_AGENT_MAX_INPUT_BYTES
    {
        return Err(ClientStorageError::Conflict(format!(
            "pending guidance exceeds the {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
        )));
    }
    Ok(())
}

fn continuation_guidance_bytes(continuation: Option<&str>) -> Result<usize, ClientStorageError> {
    let Some(continuation) = continuation else {
        return Ok(0);
    };
    let continuation: Value = serde_json::from_str(continuation)?;
    let Some(guidance) = continuation.get("guidance") else {
        return Ok(0);
    };
    let guidance = guidance.as_array().ok_or_else(|| {
        ClientStorageError::InvalidState("Run continuation guidance must be an array".to_string())
    })?;
    guidance.iter().try_fold(0usize, |total, item| {
        Ok(total.saturating_add(serde_json::to_vec(item)?.len()))
    })
}

async fn verify_conversation_version(
    connection: &mut SqliteConnection,
    owner_user_id: &str,
    conversation_id: &str,
    expected_version: u64,
) -> Result<(), ClientStorageError> {
    let conversation =
        fetch_conversation_record_for_owner(connection, owner_user_id, conversation_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(conversation_id.to_string()))?;
    if conversation.version != expected_version {
        return Err(ClientStorageError::Conflict(format!(
            "conversation version changed: {conversation_id}"
        )));
    }
    Ok(())
}

pub(super) async fn fetch_owned_turn_run(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    turn_id: &str,
) -> Result<LocalAgentRunRecord, ClientStorageError> {
    let row = sqlx::query(
        "SELECT run_id, status FROM local_conversation_turns \
         WHERE turn_id = ? AND conversation_id = ?",
    )
    .bind(turn_id)
    .bind(conversation_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .ok_or_else(|| ClientStorageError::NotFound(turn_id.to_string()))?;
    let turn_status: String = row.try_get("status").db()?;
    if turn_status != "running" {
        return Err(ClientStorageError::Conflict(format!(
            "conversation Turn is already terminal: {turn_id}"
        )));
    }
    let run_id: String = row.try_get("run_id").db()?;
    let run = SqliteClientStorage::fetch_run_on(connection, &run_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
    if run.owner_entity_type != "conversation_turn" || run.owner_entity_id != turn_id {
        return Err(ClientStorageError::InvalidState(format!(
            "Run ownership does not match conversation Turn: {turn_id}"
        )));
    }
    Ok(run)
}

async fn fetch_owned_turn_run_for_review_action(
    connection: &mut SqliteConnection,
    conversation_id: &str,
    turn_id: &str,
) -> Result<LocalAgentRunRecord, ClientStorageError> {
    let row = sqlx::query(
        "SELECT run_id, status FROM local_conversation_turns \
         WHERE turn_id = ? AND conversation_id = ?",
    )
    .bind(turn_id)
    .bind(conversation_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .ok_or_else(|| ClientStorageError::NotFound(turn_id.to_string()))?;
    let run_id: String = row.try_get("run_id").db()?;
    let turn_status: String = row.try_get("status").db()?;
    let run = SqliteClientStorage::fetch_run_on(connection, &run_id)
        .await?
        .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
    let can_act_on_reviewed_turn = turn_status == "failed"
        && run.status == chatos_local_agent_protocol::LocalAgentRunStatus::NeedsReview;
    if turn_status != "running" && !can_act_on_reviewed_turn {
        return Err(ClientStorageError::Conflict(format!(
            "conversation Turn is already terminal: {turn_id}"
        )));
    }
    if run.owner_entity_type != "conversation_turn" || run.owner_entity_id != turn_id {
        return Err(ClientStorageError::InvalidState(format!(
            "Run ownership does not match conversation Turn: {turn_id}"
        )));
    }
    Ok(run)
}
