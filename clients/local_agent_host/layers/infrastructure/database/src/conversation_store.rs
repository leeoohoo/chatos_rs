// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    is_unique_violation, ClientStorageError, IdempotentCommand, LocalConversationStore,
    SqliteClientStorage, SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CreateConversationCommand, LocalAgentRunRecord, LocalConversationAttachmentRecord,
    LocalConversationAttachmentSpec, LocalConversationDetail, LocalConversationMessageRecord,
    LocalConversationMessageRole, LocalConversationPage, LocalConversationRecord,
    LocalConversationTurnRecord, LocalConversationTurnStart, LocalConversationTurnStatus,
    StartConversationTurnCommand, LOCAL_CONVERSATION_MAX_HISTORY_PAGE_SIZE,
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
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            // Older Agent conversations were created without a resource binding.  A later
            // Agent-bound client may safely migrate exactly that legacy shape while creating
            // the deterministic conversation again.  Never overwrite an existing binding;
            // the owner-scoped unique index and the version predicate keep this idempotent.
            if let Some(existing) = fetch_conversation_record_for_owner(
                &mut connection,
                &conversation.owner_user_id,
                &conversation.conversation_id,
            )
            .await?
            {
                if existing.resource == conversation.resource {
                    let detail = fetch_conversation(&mut connection, &conversation.conversation_id)
                        .await?
                        .ok_or_else(|| {
                            ClientStorageError::NotFound(conversation.conversation_id.clone())
                        })?;
                    Self::record_receipt(&mut connection, command, &detail, now_unix_ms).await?;
                    return Ok(detail);
                }
                let Some(resource) = conversation.resource.as_ref() else {
                    return Err(ClientStorageError::Conflict(format!(
                        "conversation already exists: {}",
                        conversation.conversation_id
                    )));
                };
                if existing.resource.is_some() {
                    return Err(ClientStorageError::Conflict(format!(
                        "conversation resource already bound: {}",
                        conversation.conversation_id
                    )));
                }
                let resource_kind = match resource.kind {
                    chatos_local_agent_protocol::LocalConversationResourceKind::Contact => {
                        "contact"
                    }
                    chatos_local_agent_protocol::LocalConversationResourceKind::Project => {
                        "project"
                    }
                };
                let updated = sqlx::query(
                    "UPDATE local_conversations SET resource_kind = ?, resource_id = ?, \
                     version = version + 1, updated_at_unix_ms = ? \
                     WHERE conversation_id = ? AND owner_user_id = ? AND version = ? \
                       AND resource_kind IS NULL AND resource_id IS NULL",
                )
                .bind(resource_kind)
                .bind(&resource.resource_id)
                .bind(now_unix_ms)
                .bind(&conversation.conversation_id)
                .bind(&conversation.owner_user_id)
                .bind(i64::try_from(existing.version).map_err(|_| {
                    ClientStorageError::InvalidState("conversation version exceeds i64".to_string())
                })?)
                .execute(&mut *connection)
                .await;
                if let Err(error) = updated {
                    return Err(if is_unique_violation(&error) {
                        ClientStorageError::Conflict(format!(
                            "conversation resource already belongs to another conversation: {}",
                            resource.resource_id
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
                return Ok(detail);
            }
            let inserted = sqlx::query(
                "INSERT INTO local_conversations(\
                 conversation_id, owner_user_id, title, resource_kind, resource_id, version, \
                 created_at_unix_ms, updated_at_unix_ms) VALUES(?, ?, ?, ?, ?, 1, ?, ?)",
            )
            .bind(&conversation.conversation_id)
            .bind(&conversation.owner_user_id)
            .bind(conversation.title.trim())
            .bind(
                conversation
                    .resource
                    .as_ref()
                    .map(|value| match value.kind {
                        chatos_local_agent_protocol::LocalConversationResourceKind::Contact => {
                            "contact"
                        }
                        chatos_local_agent_protocol::LocalConversationResourceKind::Project => {
                            "project"
                        }
                    }),
            )
            .bind(
                conversation
                    .resource
                    .as_ref()
                    .map(|value| value.resource_id.as_str()),
            )
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
        owner_user_id: &str,
        conversation_id: &str,
    ) -> Result<Option<LocalConversationDetail>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        fetch_conversation_for_owner(&mut connection, owner_user_id, conversation_id).await
    }

    async fn get_conversation_context(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
    ) -> Result<Option<LocalConversationDetail>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        fetch_conversation_context_for_owner(&mut connection, owner_user_id, conversation_id).await
    }

    async fn get_conversation_attachment(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        authorized_local_ref: &str,
    ) -> Result<Option<LocalConversationAttachmentRecord>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        sqlx::query(
            "SELECT a.attachment_id, a.conversation_id, a.turn_id, a.message_id, a.ordinal, \
             a.display_name, a.media_type, a.byte_size, a.sha256, a.authorized_local_ref, \
             a.metadata_json, a.created_at_unix_ms \
             FROM local_conversation_message_attachments a \
             JOIN local_conversations c ON c.conversation_id = a.conversation_id \
             WHERE c.owner_user_id = ? AND a.conversation_id = ? \
               AND a.authorized_local_ref = ? \
             ORDER BY a.created_at_unix_ms DESC, a.attachment_id DESC LIMIT 1",
        )
        .bind(owner_user_id)
        .bind(conversation_id)
        .bind(authorized_local_ref)
        .fetch_optional(&mut *connection)
        .await
        .db()?
        .map(decode_attachment)
        .transpose()
    }

    async fn list_recent_conversation_attachments(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        turn_limit: u32,
    ) -> Result<Vec<LocalConversationAttachmentRecord>, ClientStorageError> {
        if turn_limit == 0 || turn_limit > LOCAL_CONVERSATION_MAX_HISTORY_PAGE_SIZE {
            return Err(ClientStorageError::InvalidState(format!(
                "conversation attachment turn limit must be between 1 and \
                 {LOCAL_CONVERSATION_MAX_HISTORY_PAGE_SIZE}"
            )));
        }
        let mut connection = self.pool.acquire().await.db()?;
        if fetch_conversation_record_for_owner(&mut connection, owner_user_id, conversation_id)
            .await?
            .is_none()
        {
            return Err(ClientStorageError::NotFound(conversation_id.to_string()));
        }
        sqlx::query(
            "SELECT a.attachment_id, a.conversation_id, a.turn_id, a.message_id, a.ordinal, \
             a.display_name, a.media_type, a.byte_size, a.sha256, a.authorized_local_ref, \
             a.metadata_json, a.created_at_unix_ms \
             FROM (SELECT turn_id, created_at_unix_ms FROM local_conversation_turns \
                   WHERE conversation_id = ? \
                   ORDER BY created_at_unix_ms DESC, turn_id DESC LIMIT ?) recent \
             JOIN local_conversation_messages m ON m.turn_id = recent.turn_id \
             JOIN local_conversation_message_attachments a ON a.message_id = m.message_id \
             ORDER BY recent.created_at_unix_ms, recent.turn_id, m.ordinal, a.ordinal",
        )
        .bind(conversation_id)
        .bind(i64::from(turn_limit))
        .fetch_all(&mut *connection)
        .await
        .db()?
        .into_iter()
        .map(decode_attachment)
        .collect()
    }

    async fn get_conversation_history(
        &self,
        owner_user_id: &str,
        conversation_id: &str,
        before_ordinal: Option<u64>,
        limit: u32,
    ) -> Result<chatos_local_agent_protocol::LocalConversationHistoryPage, ClientStorageError> {
        super::conversation_history::get_conversation_history(
            self,
            owner_user_id,
            conversation_id,
            before_ordinal,
            limit,
        )
        .await
    }

    async fn list_conversations(
        &self,
        owner_user_id: &str,
        before_updated_at_unix_ms: Option<i64>,
        before_conversation_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalConversationPage, ClientStorageError> {
        super::conversation_query_store::list_conversations(
            self,
            owner_user_id,
            before_updated_at_unix_ms,
            before_conversation_id,
            limit,
        )
        .await
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
        let mut connection = self.acquire_write().await.db()?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            if let Some(seed) = &turn.conversation_if_missing {
                sqlx::query(
                    "INSERT INTO local_conversations(\
                     conversation_id, owner_user_id, title, resource_kind, resource_id, version, \
                     created_at_unix_ms, updated_at_unix_ms) VALUES(?, ?, ?, ?, ?, 1, ?, ?) \
                     ON CONFLICT(conversation_id) DO NOTHING",
                )
                .bind(&seed.conversation_id)
                .bind(&seed.owner_user_id)
                .bind(seed.title.trim())
                .bind(seed.resource.as_ref().map(|value| match value.kind {
                    chatos_local_agent_protocol::LocalConversationResourceKind::Contact => {
                        "contact"
                    }
                    chatos_local_agent_protocol::LocalConversationResourceKind::Project => {
                        "project"
                    }
                }))
                .bind(
                    seed.resource
                        .as_ref()
                        .map(|value| value.resource_id.as_str()),
                )
                .bind(now_unix_ms)
                .bind(now_unix_ms)
                .execute(&mut *connection)
                .await
                .db()?;
            }
            let conversation = fetch_conversation_record_for_owner(
                &mut connection,
                &turn.owner_user_id,
                &turn.conversation_id,
            )
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(turn.conversation_id.clone()))?;
            if let Some(seed) = &turn.conversation_if_missing {
                if conversation.resource != seed.resource {
                    return Err(ClientStorageError::Conflict(format!(
                        "conversation resource changed: {}",
                        turn.conversation_id
                    )));
                }
            }
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
            let mut bound_run = run.clone();
            if let Some(resource) = conversation.resource.as_ref().filter(|resource| {
                resource.kind == chatos_local_agent_protocol::LocalConversationResourceKind::Project
            }) {
                if let Some(plan) = super::requirement_plan_store::latest_for_scope(
                    &mut connection,
                    &turn.owner_user_id,
                    &resource.resource_id,
                )
                .await?
                {
                    let input = bound_run.input.as_object_mut().ok_or_else(|| {
                        ClientStorageError::InvalidState(
                            "Main Chat input must be an object when binding a plan".into(),
                        )
                    })?;
                    input.insert("requirement_plan".into(), serde_json::to_value(plan)?);
                }
            }
            if serde_json::to_vec(&bound_run.input)?.len()
                > chatos_local_agent_protocol::LOCAL_AGENT_MAX_INPUT_BYTES
            {
                return Err(ClientStorageError::InvalidState(
                    "Main Chat input with bound plan exceeds its byte limit".into(),
                ));
            }
            let superseded_recoveries = sqlx::query(
                "SELECT run.run_id FROM local_agent_runs run \
                 JOIN local_conversation_turns old_turn ON old_turn.run_id = run.run_id \
                 WHERE run.status = 'continuation_ready' \
                   AND json_extract(run.input_json, '$.task_graph_recovery.type') = 'task_graph_recovery' \
                   AND old_turn.conversation_id = ? \
                   AND old_turn.status IN ('succeeded','failed')",
            )
            .bind(&turn.conversation_id)
            .fetch_all(&mut *connection)
            .await
            .db()?;
            sqlx::query(
                "UPDATE local_agent_runs SET status = CASE WHEN EXISTS(\
                   SELECT 1 FROM local_conversation_turns source_turn \
                   WHERE source_turn.run_id = local_agent_runs.run_id \
                     AND source_turn.status = 'failed') THEN 'failed' ELSE 'succeeded' END, \
                 version = version + 1, \
                 continuation_input_json = NULL, updated_at_unix_ms = ? \
                 WHERE status = 'continuation_ready' \
                   AND json_extract(input_json, '$.task_graph_recovery.type') = 'task_graph_recovery' \
                   AND run_id IN (SELECT run_id FROM local_conversation_turns \
                     WHERE conversation_id = ? AND status IN ('succeeded','failed'))",
            )
            .bind(now_unix_ms)
            .bind(&turn.conversation_id)
            .execute(&mut *connection)
            .await
            .db()?;
            for row in superseded_recoveries {
                let run_id: String = row.try_get("run_id").db()?;
                Self::insert_event(
                    &mut connection,
                    &format!("task-recovery-superseded:{run_id}:{}", turn.turn_id),
                    &run_id,
                    "task_graph_recovery_superseded",
                    &json!({"newer_turn_id": turn.turn_id}),
                    now_unix_ms,
                )
                .await?;
            }
            Self::insert_run_on(&mut connection, &bound_run).await?;
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
            super::conversation_commands::project_conversation_message_to_memory(
                &mut connection,
                &bound_run,
                &turn.conversation_id,
                &turn.turn_id,
                &turn.message_id,
                "user",
                &json!(turn.message),
                &turn.message_metadata,
                &turn.attachments,
                "conversation_start",
                "pending",
                false,
                false,
                now_unix_ms,
            )
            .await?;
            super::task_followup_gate::begin_review(
                &mut connection,
                &turn.conversation_id,
                &turn.run_id,
                &turn.message_id,
                now_unix_ms,
            )
            .await?;
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
                attachments: detail
                    .attachments
                    .into_iter()
                    .filter(|record| record.message_id == turn.message_id)
                    .collect(),
                run: bound_run,
            };
            Self::record_receipt(&mut connection, command, &started, now_unix_ms).await?;
            Ok(started)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn resume_conversation_turn(
        &self,
        command: &IdempotentCommand,
        turn: &chatos_local_agent_protocol::ResumeConversationTurnCommand,
        continuation_input: &serde_json::Value,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<chatos_local_agent_protocol::LocalConversationTurnUpdate, ClientStorageError> {
        super::conversation_commands::resume_conversation_turn(
            self,
            command,
            turn,
            continuation_input,
            event_id,
            now_unix_ms,
        )
        .await
    }

    async fn guide_conversation_turn(
        &self,
        command: &IdempotentCommand,
        turn: &chatos_local_agent_protocol::GuideConversationTurnCommand,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<chatos_local_agent_protocol::LocalConversationTurnUpdate, ClientStorageError> {
        super::conversation_commands::guide_conversation_turn(
            self,
            command,
            turn,
            event_id,
            now_unix_ms,
        )
        .await
    }

    async fn cancel_conversation_turn(
        &self,
        command: &IdempotentCommand,
        turn: &chatos_local_agent_protocol::CancelConversationTurnCommand,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<chatos_local_agent_protocol::LocalConversationTurnUpdate, ClientStorageError> {
        super::conversation_commands::cancel_conversation_turn(
            self,
            command,
            turn,
            event_id,
            now_unix_ms,
        )
        .await
    }
}

include!("conversation_store/queries.rs");
include!("conversation_store/decoding.rs");
#[cfg(test)]
#[path = "conversation_store_tests.rs"]
mod tests;
