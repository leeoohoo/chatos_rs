#[allow(clippy::too_many_arguments)]
pub(super) async fn resume_conversation_turn(
    storage: &SqliteClientStorage,
    command: &IdempotentCommand,
    turn: &ResumeConversationTurnCommand,
    continuation_input: &Value,
    event_id: &str,
    now_unix_ms: i64,
) -> Result<LocalConversationTurnUpdate, ClientStorageError> {
    turn.validate().map_err(ClientStorageError::InvalidState)?;
    let mut connection = storage.acquire_write().await.db()?;
    SqliteClientStorage::begin_immediate(&mut connection).await?;
    let result = async {
        if let Some(replay) = SqliteClientStorage::replay(&mut connection, command).await? {
            return Ok(replay);
        }
        verify_conversation_version(
            &mut connection,
            &turn.owner_user_id,
            &turn.conversation_id,
            turn.expected_conversation_version,
        )
        .await?;
        let current = fetch_owned_turn_run_for_review_action(
            &mut connection,
            &turn.conversation_id,
            &turn.turn_id,
        )
        .await?;
        if current.version != turn.expected_run_version
            || current.status != turn.expected_run_status
        {
            return Err(ClientStorageError::Conflict(format!(
                "run status or version changed while resuming: {}",
                current.run_id
            )));
        }
        let continuation_input =
            super::run_continuation::for_resume(&mut connection, &current, continuation_input)
                .await?;
        let expected_run_version = i64::try_from(turn.expected_run_version)
            .map_err(|_| ClientStorageError::InvalidState("run version exceeds i64".to_string()))?;
        let updated = sqlx::query(
            "UPDATE local_agent_runs SET status = 'continuation_ready', version = version + 1, \
             terminal_outcome_json = NULL, continuation_input_json = ?, updated_at_unix_ms = ? \
             WHERE run_id = ? AND status = ? AND version = ?",
        )
        .bind(serde_json::to_string(&continuation_input)?)
        .bind(now_unix_ms)
        .bind(&current.run_id)
        .bind(turn.expected_run_status.as_str())
        .bind(expected_run_version)
        .execute(&mut *connection)
        .await
        .db()?;
        if updated.rows_affected() != 1 {
            return Err(ClientStorageError::Conflict(format!(
                "run changed while resuming: {}",
                current.run_id
            )));
        }
        // needs_review closes the visible Turn as failed. Reopen it in this same
        // transaction so the resumed Run can finish and publish its new result.
        sqlx::query(
            "UPDATE local_conversation_turns SET status = 'running', updated_at_unix_ms = ? \
             WHERE turn_id = ? AND run_id = ?",
        )
        .bind(now_unix_ms)
        .bind(&turn.turn_id)
        .bind(&current.run_id)
        .execute(&mut *connection)
        .await
        .db()?;
        insert_user_message_on(
            &mut connection,
            &turn.conversation_id,
            &turn.turn_id,
            &turn.message_id,
            &turn.message,
            &turn.message_metadata,
            &turn.attachments,
            now_unix_ms,
        )
        .await?;
        project_user_message_to_memory(
            &mut connection,
            &current,
            &turn.conversation_id,
            &turn.turn_id,
            &turn.message_id,
            &turn.message,
            &turn.message_metadata,
            &turn.attachments,
            now_unix_ms,
        )
        .await?;
        super::task_followup_gate::begin_review(
            &mut connection,
            &turn.conversation_id,
            &current.run_id,
            &turn.message_id,
            now_unix_ms,
        )
        .await?;
        increment_conversation_version(
            &mut connection,
            &turn.conversation_id,
            turn.expected_conversation_version,
            now_unix_ms,
        )
        .await?;
        SqliteClientStorage::insert_event(
            &mut connection,
            event_id,
            &current.run_id,
            "conversation_turn_resumed",
            &continuation_input,
            now_unix_ms,
        )
        .await?;
        let run = SqliteClientStorage::fetch_run_on(&mut connection, &current.run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(current.run_id.clone()))?;
        let response = build_update(
            &mut connection,
            &turn.conversation_id,
            &turn.turn_id,
            Some(&turn.message_id),
            run,
        )
        .await?;
        SqliteClientStorage::record_receipt(&mut connection, command, &response, now_unix_ms)
            .await?;
        Ok(response)
    }
    .await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}

pub(super) async fn cancel_conversation_turn(
    storage: &SqliteClientStorage,
    command: &IdempotentCommand,
    turn: &CancelConversationTurnCommand,
    event_id: &str,
    now_unix_ms: i64,
) -> Result<LocalConversationTurnUpdate, ClientStorageError> {
    turn.validate().map_err(ClientStorageError::InvalidState)?;
    let mut connection = storage.acquire_write().await.db()?;
    SqliteClientStorage::begin_immediate(&mut connection).await?;
    let result = async {
        if let Some(replay) = SqliteClientStorage::replay(&mut connection, command).await? {
            return Ok(replay);
        }
        verify_conversation_version(
            &mut connection,
            &turn.owner_user_id,
            &turn.conversation_id,
            turn.expected_conversation_version,
        )
        .await?;
        let current = fetch_owned_turn_run_for_review_action(
            &mut connection,
            &turn.conversation_id,
            &turn.turn_id,
        )
        .await?;
        let run = run_commands::cancel_run_on(
            &mut connection,
            &current.run_id,
            turn.expected_run_version,
            &turn.reason,
            event_id,
            now_unix_ms,
        )
        .await?;
        let response = build_update(
            &mut connection,
            &turn.conversation_id,
            &turn.turn_id,
            None,
            run,
        )
        .await?;
        SqliteClientStorage::record_receipt(&mut connection, command, &response, now_unix_ms)
            .await?;
        Ok(response)
    }
    .await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}

pub(super) async fn guide_conversation_turn(
    storage: &SqliteClientStorage,
    command: &IdempotentCommand,
    turn: &GuideConversationTurnCommand,
    event_id: &str,
    now_unix_ms: i64,
) -> Result<LocalConversationTurnUpdate, ClientStorageError> {
    turn.validate().map_err(ClientStorageError::InvalidState)?;
    let mut connection = storage.acquire_write().await.db()?;
    SqliteClientStorage::begin_immediate(&mut connection).await?;
    let result = async {
        if let Some(replay) = SqliteClientStorage::replay(&mut connection, command).await? {
            return Ok(replay);
        }
        verify_conversation_version(
            &mut connection,
            &turn.owner_user_id,
            &turn.conversation_id,
            turn.expected_conversation_version,
        )
        .await?;
        let current =
            fetch_owned_turn_run(&mut connection, &turn.conversation_id, &turn.turn_id).await?;
        if current.status.is_terminal() {
            return Err(ClientStorageError::Conflict(format!(
                "terminal Run cannot accept guidance: {}",
                current.run_id
            )));
        }
        if turn
            .expected_run_version
            .is_some_and(|version| version != current.version)
        {
            return Err(ClientStorageError::Conflict(format!(
                "run version changed while adding guidance: {}",
                current.run_id
            )));
        }
        let payload = serde_json::json!({
            "message_id": turn.message_id,
            "message": turn.message,
            "attachments": turn.attachments,
        });
        let payload_json = serde_json::to_string(&payload)?;
        enforce_pending_guidance_limit(&mut connection, &current.run_id, payload_json.len())
            .await?;
        insert_user_message_on(
            &mut connection,
            &turn.conversation_id,
            &turn.turn_id,
            &turn.message_id,
            &turn.message,
            &turn.message_metadata,
            &turn.attachments,
            now_unix_ms,
        )
        .await?;
        project_user_message_to_memory(
            &mut connection,
            &current,
            &turn.conversation_id,
            &turn.turn_id,
            &turn.message_id,
            &turn.message,
            &turn.message_metadata,
            &turn.attachments,
            now_unix_ms,
        )
        .await?;
        sqlx::query(
            "INSERT INTO local_conversation_guidance(\
             message_id, conversation_id, turn_id, run_id, payload_json, \
             delivered_run_version, created_at_unix_ms, delivered_at_unix_ms) \
             VALUES(?, ?, ?, ?, ?, NULL, ?, NULL)",
        )
        .bind(&turn.message_id)
        .bind(&turn.conversation_id)
        .bind(&turn.turn_id)
        .bind(&current.run_id)
        .bind(&payload_json)
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await
        .db()?;
        super::task_followup_gate::begin_review(
            &mut connection,
            &turn.conversation_id,
            &current.run_id,
            &turn.message_id,
            now_unix_ms,
        )
        .await?;
        let updated = sqlx::query(
            "UPDATE local_agent_runs SET \
             status = CASE WHEN status IN (\
               'model_running','waiting_user','retry_scheduled','paused','needs_review'\
             ) THEN 'continuation_ready' ELSE status END, \
             version = version + 1, \
             claim_token = CASE WHEN status = 'model_running' THEN NULL ELSE claim_token END, \
             claim_until_unix_ms = CASE \
               WHEN status = 'model_running' THEN NULL ELSE claim_until_unix_ms END, \
             next_attempt_at_unix_ms = CASE \
               WHEN status = 'retry_scheduled' THEN NULL ELSE next_attempt_at_unix_ms END, \
             updated_at_unix_ms = ? WHERE run_id = ? AND version = ?",
        )
        .bind(now_unix_ms)
        .bind(&current.run_id)
        .bind(current.version as i64)
        .execute(&mut *connection)
        .await
        .db()?;
        if updated.rows_affected() != 1 {
            return Err(ClientStorageError::Conflict(format!(
                "run changed while adding guidance: {}",
                current.run_id
            )));
        }
        increment_conversation_version(
            &mut connection,
            &turn.conversation_id,
            turn.expected_conversation_version,
            now_unix_ms,
        )
        .await?;
        SqliteClientStorage::insert_event(
            &mut connection,
            event_id,
            &current.run_id,
            "conversation_guidance_queued",
            &serde_json::json!({
                "conversation_id": turn.conversation_id,
                "turn_id": turn.turn_id,
                "message_id": turn.message_id,
            }),
            now_unix_ms,
        )
        .await?;
        let run = SqliteClientStorage::fetch_run_on(&mut connection, &current.run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(current.run_id.clone()))?;
        let response = build_update(
            &mut connection,
            &turn.conversation_id,
            &turn.turn_id,
            Some(&turn.message_id),
            run,
        )
        .await?;
        SqliteClientStorage::record_receipt(&mut connection, command, &response, now_unix_ms)
            .await?;
        Ok(response)
    }
    .await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}
