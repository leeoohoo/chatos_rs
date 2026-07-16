// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use sqlx::{Sqlite, Transaction};

use super::{LocalMessageRecord, LocalTurnRecord, LocalTurnSnapshot};

pub(super) async fn find_turn_by_idempotency(
    transaction: &mut Transaction<'_, Sqlite>,
    owner_user_id: &str,
    session_id: &str,
    idempotency_key: &str,
) -> Result<Option<LocalTurnRecord>> {
    sqlx::query_as::<_, LocalTurnRecord>(
        r#"
        SELECT turns.id, turns.session_id, turns.user_message_id,
               turns.idempotency_key, turns.status, turns.cancel_requested,
               turns.error_code, turns.error_message, turns.started_at,
               turns.finished_at, turns.created_at, turns.updated_at
        FROM turns
        INNER JOIN sessions ON sessions.id = turns.session_id
        WHERE turns.session_id = ? AND turns.idempotency_key = ?
          AND sessions.owner_user_id = ?
        "#,
    )
    .bind(session_id)
    .bind(idempotency_key)
    .bind(owner_user_id)
    .fetch_optional(&mut **transaction)
    .await
    .context("find local turn by idempotency key")
}

pub(super) async fn find_turn_by_id(
    transaction: &mut Transaction<'_, Sqlite>,
    owner_user_id: &str,
    turn_id: &str,
) -> Result<Option<LocalTurnRecord>> {
    sqlx::query_as::<_, LocalTurnRecord>(
        r#"
        SELECT turns.id, turns.session_id, turns.user_message_id,
               turns.idempotency_key, turns.status, turns.cancel_requested,
               turns.error_code, turns.error_message, turns.started_at,
               turns.finished_at, turns.created_at, turns.updated_at
        FROM turns
        INNER JOIN sessions ON sessions.id = turns.session_id
        WHERE turns.id = ? AND sessions.owner_user_id = ?
        "#,
    )
    .bind(turn_id)
    .bind(owner_user_id)
    .fetch_optional(&mut **transaction)
    .await
    .context("find local turn by id")
}

pub(super) async fn load_turn_snapshot(
    transaction: &mut Transaction<'_, Sqlite>,
    turn: LocalTurnRecord,
) -> Result<LocalTurnSnapshot> {
    let user_message_id = turn
        .user_message_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("local turn has no user message"))?;
    let user_message = find_message(transaction, user_message_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("local turn user message is unavailable"))?;
    let assistant_message = sqlx::query_as::<_, LocalMessageRecord>(
        r#"
        SELECT id, session_id, turn_id, sequence_no, role, content, reasoning,
               tool_calls_json, tool_call_id, metadata_json, created_at
        FROM messages
        WHERE turn_id = ? AND role = 'assistant'
        ORDER BY sequence_no DESC
        LIMIT 1
        "#,
    )
    .bind(turn.id.as_str())
    .fetch_optional(&mut **transaction)
    .await
    .context("load local turn assistant message")?;
    Ok(LocalTurnSnapshot {
        turn,
        user_message,
        assistant_message,
    })
}

pub(super) async fn next_message_sequence(
    transaction: &mut Transaction<'_, Sqlite>,
    session_id: &str,
) -> Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(sequence_no), 0) + 1 FROM messages WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(&mut **transaction)
    .await
    .context("allocate local message sequence")
}

pub(super) async fn refresh_session_message_count(
    transaction: &mut Transaction<'_, Sqlite>,
    session_id: &str,
    updated_at: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE sessions
        SET message_count = (SELECT COUNT(*) FROM messages WHERE session_id = ?),
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(session_id)
    .bind(updated_at)
    .bind(session_id)
    .execute(&mut **transaction)
    .await
    .context("refresh local session message count")?;
    Ok(())
}

async fn find_message(
    transaction: &mut Transaction<'_, Sqlite>,
    message_id: &str,
) -> Result<Option<LocalMessageRecord>> {
    sqlx::query_as::<_, LocalMessageRecord>(
        r#"
        SELECT id, session_id, turn_id, sequence_no, role, content, reasoning,
               tool_calls_json, tool_call_id, metadata_json, created_at
        FROM messages
        WHERE id = ?
        "#,
    )
    .bind(message_id)
    .fetch_optional(&mut **transaction)
    .await
    .context("load local turn message")
}
