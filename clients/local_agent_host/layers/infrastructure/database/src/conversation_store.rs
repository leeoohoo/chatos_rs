// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    is_unique_violation, ClientStorageError, IdempotentCommand, LocalConversationStore,
    SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CreateConversationCommand, LocalAgentRunRecord, LocalConversationDetail,
    LocalConversationMessageRecord, LocalConversationMessageRole, LocalConversationRecord,
    LocalConversationTurnRecord, LocalConversationTurnStart, LocalConversationTurnStatus,
    StartConversationTurnCommand,
};
use serde_json::json;
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};
use std::str::FromStr;

#[async_trait]
impl LocalConversationStore for SqliteClientStorage {
    async fn create_conversation(
        &self,
        command: &IdempotentCommand,
        conversation: &CreateConversationCommand,
        now_unix_ms: i64,
    ) -> Result<LocalConversationDetail, ClientStorageError> {
        conversation
            .validate()
            .map_err(ClientStorageError::InvalidState)?;
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            let inserted = sqlx::query(
                "INSERT INTO local_conversations(\
                 conversation_id, owner_user_id, title, version, created_at_unix_ms, \
                 updated_at_unix_ms) VALUES(?, ?, ?, 1, ?, ?)",
            )
            .bind(&conversation.conversation_id)
            .bind(&conversation.owner_user_id)
            .bind(conversation.title.trim())
            .bind(now_unix_ms)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await;
            if let Err(error) = inserted {
                return Err(if is_unique_violation(&error) {
                    ClientStorageError::Conflict(format!(
                        "conversation already exists: {}",
                        conversation.conversation_id
                    ))
                } else {
                    ClientStorageError::database(error)
                });
            }
            let detail = fetch_conversation(&mut connection, &conversation.conversation_id)
                .await?
                .ok_or_else(|| {
                    ClientStorageError::NotFound(conversation.conversation_id.clone())
                })?;
            Self::record_receipt(&mut connection, command, &detail, now_unix_ms).await?;
            Ok(detail)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn get_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Option<LocalConversationDetail>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        fetch_conversation(&mut connection, conversation_id).await
    }

    async fn list_conversations(
        &self,
        owner_user_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalConversationRecord>, ClientStorageError> {
        if !(1..=200).contains(&limit) {
            return Err(ClientStorageError::InvalidState(
                "conversation limit must be between 1 and 200".to_string(),
            ));
        }
        let mut connection = self.pool.acquire().await.db()?;
        sqlx::query(
            "SELECT conversation_id, owner_user_id, title, version, created_at_unix_ms, \
             updated_at_unix_ms FROM local_conversations WHERE owner_user_id = ? \
             ORDER BY updated_at_unix_ms DESC, conversation_id LIMIT ?",
        )
        .bind(owner_user_id)
        .bind(i64::from(limit))
        .fetch_all(&mut *connection)
        .await
        .db()?
        .into_iter()
        .map(decode_conversation)
        .collect()
    }

    async fn start_conversation_turn(
        &self,
        command: &IdempotentCommand,
        turn: &StartConversationTurnCommand,
        run: &LocalAgentRunRecord,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalConversationTurnStart, ClientStorageError> {
        turn.validate().map_err(ClientStorageError::InvalidState)?;
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            let conversation = fetch_conversation_record(&mut connection, &turn.conversation_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(turn.conversation_id.clone()))?;
            if conversation.version != turn.expected_conversation_version {
                return Err(ClientStorageError::Conflict(format!(
                    "conversation version changed: {}",
                    turn.conversation_id
                )));
            }
            if run.owner_user_id != conversation.owner_user_id
                || run.owner_entity_type != "conversation_turn"
                || run.owner_entity_id != turn.turn_id
                || run.run_id != turn.run_id
            {
                return Err(ClientStorageError::InvalidState(
                    "conversation Run ownership does not match the Turn".to_string(),
                ));
            }
            Self::insert_run_on(&mut connection, run).await?;
            sqlx::query(
                "INSERT INTO local_conversation_turns(\
                 turn_id, conversation_id, user_message_id, run_id, status, \
                 created_at_unix_ms, updated_at_unix_ms) VALUES(?, ?, ?, ?, 'running', ?, ?)",
            )
            .bind(&turn.turn_id)
            .bind(&turn.conversation_id)
            .bind(&turn.message_id)
            .bind(&turn.run_id)
            .bind(now_unix_ms)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await
            .map_err(|error| map_turn_insert_error(error, &turn.conversation_id))?;
            let ordinal = next_message_ordinal(&mut connection, &turn.conversation_id).await?;
            sqlx::query(
                "INSERT INTO local_conversation_messages(\
                 message_id, conversation_id, turn_id, ordinal, role, content_json, \
                 metadata_json, created_at_unix_ms) VALUES(?, ?, ?, ?, 'user', ?, ?, ?)",
            )
            .bind(&turn.message_id)
            .bind(&turn.conversation_id)
            .bind(&turn.turn_id)
            .bind(ordinal)
            .bind(serde_json::to_string(&json!({"text": turn.message}))?)
            .bind(serde_json::to_string(&turn.message_metadata)?)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await
            .db()?;
            let updated = sqlx::query(
                "UPDATE local_conversations SET version = version + 1, updated_at_unix_ms = ? \
                 WHERE conversation_id = ? AND version = ?",
            )
            .bind(now_unix_ms)
            .bind(&turn.conversation_id)
            .bind(
                i64::try_from(turn.expected_conversation_version).map_err(|_| {
                    ClientStorageError::InvalidState("conversation version exceeds i64".to_string())
                })?,
            )
            .execute(&mut *connection)
            .await
            .db()?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "conversation changed while starting Turn: {}",
                    turn.conversation_id
                )));
            }
            Self::insert_event(
                &mut connection,
                event_id,
                &turn.run_id,
                "conversation_turn_started",
                &json!({
                    "conversation_id": turn.conversation_id,
                    "turn_id": turn.turn_id,
                    "message_id": turn.message_id
                }),
                now_unix_ms,
            )
            .await?;
            let detail = fetch_conversation(&mut connection, &turn.conversation_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(turn.conversation_id.clone()))?;
            let started = LocalConversationTurnStart {
                conversation: detail.conversation,
                turn: detail
                    .turns
                    .into_iter()
                    .find(|record| record.turn_id == turn.turn_id)
                    .ok_or_else(|| ClientStorageError::NotFound(turn.turn_id.clone()))?,
                message: detail
                    .messages
                    .into_iter()
                    .find(|record| record.message_id == turn.message_id)
                    .ok_or_else(|| ClientStorageError::NotFound(turn.message_id.clone()))?,
                run: run.clone(),
            };
            Self::record_receipt(&mut connection, command, &started, now_unix_ms).await?;
            Ok(started)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }
}

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

async fn fetch_conversation(
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
    Ok(Some(LocalConversationDetail {
        conversation,
        turns,
        messages,
    }))
}

async fn fetch_conversation_record(
    connection: &mut SqliteConnection,
    conversation_id: &str,
) -> Result<Option<LocalConversationRecord>, ClientStorageError> {
    sqlx::query(
        "SELECT conversation_id, owner_user_id, title, version, created_at_unix_ms, \
         updated_at_unix_ms FROM local_conversations WHERE conversation_id = ?",
    )
    .bind(conversation_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .map(decode_conversation)
    .transpose()
}

fn decode_conversation(row: SqliteRow) -> Result<LocalConversationRecord, ClientStorageError> {
    Ok(LocalConversationRecord {
        conversation_id: row.try_get("conversation_id").db()?,
        owner_user_id: row.try_get("owner_user_id").db()?,
        title: row.try_get("title").db()?,
        version: decode_u64(&row, "version")?,
        created_at_unix_ms: row.try_get("created_at_unix_ms").db()?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms").db()?,
    })
}

fn decode_turn(row: SqliteRow) -> Result<LocalConversationTurnRecord, ClientStorageError> {
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

fn decode_message(row: SqliteRow) -> Result<LocalConversationMessageRecord, ClientStorageError> {
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

fn decode_u64(row: &SqliteRow, field: &str) -> Result<u64, ClientStorageError> {
    let value: i64 = row.try_get(field).db()?;
    u64::try_from(value)
        .map_err(|_| ClientStorageError::InvalidState(format!("invalid {field}: {value}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_local_agent_ports::LocalAgentRunStore;
    use chatos_local_agent_protocol::LocalAgentRunStatus;
    use serde_json::{json, Value};

    fn idempotency(command_id: &str) -> IdempotentCommand {
        IdempotentCommand {
            command_id: command_id.to_string(),
            request_fingerprint: command_id.to_string(),
        }
    }

    fn conversation(conversation_id: &str) -> CreateConversationCommand {
        CreateConversationCommand {
            conversation_id: conversation_id.to_string(),
            owner_user_id: "user-1".to_string(),
            title: "Local conversation".to_string(),
        }
    }

    fn turn(
        conversation_id: &str,
        expected_conversation_version: u64,
        suffix: &str,
    ) -> StartConversationTurnCommand {
        StartConversationTurnCommand {
            conversation_id: conversation_id.to_string(),
            expected_conversation_version,
            turn_id: format!("turn-{suffix}"),
            message_id: format!("message-{suffix}"),
            run_id: format!("run-{suffix}"),
            message: format!("hello {suffix}"),
            message_metadata: json!({"source": "test"}),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            max_iterations: 8,
        }
    }

    fn run(turn: &StartConversationTurnCommand, now: i64) -> LocalAgentRunRecord {
        LocalAgentRunRecord {
            run_id: turn.run_id.clone(),
            owner_user_id: "user-1".to_string(),
            owner_entity_type: "conversation_turn".to_string(),
            owner_entity_id: turn.turn_id.clone(),
            profile_key: "main_chat".to_string(),
            model_config_ref: turn.model_config_ref.clone(),
            model_config_revision: turn.model_config_revision.clone(),
            capability_policy_revision: turn.capability_policy_revision.clone(),
            input: json!({"message": turn.message}),
            status: LocalAgentRunStatus::Queued,
            iteration: 0,
            model_attempt: 1,
            max_iterations: turn.max_iterations,
            version: 1,
            claim_token: None,
            claim_until_unix_ms: None,
            next_attempt_at_unix_ms: None,
            pending_tool_batch: None,
            checkpoint: Value::Null,
            continuation_input: None,
            terminal_outcome: None,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
        }
    }

    async fn create(
        storage: &SqliteClientStorage,
        conversation_id: &str,
    ) -> LocalConversationDetail {
        storage
            .create_conversation(
                &idempotency(&format!("create-{conversation_id}")),
                &conversation(conversation_id),
                1_000,
            )
            .await
            .expect("create conversation")
    }

    async fn start(
        storage: &SqliteClientStorage,
        turn: &StartConversationTurnCommand,
        now: i64,
    ) -> Result<LocalConversationTurnStart, ClientStorageError> {
        storage
            .start_conversation_turn(
                &idempotency(&format!("start-{}", turn.turn_id)),
                turn,
                &run(turn, now),
                &format!("event-{}", turn.turn_id),
                now,
            )
            .await
    }

    #[tokio::test]
    async fn turn_start_is_atomic_and_rejects_stale_or_concurrent_writes() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        create(&storage, "conversation-1").await;

        let stale = turn("conversation-1", 2, "stale");
        assert!(matches!(
            start(&storage, &stale, 2_000).await,
            Err(ClientStorageError::Conflict(_))
        ));
        assert!(storage
            .get_run(&stale.run_id)
            .await
            .expect("get stale Run")
            .is_none());
        let after_stale = storage
            .get_conversation("conversation-1")
            .await
            .expect("get conversation")
            .expect("conversation");
        assert!(after_stale.turns.is_empty());
        assert!(after_stale.messages.is_empty());

        let first = turn("conversation-1", 1, "first");
        let started = start(&storage, &first, 3_000).await.expect("start first");
        assert_eq!(started.conversation.version, 2);
        assert_eq!(started.turn.status, LocalConversationTurnStatus::Running);
        assert_eq!(started.message.ordinal, 1);
        assert_eq!(started.run.status, LocalAgentRunStatus::Queued);

        let concurrent = turn("conversation-1", 2, "concurrent");
        assert!(matches!(
            start(&storage, &concurrent, 4_000).await,
            Err(ClientStorageError::Conflict(_))
        ));
        assert!(storage
            .get_run(&concurrent.run_id)
            .await
            .expect("get concurrent Run")
            .is_none());
    }

    #[tokio::test]
    async fn terminal_runs_reconcile_turns_and_assistant_messages() {
        let storage = SqliteClientStorage::connect_memory()
            .await
            .expect("storage");
        create(&storage, "conversation-success").await;
        let success_turn = turn("conversation-success", 1, "success");
        start(&storage, &success_turn, 2_000)
            .await
            .expect("start success Turn");
        let claim = storage
            .claim_next_run(
                &idempotency("claim-success"),
                "worker-1",
                "token-success",
                3_000,
                13_000,
                "event-claim-success",
            )
            .await
            .expect("claim")
            .expect("claimed Run");
        storage
            .apply_transition(
                &idempotency("complete-success"),
                &chatos_local_agent_ports::RunTransition {
                    run_id: claim.run.run_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    expected_status: LocalAgentRunStatus::ModelRunning,
                    next_status: LocalAgentRunStatus::Succeeded,
                    next_model_attempt: 1,
                    next_attempt_at_unix_ms: None,
                    pending_tool_batch: None,
                    tool_batch: None,
                    checkpoint: None,
                    clear_continuation_input: true,
                    terminal_outcome: Some(json!({"answer": "done"})),
                    event_id: "event-success".to_string(),
                    event_type: "run_succeeded".to_string(),
                    event_payload: json!({"answer": "done"}),
                    occurred_at_unix_ms: 4_000,
                },
            )
            .await
            .expect("complete Run");
        let succeeded = storage
            .get_conversation("conversation-success")
            .await
            .expect("get success conversation")
            .expect("success conversation");
        assert_eq!(succeeded.conversation.version, 3);
        assert_eq!(
            succeeded.turns[0].status,
            LocalConversationTurnStatus::Succeeded
        );
        assert_eq!(succeeded.messages.len(), 2);
        assert_eq!(
            succeeded.messages[1].role,
            LocalConversationMessageRole::Assistant
        );
        assert_eq!(succeeded.messages[1].content, json!({"answer": "done"}));

        create(&storage, "conversation-cancel").await;
        let cancel_turn = turn("conversation-cancel", 1, "cancel");
        let started = start(&storage, &cancel_turn, 5_000)
            .await
            .expect("start cancelled Turn");
        storage
            .cancel_run(
                &idempotency("cancel-run"),
                &started.run.run_id,
                Some(started.run.version),
                "user cancelled",
                "event-cancel",
                6_000,
            )
            .await
            .expect("cancel Run");
        let cancelled = storage
            .get_conversation("conversation-cancel")
            .await
            .expect("get cancelled conversation")
            .expect("cancelled conversation");
        assert_eq!(cancelled.conversation.version, 3);
        assert_eq!(
            cancelled.turns[0].status,
            LocalConversationTurnStatus::Cancelled
        );
        assert_eq!(cancelled.messages.len(), 1);

        create(&storage, "conversation-failure").await;
        let failure_turn = turn("conversation-failure", 1, "failure");
        start(&storage, &failure_turn, 7_000)
            .await
            .expect("start failed Turn");
        let claim = storage
            .claim_next_run(
                &idempotency("claim-failure"),
                "worker-1",
                "token-failure",
                8_000,
                18_000,
                "event-claim-failure",
            )
            .await
            .expect("claim")
            .expect("claimed Run");
        storage
            .apply_transition(
                &idempotency("complete-failure"),
                &chatos_local_agent_ports::RunTransition {
                    run_id: claim.run.run_id,
                    claim_token: claim.claim_token,
                    expected_version: claim.run.version,
                    expected_status: LocalAgentRunStatus::ModelRunning,
                    next_status: LocalAgentRunStatus::Failed,
                    next_model_attempt: 1,
                    next_attempt_at_unix_ms: None,
                    pending_tool_batch: None,
                    tool_batch: None,
                    checkpoint: None,
                    clear_continuation_input: true,
                    terminal_outcome: Some(json!({"error": "provider rejected request"})),
                    event_id: "event-failure".to_string(),
                    event_type: "run_failed".to_string(),
                    event_payload: json!({"error": "provider rejected request"}),
                    occurred_at_unix_ms: 9_000,
                },
            )
            .await
            .expect("fail Run");
        let failed = storage
            .get_conversation("conversation-failure")
            .await
            .expect("get failed conversation")
            .expect("failed conversation");
        assert_eq!(failed.conversation.version, 3);
        assert_eq!(failed.turns[0].status, LocalConversationTurnStatus::Failed);
        assert_eq!(failed.messages.len(), 1);
    }
}
