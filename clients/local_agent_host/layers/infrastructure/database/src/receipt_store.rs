// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    receipt_fingerprint, ClientStorageError, IdempotentCommand, SqliteClientStorage,
    SqliteResultExt,
};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{Row, SqliteConnection};

impl SqliteClientStorage {
    pub(crate) async fn replay<T: DeserializeOwned>(
        connection: &mut SqliteConnection,
        command: &IdempotentCommand,
    ) -> Result<Option<T>, ClientStorageError> {
        if !command.persist_receipt {
            return Ok(None);
        }
        let fingerprint: Option<String> = sqlx::query_scalar(
            "SELECT request_fingerprint FROM local_agent_command_tombstones WHERE command_id = ?",
        )
        .bind(&command.command_id)
        .fetch_optional(&mut *connection)
        .await
        .db()?;
        let Some(fingerprint) = fingerprint else {
            return Ok(None);
        };
        // Changed input remains a mismatch even after eviction; expiration must
        // never turn a reused command ID into a fresh operation.
        if !receipt_fingerprint::matches(&fingerprint, &command.request_fingerprint) {
            return Err(ClientStorageError::CommandMismatch(
                command.command_id.clone(),
            ));
        }
        // Only load a potentially large historical response after identity
        // verification. Normal write callers already hold BEGIN IMMEDIATE.
        let receipt = sqlx::query(
            "SELECT response_json, result_digest FROM local_agent_command_receipts WHERE command_id = ?",
        )
        .bind(&command.command_id)
        .fetch_optional(&mut *connection)
        .await
        .db()?;
        let receipt =
            receipt.ok_or_else(|| ClientStorageError::ReplayExpired(command.command_id.clone()))?;
        let reference: Option<String> = receipt.try_get("result_digest").db()?;
        let response = if let Some(reference) = reference {
            super::receipt_result::load(connection, &reference).await?
        } else {
            receipt.try_get("response_json").db()?
        };
        Ok(Some(serde_json::from_str(&response)?))
    }

    pub(crate) async fn record_receipt<T: Serialize>(
        connection: &mut SqliteConnection,
        command: &IdempotentCommand,
        response: &T,
        now_unix_ms: i64,
    ) -> Result<(), ClientStorageError> {
        if !command.persist_receipt {
            return Ok(());
        }
        let budget: i64 = sqlx::query_scalar(
            "SELECT budget_bytes FROM local_agent_receipt_cache_usage WHERE singleton = 1",
        )
        .fetch_one(&mut *connection)
        .await
        .db()?;
        let budget = usize::try_from(budget)
            .map_err(|_| ClientStorageError::InvalidState("invalid receipt byte budget".into()))?;
        let fingerprint = receipt_fingerprint::compact_legacy(&command.request_fingerprint);
        let Some(response_json) = super::receipt_cache::serialize_bounded(response, budget)? else {
            // A single result cannot fit in the entire cache. Commit only its
            // identity; do not write a huge body/WAL just to immediately evict it.
            sqlx::query(
                "INSERT INTO local_agent_command_tombstones(\
                 command_id, request_fingerprint, completed_at_unix_ms) VALUES(?, ?, ?)",
            )
            .bind(&command.command_id)
            .bind(&fingerprint)
            .bind(now_unix_ms)
            .execute(&mut *connection)
            .await
            .db()?;
            return Ok(());
        };
        let (response_json, reference) =
            if response_json.len() > super::receipt_result::INLINE_LIMIT {
                let reference = super::receipt_result::store(connection, &response_json).await?;
                (String::new(), Some(reference))
            } else {
                (response_json, None)
            };
        // The BEFORE INSERT trigger records identity in the same business
        // transaction. Response eviction never deletes the identity table.
        sqlx::query(
            "INSERT INTO local_agent_command_receipts(\
             command_id, request_fingerprint, response_json, created_at_unix_ms, result_digest) \
             VALUES(?, ?, ?, ?, ?)",
        )
        .bind(&command.command_id)
        .bind(&fingerprint)
        .bind(response_json)
        .bind(now_unix_ms)
        .bind(reference)
        .execute(&mut *connection)
        .await
        .db()?;
        Ok(())
    }
}

/// Executed inside the v35 migration transaction. Read one identity at a time,
/// never materialize historical response bodies or duplicate a legacy request
/// JSON in the tombstone. A crash rolls back the schema and entire backfill.
pub(super) async fn backfill_tombstones(
    connection: &mut SqliteConnection,
) -> Result<(), ClientStorageError> {
    let mut after: Option<String> = None;
    loop {
        let row = if let Some(after) = &after {
            sqlx::query(
                "SELECT command_id, request_fingerprint, created_at_unix_ms \
                 FROM local_agent_command_receipts WHERE command_id > ? \
                 ORDER BY command_id LIMIT 1",
            )
            .bind(after)
            .fetch_optional(&mut *connection)
            .await
            .db()?
        } else {
            sqlx::query(
                "SELECT command_id, request_fingerprint, created_at_unix_ms \
                 FROM local_agent_command_receipts ORDER BY command_id LIMIT 1",
            )
            .fetch_optional(&mut *connection)
            .await
            .db()?
        };
        let Some(row) = row else { break };
        let command_id: String = row.try_get("command_id").db()?;
        let legacy: String = row.try_get("request_fingerprint").db()?;
        let fingerprint = receipt_fingerprint::compact_legacy(&legacy);
        let completed_at: i64 = row.try_get("created_at_unix_ms").db()?;
        sqlx::query(
            "INSERT INTO local_agent_command_tombstones(\
             command_id, request_fingerprint, completed_at_unix_ms) VALUES(?, ?, ?)",
        )
        .bind(&command_id)
        .bind(&fingerprint)
        .bind(completed_at)
        .execute(&mut *connection)
        .await
        .db()?;
        if fingerprint != legacy {
            sqlx::query("UPDATE local_agent_command_receipts SET request_fingerprint = ? WHERE command_id = ?")
                .bind(&fingerprint)
                .bind(&command_id)
                .execute(&mut *connection)
                .await
                .db()?;
        }
        after = Some(command_id);
        #[cfg(all(test, unix))]
        super::migration_crash_tests::checkpoint("identity_row_backfilled").await;
    }
    Ok(())
}
