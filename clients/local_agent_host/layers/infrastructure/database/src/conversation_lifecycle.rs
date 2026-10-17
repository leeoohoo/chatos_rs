// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{conversation_store::next_message_ordinal, ClientStorageError, SqliteResultExt};
use chatos_local_agent_protocol::{LocalAgentRunRecord, LocalAgentRunStatus};
use serde_json::json;
use sqlx::{Row, SqliteConnection};

pub(super) async fn reconcile_conversation_after_run(
    connection: &mut SqliteConnection,
    run: &LocalAgentRunRecord,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    if run.owner_entity_type != "conversation_turn"
        || (!run.status.is_terminal() && run.status != LocalAgentRunStatus::NeedsReview)
    {
        return Ok(());
    }
    let row = sqlx::query(
        "SELECT turn_id, conversation_id FROM local_conversation_turns WHERE run_id = ?",
    )
    .bind(&run.run_id)
    .fetch_optional(&mut *connection)
    .await
    .db()?;
    let Some(row) = row else { return Ok(()) };
    let turn_id: String = row.try_get("turn_id").db()?;
    let conversation_id: String = row.try_get("conversation_id").db()?;
    let status = match run.status {
        LocalAgentRunStatus::Succeeded => "succeeded",
        LocalAgentRunStatus::Failed => "failed",
        LocalAgentRunStatus::Cancelled => "cancelled",
        // A lost Host claim cannot be replayed safely. Close the visible Turn as failed so
        // Main Chat becomes writable again while preserving the Run's more precise
        // needs_review state and diagnostic payload.
        LocalAgentRunStatus::NeedsReview => "failed",
        _ => return Ok(()),
    };
    let updated = sqlx::query(
        "UPDATE local_conversation_turns SET status = ?, updated_at_unix_ms = ? \
         WHERE turn_id = ? AND run_id = ? AND status = 'running'",
    )
    .bind(status)
    .bind(now_unix_ms)
    .bind(&turn_id)
    .bind(&run.run_id)
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "conversation Turn changed while reconciling: {turn_id}"
        )));
    }
    if matches!(
        run.status,
        LocalAgentRunStatus::Succeeded
            | LocalAgentRunStatus::Failed
            | LocalAgentRunStatus::NeedsReview
    ) {
        let ordinal = next_message_ordinal(connection, &conversation_id).await?;
        let content = run
            .terminal_outcome
            .clone()
            .unwrap_or_else(|| match run.status {
                LocalAgentRunStatus::Failed => {
                    json!({"error": "Local model execution failed before producing a response."})
                }
                LocalAgentRunStatus::NeedsReview => json!({
                    "status": "needs_review",
                    "reason_code": "execution_interrupted",
                    "error": "Execution stopped before the current step result could be confirmed."
                }),
                _ => serde_json::Value::Null,
            });
        sqlx::query(
            "INSERT INTO local_conversation_messages(\
             message_id, conversation_id, turn_id, ordinal, role, content_json, metadata_json, \
             created_at_unix_ms) VALUES(?, ?, ?, ?, 'assistant', ?, ?, ?)",
        )
        .bind(format!("assistant:{}", run.run_id))
        .bind(&conversation_id)
        .bind(&turn_id)
        .bind(ordinal)
        .bind(serde_json::to_string(&content)?)
        .bind(serde_json::to_string(&json!({
            "run_id": run.run_id,
            "terminal_status": run.status.as_str(),
        }))?)
        .bind(now_unix_ms)
        .execute(&mut *connection)
        .await
        .db()?;
    }
    sqlx::query(
        "UPDATE local_conversations SET version = version + 1, updated_at_unix_ms = ? \
         WHERE conversation_id = ?",
    )
    .bind(now_unix_ms)
    .bind(&conversation_id)
    .execute(&mut *connection)
    .await
    .db()?;
    Ok(())
}
