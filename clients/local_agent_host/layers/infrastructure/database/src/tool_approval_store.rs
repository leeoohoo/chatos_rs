// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    tool_store::{advance_run_after_tool, decode_invocation, fetch_invocation},
    ClientStorageError, IdempotentCommand, SqliteClientStorage, SqliteResultExt,
};
use chatos_local_agent_protocol::{
    LocalAgentToolApprovalDecision, LocalAgentToolApprovalResult, LocalAgentToolInvocationRecord,
    LocalAgentToolStatus,
};
use serde_json::json;
use sqlx::Row;
pub(super) async fn list_pending(
    storage: &SqliteClientStorage,
    owner_user_id: &str,
    limit: u32,
) -> Result<Vec<LocalAgentToolInvocationRecord>, ClientStorageError> {
    if owner_user_id.trim().is_empty()
        || owner_user_id.len() > 256
        || owner_user_id.chars().any(char::is_control)
        || !(1..=100).contains(&limit)
    {
        return Err(ClientStorageError::InvalidState(
            "invalid pending tool approval query".to_string(),
        ));
    }
    let mut connection = storage.pool.acquire().await.db()?;
    sqlx::query(
        "SELECT i.invocation_id, i.run_id, i.batch_id, i.call_id, i.tool_name, \
         i.arguments_json, i.side_effecting, i.requires_approval, i.approval_status, \
         i.approval_decided_by, i.approval_reason, i.approval_decided_at_unix_ms, i.status, \
         i.result_json, i.error_text, i.version, i.claim_token, i.claim_until_unix_ms, \
         i.created_at_unix_ms, i.updated_at_unix_ms \
         FROM local_agent_tool_invocations i JOIN local_agent_runs r ON r.run_id = i.run_id \
         WHERE r.owner_user_id = ? AND r.status = 'waiting_tool_result' \
         AND i.status = 'pending' AND i.approval_status = 'pending' \
         ORDER BY i.created_at_unix_ms, i.invocation_id LIMIT ?",
    )
    .bind(owner_user_id)
    .bind(i64::from(limit))
    .fetch_all(&mut *connection)
    .await
    .db()?
    .into_iter()
    .map(decode_invocation)
    .collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn decide(
    storage: &SqliteClientStorage,
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
    let mut connection = storage.pool.acquire().await.db()?;
    SqliteClientStorage::begin_immediate(&mut connection).await?;
    let result = async {
        if let Some(replay) = SqliteClientStorage::replay(&mut connection, command).await? {
            return Ok(replay);
        }
        let current = fetch_invocation(&mut connection, invocation_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(invocation_id.to_string()))?;
        let run =
            sqlx::query("SELECT status, owner_user_id FROM local_agent_runs WHERE run_id = ?")
                .bind(&current.run_id)
                .fetch_optional(&mut *connection)
                .await
                .db()?
                .ok_or_else(|| ClientStorageError::NotFound(current.run_id.clone()))?;
        let run_status: String = run.try_get("status").db()?;
        let run_owner_user_id: String = run.try_get("owner_user_id").db()?;
        if !current.requires_approval
            || current.approval_status.as_str() != "pending"
            || current.status != LocalAgentToolStatus::Pending
            || current.version != expected_version
            || run_status != "waiting_tool_result"
            || run_owner_user_id != owner_user_id
        {
            return Err(ClientStorageError::Conflict(format!(
                "tool approval state changed: {invocation_id}"
            )));
        }
        let (approval_status, invocation_status, error_text, event_type) = match decision {
            LocalAgentToolApprovalDecision::Approve => {
                ("approved", "pending", None, "tool_invocation_approved")
            }
            LocalAgentToolApprovalDecision::Reject => (
                "rejected",
                "failed",
                Some(reason),
                "tool_invocation_rejected",
            ),
        };
        let updated = sqlx::query(
            "UPDATE local_agent_tool_invocations SET approval_status = ?, status = ?, \
             approval_decided_by = ?, approval_reason = ?, approval_decided_at_unix_ms = ?, \
             error_text = ?, version = version + 1, updated_at_unix_ms = ? \
             WHERE invocation_id = ? AND version = ? AND status = 'pending' \
             AND approval_status = 'pending'",
        )
        .bind(approval_status)
        .bind(invocation_status)
        .bind(decided_by)
        .bind(reason)
        .bind(now_unix_ms)
        .bind(error_text)
        .bind(now_unix_ms)
        .bind(invocation_id)
        .bind(expected_version as i64)
        .execute(&mut *connection)
        .await
        .db()?;
        if updated.rows_affected() != 1 {
            return Err(ClientStorageError::Conflict(format!(
                "tool approval state changed: {invocation_id}"
            )));
        }
        SqliteClientStorage::insert_event(
            &mut connection,
            event_id,
            &current.run_id,
            event_type,
            &json!({
                "invocation_id": invocation_id,
                "call_id": current.call_id,
                "tool_name": current.tool_name,
                "decision": decision,
                "decided_by": decided_by,
                "reason": reason
            }),
            now_unix_ms,
        )
        .await?;
        if decision == LocalAgentToolApprovalDecision::Reject {
            advance_run_after_tool(
                &mut connection,
                &current.run_id,
                &current.batch_id,
                LocalAgentToolStatus::Failed,
                batch_event_id,
                now_unix_ms,
            )
            .await?;
        }
        let invocation = fetch_invocation(&mut connection, invocation_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(invocation_id.to_string()))?;
        let run = SqliteClientStorage::fetch_run_on(&mut connection, &current.run_id)
            .await?
            .ok_or_else(|| ClientStorageError::NotFound(current.run_id.clone()))?;
        let response = LocalAgentToolApprovalResult { invocation, run };
        SqliteClientStorage::record_receipt(&mut connection, command, &response, now_unix_ms)
            .await?;
        Ok(response)
    }
    .await;
    SqliteClientStorage::finish_write(&mut connection, result).await
}
