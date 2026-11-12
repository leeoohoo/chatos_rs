// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    conversation_store::{
        fetch_conversation, fetch_conversation_record_for_owner, insert_user_message_on,
    },
    memory_outbox_store, run_commands, ClientStorageError, IdempotentCommand, SqliteClientStorage,
    SqliteResultExt,
};
use chatos_local_agent_protocol::{
    CancelConversationTurnCommand, GuideConversationTurnCommand, LocalAgentRunRecord,
    LocalConversationAttachmentSpec, LocalConversationTurnUpdate, ResumeConversationTurnCommand,
    LOCAL_AGENT_MAX_INPUT_BYTES,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};

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
        let expected_run_version = i64::try_from(turn.expected_run_version)
            .map_err(|_| ClientStorageError::InvalidState("run version exceeds i64".to_string()))?;
        let updated = sqlx::query(
            "UPDATE local_agent_runs SET status = 'continuation_ready', version = version + 1, \
             terminal_outcome_json = NULL, continuation_input_json = ?, updated_at_unix_ms = ? \
             WHERE run_id = ? AND status = ? AND version = ?",
        )
        .bind(serde_json::to_string(continuation_input)?)
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
            continuation_input,
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
    let continuation_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(length(CAST(continuation_input_json AS BLOB)), 0) \
         FROM local_agent_runs WHERE run_id = ?",
    )
    .bind(run_id)
    .fetch_one(&mut *connection)
    .await
    .db()?;
    let continuation_bytes = usize::try_from(continuation_bytes).map_err(|_| {
        ClientStorageError::InvalidState("Run continuation size is invalid".to_string())
    })?;
    if pending_bytes
        .saturating_add(continuation_bytes)
        .saturating_add(additional_bytes)
        > LOCAL_AGENT_MAX_INPUT_BYTES
    {
        return Err(ClientStorageError::Conflict(format!(
            "pending guidance exceeds the {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
        )));
    }
    Ok(())
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

#[allow(clippy::too_many_arguments)]
async fn project_user_message_to_memory(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    conversation_id: &str,
    turn_id: &str,
    message_id: &str,
    message: &str,
    message_metadata: &Value,
    attachments: &[LocalConversationAttachmentSpec],
    created_at_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    project_conversation_message_to_memory(
        connection,
        run,
        conversation_id,
        turn_id,
        message_id,
        "user",
        &Value::String(message.to_string()),
        message_metadata,
        attachments,
        "conversation_continuation",
        "pending",
        false,
        false,
        created_at_unix_ms,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn project_conversation_message_to_memory(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    conversation_id: &str,
    turn_id: &str,
    message_id: &str,
    role: &str,
    content: &Value,
    message_metadata: &Value,
    attachments: &[LocalConversationAttachmentSpec],
    message_source: &str,
    summary_status: &str,
    structured_content: bool,
    mutable: bool,
    created_at_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let source_id = sqlx::query_scalar::<_, String>(
        "SELECT source_id FROM local_memory_projection_config WHERE owner_user_id = ?",
    )
    .bind(&run.owner_user_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(source_id) = source_id else {
        return Ok(());
    };
    let agent_id = consistent_run_identity(&run.input, "agent_id")?.ok_or_else(|| {
        ClientStorageError::InvalidState(
            "conversation Memory projection requires agent_id".to_string(),
        )
    })?;
    let agent_name = consistent_run_identity(&run.input, "agent_name")?;
    let project_id = consistent_run_identity(&run.input, "project_id")?;
    let project_name = consistent_run_identity(&run.input, "project_name")?;
    let is_task_execution = run.profile_key == "task_execution";
    let thread_kind = if is_task_execution {
        if consistent_run_identity(&run.input, "todo_id")?.is_some() {
            "todo_execution"
        } else {
            "agent_task"
        }
    } else if run.profile_key == "main_chat" && agent_id == "jiguli" {
        "main_chat"
    } else {
        "agent_conversation"
    };
    let thread_project_aggregation_id = is_task_execution.then(|| project_id.clone()).flatten();
    let thread_id = if run.profile_key == "task_execution" {
        run.input
            .get("memory_thread_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(run.owner_entity_id.as_str())
            .to_string()
    } else {
        format!("client-agent:agent:{agent_id}")
    };
    let created_at = DateTime::<Utc>::from_timestamp_millis(created_at_unix_ms)
        .ok_or_else(|| {
            ClientStorageError::InvalidState(
                "conversation Memory projection timestamp is invalid".to_string(),
            )
        })?
        .to_rfc3339();
    let mut metadata = message_metadata.as_object().cloned().unwrap_or_default();
    metadata.insert("tenant_id".to_string(), json!(run.owner_user_id));
    metadata.insert("conversation_id".to_string(), json!(conversation_id));
    metadata.insert(
        "conversation_title".to_string(),
        run.input
            .get("conversation_title")
            .cloned()
            .unwrap_or(Value::Null),
    );
    metadata.insert(
        "conversation_resource".to_string(),
        run.input
            .get("conversation_resource")
            .cloned()
            .unwrap_or(Value::Null),
    );
    metadata.insert("profile_key".to_string(), json!(run.profile_key));
    metadata.insert("agent_id".to_string(), json!(agent_id));
    metadata.insert("agent_name".to_string(), json!(agent_name));
    metadata.insert("project_id".to_string(), json!(project_id));
    metadata.insert("project_name".to_string(), json!(project_name));
    metadata.insert("memory_project_id".to_string(), json!(project_id));
    metadata.insert(
        "thread_project_aggregation_id".to_string(),
        json!(thread_project_aggregation_id),
    );
    metadata.insert("thread_kind".to_string(), json!(thread_kind));
    metadata.insert("run_id".to_string(), json!(run.run_id));
    metadata.insert(
        "owner_entity_type".to_string(),
        json!(run.owner_entity_type),
    );
    metadata.insert("owner_entity_id".to_string(), json!(run.owner_entity_id));
    metadata.insert("ui_message_id".to_string(), json!(message_id));
    metadata.insert("ui_message_metadata".to_string(), message_metadata.clone());
    metadata.insert("ui_attachments".to_string(), json!(attachments));
    if structured_content {
        metadata.insert("ui_structured_content".to_string(), json!(true));
    }
    if mutable {
        metadata.insert("memory_projection_mutable".to_string(), json!(true));
    }
    let display_content = content
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| {
            content
                .get("text")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| content.to_string());
    let payload = json!({
        "conversation_id": thread_id,
        "conversation_turn_id": turn_id,
        "message_id": message_id,
        "role": role,
        "content": display_content,
        "structured_payload": if structured_content { Some(content.clone()) } else { None },
        "metadata": metadata,
        "message_mode": if run.profile_key == "task_execution" { "task_run" } else { "main_chat" },
        "message_source": message_source,
        "summary_status": summary_status,
        "created_at": created_at,
    });
    memory_outbox_store::enqueue_on(
        connection,
        message_id,
        &run.owner_user_id,
        &source_id,
        payload
            .get("conversation_id")
            .and_then(Value::as_str)
            .expect("projection thread id"),
        &payload,
        created_at_unix_ms,
    )
    .await?;
    Ok(())
}

fn consistent_run_identity(input: &Value, key: &str) -> Result<Option<String>, ClientStorageError> {
    let values = [
        input.get(key),
        input
            .get("agent_execution_context")
            .and_then(|value| value.get(key)),
        input
            .get("agent_team_execution")
            .and_then(|value| value.get(key)),
        input
            .get("team_executor_context")
            .and_then(|value| value.get(key)),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>();
    let Some(value) = values.first() else {
        return Ok(None);
    };
    if values.iter().any(|candidate| candidate != value) {
        return Err(ClientStorageError::InvalidState(format!(
            "conversation Memory projection has conflicting {key} values"
        )));
    }
    Ok(Some((*value).to_string()))
}

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
