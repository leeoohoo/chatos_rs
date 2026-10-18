// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    tool_store_execution::{advance_run_after_tool, recover_expired_on},
    tool_store_record::{
        decode_invocation, fetch_invocation, outcome_fields, tool_execution_route,
    },
    ClientStorageError, IdempotentCommand, LocalAgentToolStore, SqliteClientStorage,
    SqliteResultExt,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalAgentToolApprovalDecision, LocalAgentToolApprovalResult, LocalAgentToolApprovalStatus,
    LocalAgentToolBatch, LocalAgentToolClaim, LocalAgentToolCommitResult,
    LocalAgentToolExecutionRoute, LocalAgentToolInvocationRecord, LocalAgentToolOutcome,
};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row, Sqlite, SqliteConnection};
use uuid::Uuid;

pub(crate) async fn insert_tool_batch(
    connection: &mut SqliteConnection,
    run_id: &str,
    batch: &LocalAgentToolBatch,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    batch.validate().map_err(ClientStorageError::InvalidState)?;
    let profile_key: String =
        sqlx::query_scalar("SELECT profile_key FROM local_agent_runs WHERE run_id = ?")
            .bind(run_id)
            .fetch_one(&mut *connection)
            .await
            .db()?;
    for call in &batch.calls {
        let execution_route = tool_execution_route(&profile_key, &call.tool_name);
        sqlx::query(
            "INSERT INTO local_agent_tool_invocations(\
             invocation_id, run_id, batch_id, call_id, tool_name, execution_route, arguments_json, \
             side_effecting, requires_approval, approval_status, status, result_json, error_text, \
             version, claim_token, claim_until_unix_ms, created_at_unix_ms, updated_at_unix_ms) \
             VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', NULL, NULL, 1, NULL, NULL, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(run_id)
        .bind(&batch.batch_id)
        .bind(&call.call_id)
        .bind(&call.tool_name)
        .bind(execution_route.as_str())
        .bind(serde_json::to_string(&call.arguments)?)
        .bind(call.side_effecting)
        .bind(call.requires_approval)
        .bind(if call.requires_approval {
            LocalAgentToolApprovalStatus::Pending.as_str()
        } else {
            LocalAgentToolApprovalStatus::NotRequired.as_str()
        })
        .bind(now_unix_ms)
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await
        .db()?;
    }
    Ok(())
}

pub(crate) async fn fail_open_invocations_for_cancelled_run(
    connection: &mut SqliteConnection,
    run_id: &str,
    reason: &str,
    now_unix_ms: i64,
) -> Result<u64, ClientStorageError> {
    let updated = sqlx::query(
        "UPDATE local_agent_tool_invocations SET status = 'failed', result_json = NULL, \
         error_text = ?, version = version + 1, claim_token = NULL, claim_until_unix_ms = NULL, \
         updated_at_unix_ms = ? WHERE run_id = ? AND status IN ('pending', 'running')",
    )
    .bind(format!("owning Run was cancelled: {reason}"))
    .bind(now_unix_ms)
    .bind(run_id)
    .execute(&mut *connection)
    .await
    .db()?;
    Ok(updated.rows_affected())
}

#[async_trait]
impl LocalAgentToolStore for SqliteClientStorage {
    async fn get_tool_invocation(
        &self,
        invocation_id: &str,
    ) -> Result<Option<LocalAgentToolInvocationRecord>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        fetch_invocation(&mut connection, invocation_id).await
    }

    async fn get_completed_tool_invocation_for_run_call(
        &self,
        owner_user_id: &str,
        run_id: &str,
        call_id: &str,
    ) -> Result<Option<LocalAgentToolInvocationRecord>, ClientStorageError> {
        let row = sqlx::query(
            "SELECT i.invocation_id, i.run_id, i.batch_id, i.call_id, i.tool_name, \
             i.execution_route, i.arguments_json, i.side_effecting, i.requires_approval, \
             i.approval_status, i.approval_decided_by, i.approval_reason, \
             i.approval_decided_at_unix_ms, i.status, i.result_json, i.error_text, \
             i.version, i.claim_token, i.claim_until_unix_ms, i.created_at_unix_ms, \
             i.updated_at_unix_ms FROM local_agent_tool_invocations i \
             INNER JOIN local_agent_runs r ON r.run_id = i.run_id \
             WHERE r.owner_user_id = ? AND i.run_id = ? AND i.call_id = ? \
               AND i.status IN ('succeeded', 'failed', 'needs_review') \
               AND i.result_json IS NOT NULL LIMIT 1",
        )
        .bind(owner_user_id)
        .bind(run_id)
        .bind(call_id)
        .fetch_optional(&self.pool)
        .await
        .db()?;
        row.map(decode_invocation).transpose()
    }

    async fn successful_tool_invocation_count(
        &self,
        run_id: &str,
        tool_name: &str,
    ) -> Result<u64, ClientStorageError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM local_agent_tool_invocations \
             WHERE run_id = ? AND tool_name = ? AND status = 'succeeded' \
               AND COALESCE(json_extract(result_json, '$.is_error'), 0) = 0 \
               AND COALESCE(json_extract(result_json, '$.isError'), 0) = 0",
        )
        .bind(run_id)
        .bind(tool_name)
        .fetch_one(&self.pool)
        .await
        .db()?;
        u64::try_from(count).map_err(|_| {
            ClientStorageError::InvalidState(
                "successful tool invocation count cannot be negative".to_string(),
            )
        })
    }

    async fn has_successful_tool_invocation_argument(
        &self,
        run_id: &str,
        tool_name: &str,
        argument_name: &str,
        argument_value: &str,
    ) -> Result<bool, ClientStorageError> {
        if argument_name.trim().is_empty() || argument_name.len() > 128 {
            return Err(ClientStorageError::InvalidState(
                "tool invocation argument name must be 1..=128 bytes".to_string(),
            ));
        }
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT arguments_json FROM local_agent_tool_invocations \
             WHERE run_id = ? AND tool_name = ? AND status = 'succeeded' \
               AND COALESCE(json_extract(result_json, '$.is_error'), 0) = 0 \
               AND COALESCE(json_extract(result_json, '$.isError'), 0) = 0",
        )
        .bind(run_id)
        .bind(tool_name)
        .fetch_all(&self.pool)
        .await
        .db()?;
        for encoded in rows {
            let arguments: Value = serde_json::from_str(&encoded)?;
            if arguments.get(argument_name).and_then(Value::as_str) == Some(argument_value) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn successful_tool_invocation_argument_values(
        &self,
        owner_user_id: &str,
        run_id: &str,
        tool_name: &str,
        argument_name: &str,
    ) -> Result<Vec<String>, ClientStorageError> {
        if argument_name.trim().is_empty() || argument_name.len() > 128 {
            return Err(ClientStorageError::InvalidState(
                "tool invocation argument name must be 1..=128 bytes".to_string(),
            ));
        }
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT i.arguments_json FROM local_agent_tool_invocations i \
             INNER JOIN local_agent_runs r ON r.run_id = i.run_id \
             WHERE i.run_id = ? AND r.owner_user_id = ? AND i.tool_name = ? \
               AND i.status = 'succeeded' \
               AND COALESCE(json_extract(i.result_json, '$.is_error'), 0) = 0 \
               AND COALESCE(json_extract(i.result_json, '$.isError'), 0) = 0 \
             ORDER BY i.created_at_unix_ms, i.invocation_id",
        )
        .bind(run_id)
        .bind(owner_user_id)
        .bind(tool_name)
        .fetch_all(&self.pool)
        .await
        .db()?;
        let mut values = std::collections::BTreeSet::new();
        for encoded in rows {
            let arguments: Value = serde_json::from_str(&encoded)?;
            if let Some(value) = arguments.get(argument_name).and_then(Value::as_str) {
                values.insert(value.to_string());
            }
        }
        Ok(values.into_iter().collect())
    }

    async fn latest_successful_tool_invocation_result(
        &self,
        owner_user_id: &str,
        run_id: &str,
        tool_name: &str,
        argument_name: &str,
        argument_value: &str,
        described_tool_option: Option<&str>,
    ) -> Result<Option<Value>, ClientStorageError> {
        if argument_name.is_empty()
            || argument_name.len() > 128
            || !argument_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(ClientStorageError::InvalidState(
                "invalid result lookup argument name".into(),
            ));
        }
        let encoded: Option<String> = sqlx::query_scalar(
            "SELECT i.result_json FROM local_agent_tool_invocations i \
             INNER JOIN local_agent_runs r ON r.run_id = i.run_id \
             WHERE i.run_id = ? AND r.owner_user_id = ? AND i.tool_name = ? \
               AND i.status = 'succeeded' AND i.result_json IS NOT NULL \
               AND COALESCE(json_extract(i.result_json, '$.is_error'), 0) = 0 \
               AND COALESCE(json_extract(i.result_json, '$.isError'), 0) = 0 \
               AND json_extract(i.arguments_json, ?) = ? \
               AND (? IS NULL OR ( \
                 COALESCE(json_extract(i.result_json, '$.content.view'), json_extract(i.result_json, '$.view')) = 'tool' \
                 AND EXISTS (SELECT 1 FROM json_each(COALESCE(json_extract(i.result_json, '$.content.tools'), json_extract(i.result_json, '$.tools'))) t \
                             WHERE json_extract(t.value, '$.tool_option') = ?) \
               )) \
             ORDER BY i.updated_at_unix_ms DESC, i.created_at_unix_ms DESC, i.invocation_id DESC LIMIT 1",
        ).bind(run_id).bind(owner_user_id).bind(tool_name)
            .bind(format!("$.{argument_name}")).bind(argument_value)
            .bind(described_tool_option).bind(described_tool_option)
            .fetch_optional(&self.pool).await.db()?;
        encoded
            .map(|value| serde_json::from_str(&value).map_err(ClientStorageError::from))
            .transpose()
    }

    async fn recover_expired_tool_claims(
        &self,
        owner_user_id: &str,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = recover_expired_on(&mut connection, owner_user_id, now_unix_ms).await;
        Self::finish_write(&mut connection, result).await
    }

    async fn renew_tool_claim(
        &self,
        owner_user_id: &str,
        invocation_id: &str,
        claim_token: &str,
        expected_version: u64,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
    ) -> Result<bool, ClientStorageError> {
        let updated = sqlx::query(
            "UPDATE local_agent_tool_invocations SET \
             claim_until_unix_ms = MAX(claim_until_unix_ms, ?) \
             WHERE invocation_id = ? AND status = 'running' AND version = ? \
             AND claim_token = ? AND claim_until_unix_ms > ? AND run_id IN (\
               SELECT run_id FROM local_agent_runs WHERE owner_user_id = ?\
             )",
        )
        .bind(claim_until_unix_ms)
        .bind(invocation_id)
        .bind(expected_version as i64)
        .bind(claim_token)
        .bind(now_unix_ms)
        .bind(owner_user_id)
        .execute(&self.pool)
        .await
        .db()?;
        Ok(updated.rows_affected() == 1)
    }

    async fn claim_next_tool(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        worker_id: &str,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
        event_id: &str,
        include_tool_names: Option<&[String]>,
        exclude_tool_names: &[String],
        execution_route: Option<LocalAgentToolExecutionRoute>,
    ) -> Result<Option<LocalAgentToolClaim>, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            recover_expired_on(&mut connection, owner_user_id, now_unix_ms)
                .await
                .db()?;
            let mut candidate_query = QueryBuilder::<Sqlite>::new(
                "SELECT invocation_id FROM local_agent_tool_invocations \
                 WHERE status = 'pending' \
                 AND approval_status IN ('not_required','approved') AND run_id IN (\
                   SELECT run_id FROM local_agent_runs \
                   WHERE owner_user_id = ",
            );
            candidate_query.push_bind(owner_user_id).push(
                " AND status = 'waiting_tool_result' \
                  AND NOT EXISTS (SELECT 1 FROM local_task_followup_reviews f \
                    WHERE local_agent_runs.owner_entity_type = 'task' \
                      AND f.task_id = local_agent_runs.owner_entity_id AND f.state IN ('pending','held'))\
                 )",
            );
            if let Some(tool_names) = include_tool_names {
                candidate_query.push(" AND tool_name IN (");
                let mut separated = candidate_query.separated(", ");
                for tool_name in tool_names {
                    separated.push_bind(tool_name);
                }
                separated.push_unseparated(")");
            }
            if !exclude_tool_names.is_empty() {
                candidate_query.push(" AND tool_name NOT IN (");
                let mut separated = candidate_query.separated(", ");
                for tool_name in exclude_tool_names {
                    separated.push_bind(tool_name);
                }
                separated.push_unseparated(")");
            }
            if let Some(execution_route) = execution_route {
                candidate_query
                    .push(" AND execution_route = ")
                    .push_bind(execution_route.as_str());
            }
            candidate_query.push(" ORDER BY created_at_unix_ms, invocation_id LIMIT 1");
            let candidate = candidate_query
                .build()
                .fetch_optional(&mut *connection)
                .await
                .db()?;
            let Some(candidate) = candidate else {
                return Ok(None);
            };
            let invocation_id: String = candidate.try_get("invocation_id").db()?;
            let updated = sqlx::query(
                "UPDATE local_agent_tool_invocations SET status = 'running', \
                 version = version + 1, claim_token = ?, claim_until_unix_ms = ?, \
                 updated_at_unix_ms = ? WHERE invocation_id = ? AND status = 'pending'",
            )
            .bind(claim_token)
            .bind(claim_until_unix_ms)
            .bind(now_unix_ms)
            .bind(&invocation_id)
            .execute(&mut *connection)
            .await
            .db()?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "tool invocation changed while claiming: {invocation_id}"
                )));
            }
            let invocation = fetch_invocation(&mut connection, &invocation_id)
                .await
                .db()?
                .ok_or_else(|| ClientStorageError::NotFound(invocation_id.clone()))?;
            Self::insert_event(
                &mut connection,
                event_id,
                &invocation.run_id,
                "tool_invocation_claimed",
                &json!({
                    "invocation_id": invocation.invocation_id,
                    "call_id": invocation.call_id,
                    "worker_id": worker_id,
                    "claim_until_unix_ms": claim_until_unix_ms
                }),
                now_unix_ms,
            )
            .await
            .db()?;
            let response = Some(LocalAgentToolClaim {
                worker_id: worker_id.to_string(),
                claim_token: claim_token.to_string(),
                invocation,
                preflight_outcome: None,
            });
            Self::record_receipt(&mut connection, command, &response, now_unix_ms)
                .await
                .db()?;
            Ok(response)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn commit_tool(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        invocation_id: &str,
        claim_token: &str,
        expected_version: u64,
        outcome: &LocalAgentToolOutcome,
        event_id: &str,
        batch_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentToolCommitResult, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        Self::begin_immediate(&mut connection).await.db()?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await.db()? {
                return Ok(replay);
            }
            let current = fetch_invocation(&mut connection, invocation_id)
                .await
                .db()?
                .ok_or_else(|| ClientStorageError::NotFound(invocation_id.to_string()))?;
            let owning_run = Self::fetch_run_on(&mut connection, &current.run_id)
                .await
                .db()?
                .filter(|run| run.owner_user_id == owner_user_id)
                .ok_or_else(|| ClientStorageError::NotFound(invocation_id.to_string()))?;
            let (status, result, error) = outcome_fields(outcome);
            let updated = sqlx::query(
                "UPDATE local_agent_tool_invocations SET status = ?, result_json = ?, \
                 error_text = ?, version = version + 1, claim_token = NULL, \
                 claim_until_unix_ms = NULL, updated_at_unix_ms = ? \
                 WHERE invocation_id = ? AND status = 'running' AND version = ? \
                 AND claim_token = ? AND claim_until_unix_ms > ?",
            )
            .bind(status.as_str())
            .bind(result.as_ref().map(serde_json::to_string).transpose()?)
            .bind(error)
            .bind(now_unix_ms)
            .bind(invocation_id)
            .bind(expected_version as i64)
            .bind(claim_token)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await
            .db()?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "tool claim or version changed: {invocation_id}"
                )));
            }
            let invocation = fetch_invocation(&mut connection, invocation_id)
                .await
                .db()?
                .ok_or_else(|| ClientStorageError::NotFound(invocation_id.to_string()))?;
            Self::insert_event(
                &mut connection,
                event_id,
                &current.run_id,
                "tool_invocation_completed",
                &json!({
                    "invocation_id": invocation.invocation_id,
                    "call_id": invocation.call_id,
                    "status": invocation.status,
                    "result": invocation.result,
                    "error": invocation.error
                }),
                now_unix_ms,
            )
            .await
            .db()?;
            advance_run_after_tool(
                &mut connection,
                &current.run_id,
                &current.batch_id,
                status,
                batch_event_id,
                now_unix_ms,
            )
            .await
            .db()?;
            let run = Self::fetch_run_on(&mut connection, &owning_run.run_id)
                .await
                .db()?
                .ok_or_else(|| ClientStorageError::NotFound(current.run_id.clone()))?;
            let response = LocalAgentToolCommitResult { invocation, run };
            Self::record_receipt(&mut connection, command, &response, now_unix_ms)
                .await
                .db()?;
            Ok(response)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn list_pending_tool_approvals(
        &self,
        owner_user_id: &str,
        limit: u32,
    ) -> Result<Vec<LocalAgentToolInvocationRecord>, ClientStorageError> {
        super::tool_approval_store::list_pending(self, owner_user_id, limit).await
    }

    async fn decide_tool_approval(
        &self,
        command: &IdempotentCommand,
        owner_user_id: &str,
        invocation_id: &str,
        expected_version: u64,
        decision: LocalAgentToolApprovalDecision,
        decided_by: &str,
        reason: &str,
        event_id: &str,
        batch_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentToolApprovalResult, ClientStorageError> {
        super::tool_approval_store::decide(
            self,
            command,
            owner_user_id,
            invocation_id,
            expected_version,
            decision,
            decided_by,
            reason,
            event_id,
            batch_event_id,
            now_unix_ms,
        )
        .await
    }
}
