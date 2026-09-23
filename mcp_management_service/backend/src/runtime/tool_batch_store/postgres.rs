// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;

use super::{RuntimeToolBatchPendingEvent, RuntimeToolBatchRecord, RuntimeToolBatchStatus};

pub(super) async fn load_batch(
    pool: &chatos_postgres::PgPool,
    batch_id: &str,
) -> Result<Option<RuntimeToolBatchRecord>, String> {
    load_batch_by(
        pool,
        "SELECT data FROM mcp_management_runtime_tool_batches WHERE batch_id=$1",
        batch_id,
    )
    .await
}

pub(super) async fn load_batch_by(
    pool: &chatos_postgres::PgPool,
    query: &str,
    value: &str,
) -> Result<Option<RuntimeToolBatchRecord>, String> {
    sqlx::query_scalar::<_, Json<serde_json::Value>>(query)
        .bind(value)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("load Runtime Tool Batch failed: {error}"))?
        .map(|value| serde_json::from_value(value.0).map_err(|error| error.to_string()))
        .transpose()
}

pub(super) async fn load_batches(
    pool: &chatos_postgres::PgPool,
    query: &str,
    limit: usize,
) -> Result<Vec<RuntimeToolBatchRecord>, String> {
    sqlx::query_scalar::<_, Json<serde_json::Value>>(query)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(pool)
        .await
        .map_err(|error| format!("list Runtime Tool Batches failed: {error}"))?
        .into_iter()
        .map(|value| serde_json::from_value(value.0).map_err(|error| error.to_string()))
        .collect()
}

pub(super) async fn persist_batch<'e, E>(
    executor: E,
    record: &RuntimeToolBatchRecord,
    insert_only: bool,
) -> Result<bool, String>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let status = match record.status {
        RuntimeToolBatchStatus::Active => "active",
        RuntimeToolBatchStatus::Completed => "completed",
    };
    let pending_event_type = record.pending_event.as_ref().map(|event| match event {
        RuntimeToolBatchPendingEvent::InvocationReady { .. } => "invocation_ready",
        RuntimeToolBatchPendingEvent::AggregateResult => "aggregate_result",
    });
    let waiting_user_prompt_ids = record
        .waiting_user_prompt_ids
        .iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    let data = serde_json::to_value(record)
        .map(Json)
        .map_err(|error| error.to_string())?;
    let conflict = if insert_only {
        "DO NOTHING"
    } else {
        "DO UPDATE SET session_id=EXCLUDED.session_id,status=EXCLUDED.status,next_call_index=EXCLUDED.next_call_index, \
         pending_event_type=EXCLUDED.pending_event_type,revision=EXCLUDED.revision,invocation_ids=EXCLUDED.invocation_ids, \
         waiting_user_prompt_ids=EXCLUDED.waiting_user_prompt_ids,updated_at_unix_ms=EXCLUDED.updated_at_unix_ms, \
         expires_at=EXCLUDED.expires_at,expires_at_unix=EXCLUDED.expires_at_unix,data=EXCLUDED.data"
    };
    let query = format!(
        "INSERT INTO mcp_management_runtime_tool_batches \
         (batch_id,session_id,status,next_call_index,pending_event_type,revision,invocation_ids,waiting_user_prompt_ids, \
          created_at_unix_ms,updated_at_unix_ms,expires_at,expires_at_unix,data) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT(batch_id) {conflict}"
    );
    sqlx::query(&query)
        .bind(&record.batch_id)
        .bind(&record.session_id)
        .bind(status)
        .bind(i64::try_from(record.next_call_index).unwrap_or(i64::MAX))
        .bind(pending_event_type)
        .bind(record.revision)
        .bind(&record.invocation_ids)
        .bind(waiting_user_prompt_ids)
        .bind(record.created_at_unix_ms)
        .bind(record.updated_at_unix_ms)
        .bind(record.expires_at)
        .bind(record.expires_at_unix)
        .bind(data)
        .execute(executor)
        .await
        .map(|result| result.rows_affected() == 1)
        .map_err(|error| format!("persist Runtime Tool Batch failed: {error}"))
}
