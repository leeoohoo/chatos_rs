// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteResultExt};
use serde_json::{json, Value};
use sqlx::{Row, SqliteConnection};

pub(crate) async fn attach_pending_guidance_to_claim(
    connection: &mut SqliteConnection,
    run_id: &str,
    run_version: u64,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    let rows = sqlx::query(
        "SELECT message_id, payload_json FROM local_conversation_guidance \
         WHERE run_id = ? AND delivered_run_version IS NULL \
         ORDER BY created_at_unix_ms, message_id",
    )
    .bind(run_id)
    .fetch_all(&mut *connection)
    .await
    .db()?;
    if rows.is_empty() {
        return Ok(());
    }
    let mut message_ids = Vec::with_capacity(rows.len());
    let mut guidance = Vec::with_capacity(rows.len());
    for row in rows {
        message_ids.push(row.try_get::<String, _>("message_id").db()?);
        let payload: String = row.try_get("payload_json").db()?;
        guidance.push(serde_json::from_str(&payload)?);
    }
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT continuation_input_json FROM local_agent_runs \
         WHERE run_id = ? AND status = 'model_running' AND version = ?",
    )
    .bind(run_id)
    .bind(run_version as i64)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .flatten();
    let continuation = merge_guidance(existing.as_deref(), guidance)?;
    let updated = sqlx::query(
        "UPDATE local_agent_runs SET continuation_input_json = ? \
         WHERE run_id = ? AND status = 'model_running' AND version = ?",
    )
    .bind(serde_json::to_string(&continuation)?)
    .bind(run_id)
    .bind(run_version as i64)
    .execute(&mut *connection)
    .await
    .db()?;
    if updated.rows_affected() != 1 {
        return Err(ClientStorageError::Conflict(format!(
            "run changed while attaching guidance: {run_id}"
        )));
    }
    for message_id in message_ids {
        let delivered = sqlx::query(
            "UPDATE local_conversation_guidance SET delivered_run_version = ?, \
             delivered_at_unix_ms = ? WHERE message_id = ? AND delivered_run_version IS NULL",
        )
        .bind(run_version as i64)
        .bind(now_unix_ms)
        .bind(&message_id)
        .execute(&mut *connection)
        .await
        .db()?;
        if delivered.rows_affected() != 1 {
            return Err(ClientStorageError::Conflict(format!(
                "guidance changed while claiming: {message_id}"
            )));
        }
    }
    Ok(())
}

fn merge_guidance(
    existing: Option<&str>,
    mut pending: Vec<Value>,
) -> Result<Value, ClientStorageError> {
    let mut continuation = existing
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or_else(|| json!({"type": "guidance"}));
    let object = continuation.as_object_mut().ok_or_else(|| {
        ClientStorageError::InvalidState(
            "Run continuation must be an object before adding guidance".to_string(),
        )
    })?;
    let guidance = object
        .entry("guidance")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| {
            ClientStorageError::InvalidState(
                "Run continuation guidance must be an array".to_string(),
            )
        })?;
    guidance.append(&mut pending);
    Ok(continuation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guidance_merges_with_existing_tool_continuation() {
        let merged = merge_guidance(
            Some(r#"{"type":"tool_results","invocations":[]}"#),
            vec![json!({"message": "continue"})],
        )
        .expect("merge");
        assert_eq!(merged["type"], "tool_results");
        assert_eq!(merged["guidance"][0]["message"], "continue");
    }
}
