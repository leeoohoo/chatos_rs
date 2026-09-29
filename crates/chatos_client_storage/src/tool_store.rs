// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, IdempotentCommand, SqliteClientStorage};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    LocalAgentToolBatch, LocalAgentToolClaim, LocalAgentToolCommitResult,
    LocalAgentToolInvocationRecord, LocalAgentToolOutcome, LocalAgentToolStatus,
};
use serde_json::{json, Value};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};
use std::str::FromStr;
use uuid::Uuid;

#[async_trait]
pub trait LocalAgentToolStore: Send + Sync {
    async fn recover_expired_tool_claims(
        &self,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn claim_next_tool(
        &self,
        command: &IdempotentCommand,
        worker_id: &str,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
        event_id: &str,
    ) -> Result<Option<LocalAgentToolClaim>, ClientStorageError>;

    #[allow(clippy::too_many_arguments)]
    async fn commit_tool(
        &self,
        command: &IdempotentCommand,
        invocation_id: &str,
        claim_token: &str,
        expected_version: u64,
        outcome: &LocalAgentToolOutcome,
        event_id: &str,
        batch_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentToolCommitResult, ClientStorageError>;
}

pub(crate) async fn insert_tool_batch(
    connection: &mut SqliteConnection,
    run_id: &str,
    batch: &LocalAgentToolBatch,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    batch.validate().map_err(ClientStorageError::InvalidState)?;
    for call in &batch.calls {
        sqlx::query(
            "INSERT INTO local_agent_tool_invocations(\
             invocation_id, run_id, batch_id, call_id, tool_name, arguments_json, \
             side_effecting, status, result_json, error_text, version, claim_token, \
             claim_until_unix_ms, created_at_unix_ms, updated_at_unix_ms) \
             VALUES(?, ?, ?, ?, ?, ?, ?, 'pending', NULL, NULL, 1, NULL, NULL, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(run_id)
        .bind(&batch.batch_id)
        .bind(&call.call_id)
        .bind(&call.tool_name)
        .bind(serde_json::to_string(&call.arguments)?)
        .bind(call.side_effecting)
        .bind(now_unix_ms)
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
}

#[async_trait]
impl LocalAgentToolStore for SqliteClientStorage {
    async fn recover_expired_tool_claims(
        &self,
        now_unix_ms: i64,
    ) -> Result<u64, ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        Self::begin_immediate(&mut connection).await?;
        let result = recover_expired_on(&mut connection, now_unix_ms).await;
        Self::finish_write(&mut connection, result).await
    }

    async fn claim_next_tool(
        &self,
        command: &IdempotentCommand,
        worker_id: &str,
        claim_token: &str,
        now_unix_ms: i64,
        claim_until_unix_ms: i64,
        event_id: &str,
    ) -> Result<Option<LocalAgentToolClaim>, ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            recover_expired_on(&mut connection, now_unix_ms).await?;
            let candidate = sqlx::query(
                "SELECT invocation_id FROM local_agent_tool_invocations \
                 WHERE status = 'pending' AND run_id IN (\
                   SELECT run_id FROM local_agent_runs WHERE status = 'waiting_tool_result'\
                 ) ORDER BY created_at_unix_ms, invocation_id LIMIT 1",
            )
            .fetch_optional(&mut *connection)
            .await?;
            let Some(candidate) = candidate else {
                let response: Option<LocalAgentToolClaim> = None;
                Self::record_receipt(&mut connection, command, &response, now_unix_ms).await?;
                return Ok(response);
            };
            let invocation_id: String = candidate.try_get("invocation_id")?;
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
            .await?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "tool invocation changed while claiming: {invocation_id}"
                )));
            }
            let invocation = fetch_invocation(&mut connection, &invocation_id)
                .await?
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
            .await?;
            let response = Some(LocalAgentToolClaim {
                worker_id: worker_id.to_string(),
                claim_token: claim_token.to_string(),
                invocation,
            });
            Self::record_receipt(&mut connection, command, &response, now_unix_ms).await?;
            Ok(response)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }

    async fn commit_tool(
        &self,
        command: &IdempotentCommand,
        invocation_id: &str,
        claim_token: &str,
        expected_version: u64,
        outcome: &LocalAgentToolOutcome,
        event_id: &str,
        batch_event_id: &str,
        now_unix_ms: i64,
    ) -> Result<LocalAgentToolCommitResult, ClientStorageError> {
        let mut connection = self.pool.acquire().await?;
        Self::begin_immediate(&mut connection).await?;
        let result = async {
            if let Some(replay) = Self::replay(&mut connection, command).await? {
                return Ok(replay);
            }
            let current = fetch_invocation(&mut connection, invocation_id)
                .await?
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
            .await?;
            if updated.rows_affected() != 1 {
                return Err(ClientStorageError::Conflict(format!(
                    "tool claim or version changed: {invocation_id}"
                )));
            }
            let invocation = fetch_invocation(&mut connection, invocation_id)
                .await?
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
            .await?;
            advance_run_after_tool(
                &mut connection,
                &current.run_id,
                &current.batch_id,
                status,
                batch_event_id,
                now_unix_ms,
            )
            .await?;
            let run = Self::fetch_run_on(&mut connection, &current.run_id)
                .await?
                .ok_or_else(|| ClientStorageError::NotFound(current.run_id.clone()))?;
            let response = LocalAgentToolCommitResult { invocation, run };
            Self::record_receipt(&mut connection, command, &response, now_unix_ms).await?;
            Ok(response)
        }
        .await;
        Self::finish_write(&mut connection, result).await
    }
}

async fn recover_expired_on(
    connection: &mut SqliteConnection,
    now_unix_ms: i64,
) -> Result<u64, ClientStorageError> {
    let rows = sqlx::query(
        "SELECT invocation_id, run_id, call_id, side_effecting, version \
         FROM local_agent_tool_invocations WHERE status = 'running' \
         AND claim_until_unix_ms IS NOT NULL AND claim_until_unix_ms <= ? \
         ORDER BY invocation_id",
    )
    .bind(now_unix_ms)
    .fetch_all(&mut *connection)
    .await?;
    for row in &rows {
        let invocation_id: String = row.try_get("invocation_id")?;
        let run_id: String = row.try_get("run_id")?;
        let call_id: String = row.try_get("call_id")?;
        let side_effecting: bool = row.try_get("side_effecting")?;
        let version: i64 = row.try_get("version")?;
        let next_status = if side_effecting {
            LocalAgentToolStatus::NeedsReview
        } else {
            LocalAgentToolStatus::Pending
        };
        sqlx::query(
            "UPDATE local_agent_tool_invocations SET status = ?, version = version + 1, \
             claim_token = NULL, claim_until_unix_ms = NULL, error_text = ?, \
             updated_at_unix_ms = ? WHERE invocation_id = ? AND version = ? \
             AND status = 'running'",
        )
        .bind(next_status.as_str())
        .bind(side_effecting.then_some("tool result was unknown when its claim expired"))
        .bind(now_unix_ms)
        .bind(&invocation_id)
        .bind(version)
        .execute(&mut *connection)
        .await?;
        let event_type = if side_effecting {
            sqlx::query(
                "UPDATE local_agent_runs SET status = 'needs_review', version = version + 1, \
                 updated_at_unix_ms = ? WHERE run_id = ? AND status = 'waiting_tool_result'",
            )
            .bind(now_unix_ms)
            .bind(&run_id)
            .execute(&mut *connection)
            .await?;
            "tool_claim_expired_needs_review"
        } else {
            "tool_claim_expired_requeued"
        };
        let event_id = format!("tool-recovery:{invocation_id}:{}", version + 1);
        SqliteClientStorage::insert_event(
            connection,
            &event_id,
            &run_id,
            event_type,
            &json!({
                "invocation_id": invocation_id,
                "call_id": call_id,
                "side_effecting": side_effecting
            }),
            now_unix_ms,
        )
        .await?;
    }
    Ok(rows.len() as u64)
}

async fn advance_run_after_tool(
    connection: &mut SqliteConnection,
    run_id: &str,
    batch_id: &str,
    completed_status: LocalAgentToolStatus,
    event_id: &str,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    if completed_status == LocalAgentToolStatus::NeedsReview {
        sqlx::query(
            "UPDATE local_agent_runs SET status = 'needs_review', version = version + 1, \
             updated_at_unix_ms = ? WHERE run_id = ? AND status = 'waiting_tool_result'",
        )
        .bind(now_unix_ms)
        .bind(run_id)
        .execute(&mut *connection)
        .await?;
        return Ok(());
    }
    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM local_agent_tool_invocations WHERE run_id = ? AND batch_id = ? \
         AND status IN ('pending','running')",
    )
    .bind(run_id)
    .bind(batch_id)
    .fetch_one(&mut *connection)
    .await?;
    if remaining > 0 {
        return Ok(());
    }
    let rows = sqlx::query(
        "SELECT invocation_id, run_id, batch_id, call_id, tool_name, arguments_json, \
         side_effecting, status, result_json, error_text, version, claim_token, \
         claim_until_unix_ms, created_at_unix_ms, updated_at_unix_ms \
         FROM local_agent_tool_invocations WHERE run_id = ? AND batch_id = ? \
         ORDER BY invocation_id",
    )
    .bind(run_id)
    .bind(batch_id)
    .fetch_all(&mut *connection)
    .await?;
    let invocations = rows
        .into_iter()
        .map(decode_invocation)
        .collect::<Result<Vec<_>, _>>()?;
    let updated = sqlx::query(
        "UPDATE local_agent_runs SET status = 'continuation_ready', version = version + 1, \
         pending_tool_batch_json = NULL, updated_at_unix_ms = ? \
         WHERE run_id = ? AND status = 'waiting_tool_result'",
    )
    .bind(now_unix_ms)
    .bind(run_id)
    .execute(&mut *connection)
    .await?;
    if updated.rows_affected() == 1 {
        SqliteClientStorage::insert_event(
            connection,
            event_id,
            run_id,
            "tool_batch_completed",
            &json!({"batch_id": batch_id, "invocations": invocations}),
            now_unix_ms,
        )
        .await?;
    }
    Ok(())
}

async fn fetch_invocation(
    connection: &mut SqliteConnection,
    invocation_id: &str,
) -> Result<Option<LocalAgentToolInvocationRecord>, ClientStorageError> {
    sqlx::query(
        "SELECT invocation_id, run_id, batch_id, call_id, tool_name, arguments_json, \
         side_effecting, status, result_json, error_text, version, claim_token, \
         claim_until_unix_ms, created_at_unix_ms, updated_at_unix_ms \
         FROM local_agent_tool_invocations WHERE invocation_id = ?",
    )
    .bind(invocation_id)
    .fetch_optional(&mut *connection)
    .await?
    .map(decode_invocation)
    .transpose()
}

fn decode_invocation(row: SqliteRow) -> Result<LocalAgentToolInvocationRecord, ClientStorageError> {
    let status: String = row.try_get("status")?;
    let arguments: String = row.try_get("arguments_json")?;
    let result: Option<String> = row.try_get("result_json")?;
    Ok(LocalAgentToolInvocationRecord {
        invocation_id: row.try_get("invocation_id")?,
        run_id: row.try_get("run_id")?,
        batch_id: row.try_get("batch_id")?,
        call_id: row.try_get("call_id")?,
        tool_name: row.try_get("tool_name")?,
        arguments: serde_json::from_str(&arguments)?,
        side_effecting: row.try_get("side_effecting")?,
        status: LocalAgentToolStatus::from_str(&status)
            .map_err(ClientStorageError::InvalidState)?,
        result: result
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        error: row.try_get("error_text")?,
        version: u64::try_from(row.try_get::<i64, _>("version")?).map_err(|_| {
            ClientStorageError::InvalidState("invalid tool invocation version".to_string())
        })?,
        claim_token: row.try_get("claim_token")?,
        claim_until_unix_ms: row.try_get("claim_until_unix_ms")?,
        created_at_unix_ms: row.try_get("created_at_unix_ms")?,
        updated_at_unix_ms: row.try_get("updated_at_unix_ms")?,
    })
}

fn outcome_fields(
    outcome: &LocalAgentToolOutcome,
) -> (LocalAgentToolStatus, Option<Value>, Option<&str>) {
    match outcome {
        LocalAgentToolOutcome::Succeeded { output } => {
            (LocalAgentToolStatus::Succeeded, Some(output.clone()), None)
        }
        LocalAgentToolOutcome::Failed { error, detail } => (
            LocalAgentToolStatus::Failed,
            Some(detail.clone()),
            Some(error.as_str()),
        ),
        LocalAgentToolOutcome::NeedsReview { reason, detail } => (
            LocalAgentToolStatus::NeedsReview,
            Some(detail.clone()),
            Some(reason.as_str()),
        ),
    }
}
