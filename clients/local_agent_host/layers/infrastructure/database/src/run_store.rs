// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[async_trait]
impl LocalAgentRunStore for SqliteClientStorage {
    async fn create_run(
        &self,
        command: &IdempotentCommand,
        run: &LocalAgentRunRecord,
        event_id: &str,
    ) -> Result<LocalAgentRunRecord, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            Self::insert_run_on(&mut connection, run).await?;
            Self::insert_event(
                &mut connection,
                event_id,
                &run.run_id,
                "run_created",
                &serde_json::json!({"profile_key": run.profile_key}),
                run.created_at_unix_ms,
            )
            .await
            .db()?;
            Self::record_receipt(&mut connection, command, run, run.created_at_unix_ms)
                .await
                .db()?;
            Ok(run.clone())
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn get_run(
        &self,
        run_id: &str,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::fetch_run_on(&mut connection, run_id).await
    }

    async fn get_run_for_owner(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<Option<LocalAgentRunRecord>, ClientStorageError> {
        run_owner_store::get_run(self, owner_user_id, run_id).await
    }

    async fn list_runs(
        &self,
        owner_user_id: &str,
        scope: LocalAgentRunListScope,
        status: Option<LocalAgentRunStatus>,
        updated_after_unix_ms: Option<i64>,
        before_updated_at_unix_ms: Option<i64>,
        before_run_id: Option<&str>,
        limit: u32,
    ) -> Result<LocalAgentRunPage, ClientStorageError> {
        run_query_store::list_runs(
            self,
            owner_user_id,
            scope,
            status,
            updated_after_unix_ms,
            before_updated_at_unix_ms,
            before_run_id,
            limit,
        )
        .await
    }

    async fn recover_expired_claims(
        &self,
        owner_user_id: &str,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result =
            run_recovery_store::recover_expired_claims(&mut connection, owner_user_id, now_unix_ms)
                .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn renew_run_claim(
        &self,
        owner_user_id: &str,
        run_id: &str,
        claim_token: &str,
        expected_version: u64,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
    ) -> Result<bool, ClientStorageError> {
        run_owner_store::renew_claim(
            self,
            owner_user_id,
            run_id,
            claim_token,
            expected_version,
            now_unix_ms,
            claim_until_unix_ms,
        )
        .await
    }

    async fn claim_next_run(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        worker_id: &str,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
        event_id: &str,
    ) -> Result<Option<LocalAgentRunClaim>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            run_recovery_store::recover_expired_claims(&mut connection, owner_user_id, now_unix_ms)
                .await
                .db()?;
            let candidate = sqlx::query(
                "SELECT run_id, status FROM local_agent_runs \
                 WHERE owner_user_id = ? AND iteration < max_iterations AND (\
                    status IN ('queued', 'model_ready', 'continuation_ready') OR \
                    (status = 'retry_scheduled' AND next_attempt_at_unix_ms <= ?)\
                 ) ORDER BY created_at_unix_ms, run_id LIMIT 1",
            )
            .bind(owner_user_id)
            .bind(now_unix_ms)
            .fetch_optional(&mut *connection)
            .await
            .db()?;
            let Some(candidate) = candidate else {
                return Ok(None);
            };
            let run_id: String = candidate.try_get("run_id").db()?;
            let candidate_status: String = candidate.try_get("status").db()?;
            let updated = sqlx::query(
                "UPDATE local_agent_runs SET status = 'model_running', iteration = iteration + 1, \
                 version = version + 1, claim_token = ?, claim_until_unix_ms = ?, \
                 next_attempt_at_unix_ms = NULL, updated_at_unix_ms = ? WHERE run_id = ?",
            )
            .bind(claim_token)
            .bind(claim_until_unix_ms)
            .bind(now_unix_ms)
            .bind(&run_id)
            .execute(&mut *connection)
            .await
            .db()?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "run changed while claiming: {run_id}"
                )));
            }
            let claimed_version = Self::fetch_run_on(&mut connection, &run_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?
                .version;
            conversation_guidance::attach_pending_guidance_to_claim(
                &mut connection,
                &run_id,
                claimed_version,
                now_unix_ms,
            )
            .await?;
            Self::insert_event(
                &mut connection,
                event_id,
                &run_id,
                "run_claimed",
                &serde_json::json!({
                    "worker_id": worker_id,
                    "claim_until_unix_ms": claim_until_unix_ms
                }),
                now_unix_ms,
            )
            .await
            .db()?;
            let run = Self::fetch_run_on(&mut connection, &run_id)
                .await
                .db()?
                .ok_or_else(|| ClientStorageError::NotFound(run_id.clone()))?;
            if candidate_status == "queued" && run.owner_entity_type == "task" {
                let task_started = sqlx::query(
                    "UPDATE local_tasks SET status = 'running', version = version + 1, \
                     updated_at_unix_ms = ? WHERE task_id = ? AND status = 'ready' \
                     AND active_run_id = ?",
                )
                .bind(now_unix_ms)
                .bind(&run.owner_entity_id)
                .bind(&run.run_id)
                .execute(&mut *connection)
                .await
                .db()?;
                if task_started.rows_affected() != 1 {
                    let task_exists: i64 =
                        sqlx::query_scalar("SELECT COUNT(*) FROM local_tasks WHERE task_id = ?")
                            .bind(&run.owner_entity_id)
                            .fetch_one(&mut *connection)
                            .await
                            .db()?;
                    if task_exists != 0 {
                        return Err(ClientStorageError::Conflict(format!(
                            "queued Task changed before model claim: {}",
                            run.owner_entity_id
                        )));
                    }
                }
                task_conversation_writeback::write_back_task_run_started(
                    &mut connection,
                    &run,
                    now_unix_ms,
                )
                .await?;
            }
            let response = Some(LocalAgentRunClaim {
                worker_id: worker_id.to_string(),
                claim_token: claim_token.to_string(),
                run,
            });
            Self::record_receipt(&mut connection, command, &response, now_unix_ms)
                .await
                .db()?;
            Ok(response)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn next_retry_at(&self, owner_user_id: &str) -> Result<Option<i64>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Ok(sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MIN(deadline) FROM (\
               SELECT MIN(next_attempt_at_unix_ms) AS deadline FROM local_agent_runs \
                 WHERE owner_user_id = ? AND status = 'retry_scheduled' \
               UNION ALL \
               SELECT MIN(claim_until_unix_ms) AS deadline FROM local_agent_runs \
                 WHERE owner_user_id = ? AND status = 'model_running' \
                   AND claim_until_unix_ms IS NOT NULL \
               UNION ALL \
               SELECT MIN(i.claim_until_unix_ms) AS deadline \
                 FROM local_agent_tool_invocations i \
                 INNER JOIN local_agent_runs r ON r.run_id = i.run_id \
                 WHERE r.owner_user_id = ? AND i.status = 'running' \
                   AND i.claim_until_unix_ms IS NOT NULL\
             )",
        )
        .bind(owner_user_id)
        .bind(owner_user_id)
        .bind(owner_user_id)
        .fetch_one(&mut *connection)
        .await
        .db()?)
    }

    async fn apply_transition(
        &self,
        command: &IdempotentCommand,
        transition: &RunTransition,
    ) -> Result<LocalAgentRunRecord, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            let updated = sqlx::query(
                "UPDATE local_agent_runs SET status = ?, model_attempt = ?, version = version + 1, \
                 claim_token = NULL, claim_until_unix_ms = NULL, next_attempt_at_unix_ms = ?, \
                 pending_tool_batch_json = ?, terminal_outcome_json = ?, \
                 checkpoint_json = COALESCE(?, checkpoint_json), \
                 continuation_input_json = CASE WHEN ? THEN NULL ELSE continuation_input_json END, \
                 updated_at_unix_ms = ? \
                 WHERE run_id = ? AND status = ? AND version = ? AND claim_token = ? \
                 AND claim_until_unix_ms > ?",
            )
            .bind(transition.next_status.as_str())
            .bind(i64::from(transition.next_model_attempt))
            .bind(transition.next_attempt_at_unix_ms)
            .bind(json_option(&transition.pending_tool_batch)?)
            .bind(json_option(&transition.terminal_outcome)?)
            .bind(json_option(&transition.checkpoint)?)
            .bind(transition.clear_continuation_input)
            .bind(transition.occurred_at_unix_ms)
            .bind(&transition.run_id)
            .bind(transition.expected_status.as_str())
            .bind(transition.expected_version as i64)
            .bind(&transition.claim_token)
            .bind(transition.occurred_at_unix_ms)
            .execute(&mut *connection)
            .await.db()?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "run claim or version changed: {}",
                    transition.run_id
                )));
            }
            Self::insert_event(
                &mut connection,
                &transition.event_id,
                &transition.run_id,
                &transition.event_type,
                &transition.event_payload,
                transition.occurred_at_unix_ms,
            )
            .await
            .db()?;
            if let Some(batch) = transition.tool_batch.as_ref() {
                tool_store::insert_tool_batch(
                    &mut connection,
                    &transition.run_id,
                    batch,
                    transition.occurred_at_unix_ms,
                )
                .await
                .db()?;
            }
            let run = Self::fetch_run_on(&mut connection, &transition.run_id)
                .await
                .db()?
                .ok_or_else(|| ClientStorageError::NotFound(transition.run_id.clone()))?;
            task_lifecycle::reconcile_task_after_run(
                &mut connection,
                &run,
                transition.occurred_at_unix_ms,
            )
            .await
            .db()?;
            task_conversation_writeback::write_back_terminal_task_run(
                &mut connection,
                &run,
                transition.occurred_at_unix_ms,
            )
            .await?;
            conversation_lifecycle::reconcile_conversation_after_run(
                &mut connection,
                &run,
                transition.occurred_at_unix_ms,
            )
            .await?;
            Self::record_receipt(
                &mut connection,
                command,
                &run,
                transition.occurred_at_unix_ms,
            )
            .await
            .db()?;
            Ok(run)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn resume_run(
        &self,
        command: &IdempotentCommand,
        run_id: &str,
        expected_version: u64,
        expected_status: LocalAgentRunStatus,
        continuation_input: &Value,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentRunRecord, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            if expected_status == LocalAgentRunStatus::WaitingUser {
                requirement_survey_store::reject_resume_with_open_survey(&mut connection, run_id)
                    .await?;
            }
            let updated = sqlx::query(
                "UPDATE local_agent_runs SET status = 'continuation_ready', \
                 version = version + 1, continuation_input_json = ?, updated_at_unix_ms = ? \
                 WHERE run_id = ? AND status = ? AND version = ?",
            )
            .bind(serde_json::to_string(continuation_input)?)
            .bind(now_unix_ms)
            .bind(run_id)
            .bind(expected_status.as_str())
            .bind(expected_version as i64)
            .execute(&mut *connection)
            .await
            .db()?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "run status or version changed while resuming: {run_id}"
                )));
            }
            Self::insert_event(
                &mut connection,
                event_id,
                run_id,
                "run_resumed",
                continuation_input,
                now_unix_ms,
            )
            .await
            .db()?;
            let run = Self::fetch_run_on(&mut connection, run_id)
                .await
                .db()?
                .ok_or_else(|| ClientStorageError::NotFound(run_id.to_string()))?;
            Self::record_receipt(&mut connection, command, &run, now_unix_ms)
                .await
                .db()?;
            Ok(run)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn resume_run_for_owner(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        run_id: &str,
        expected_version: u64,
        expected_status: LocalAgentRunStatus,
        continuation_input: &Value,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentRunRecord, ClientStorageError> {
        run_owner_store::resume_run(
            self,
            command,
            owner_user_id,
            run_id,
            expected_version,
            expected_status,
            continuation_input,
            event_id,
            now_unix_ms,
        )
        .await
    }

    async fn cancel_run(
        &self,
        command: &IdempotentCommand,
        run_id: &str,
        expected_version: Option<u64>,
        reason: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentRunRecord, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            let run = run_commands::cancel_run_on(
                &mut connection,
                run_id,
                expected_version,
                reason,
                event_id,
                now_unix_ms,
            )
            .await?;
            Self::record_receipt(&mut connection, command, &run, now_unix_ms)
                .await
                .db()?;
            Ok(run)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn cancel_run_for_owner(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        run_id: &str,
        expected_version: Option<u64>,
        reason: &str,
        event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentRunRecord, ClientStorageError> {
        run_owner_store::cancel_run(
            self,
            command,
            owner_user_id,
            run_id,
            expected_version,
            reason,
            event_id,
            now_unix_ms,
        )
        .await
    }

    async fn list_events(
        &self,
        after_cursor: i64,
        limit: u32,
        run_id: Option<&str>,
    ) -> Result<Vec<LocalAgentEventRecord>, ClientStorageError> {
        run_owner_store::list_events_unscoped(self, after_cursor, limit, run_id).await
    }

    async fn list_events_for_owner(
        &self,
        owner_user_id: &str,
        after_cursor: i64,
        limit: u32,
        run_id: Option<&str>,
        event_type: Option<&str>,
        newest_first: bool,
        payload_mode: chatos_local_agent_protocol::LocalAgentEventPayloadMode,
    ) -> Result<Vec<LocalAgentEventRecord>, ClientStorageError> {
        run_owner_store::list_events(
            self,
            owner_user_id,
            after_cursor,
            limit,
            run_id,
            event_type,
            newest_first,
            payload_mode,
        )
        .await
    }

    async fn latest_event_cursor_for_owner(
        &self,
        owner_user_id: &str,
    ) -> Result<i64, ClientStorageError> {
        run_owner_store::latest_event_cursor(self, owner_user_id).await
    }

    async fn health_check(&self) -> Result<(), ClientStorageError> {
        maintenance::verify_available(&self.pool).await
    }
}
