// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{receipt_cache::prune_bytes, ClientStorageError, SqliteResultExt};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};

pub(super) const INLINE_LIMIT: usize = 512 * 1024;

// Set up first, convert legacy bodies, then install the v37 receipt triggers.
// All three steps run in the same migration transaction.
pub(super) const SCHEMA_V37: &[&str] = &[
    "CREATE TABLE local_agent_receipt_results (\
       result_digest TEXT PRIMARY KEY NOT NULL CHECK(length(result_digest) = 74 AND \
         substr(result_digest, 1, 10) = 'sha256:v1:'),\
       response_json TEXT NOT NULL,\
       response_bytes INTEGER NOT NULL CHECK(response_bytes > 524288 AND \
         response_bytes <= 134217728 AND response_bytes = length(CAST(response_json AS BLOB)))\
     )",
    "ALTER TABLE local_agent_command_receipts ADD COLUMN result_digest TEXT \
       REFERENCES local_agent_receipt_results(result_digest)",
    "CREATE INDEX local_agent_command_receipts_result ON local_agent_command_receipts(result_digest) \
       WHERE result_digest IS NOT NULL",
    "CREATE TRIGGER local_agent_receipt_results_immutable BEFORE UPDATE ON \
       local_agent_receipt_results BEGIN SELECT RAISE(ABORT, 'receipt result is immutable'); END",
    "DROP TRIGGER local_agent_command_receipts_prune",
    "DROP TRIGGER local_agent_command_receipts_resize",
];

macro_rules! response_size {
    () => {
        "CASE WHEN NEW.result_digest IS NULL THEN length(CAST(NEW.response_json AS BLOB)) \
         ELSE (SELECT response_bytes FROM local_agent_receipt_results \
           WHERE result_digest = NEW.result_digest) END"
    };
}

pub(super) const FINISH_SCHEMA_V37: &[&str] = &[
    "CREATE TRIGGER local_agent_command_receipts_validate BEFORE INSERT ON \
       local_agent_command_receipts WHEN \
         (NEW.result_digest IS NULL AND length(CAST(NEW.response_json AS BLOB)) > 524288) OR \
         (NEW.result_digest IS NOT NULL AND NEW.response_json != '') \
       BEGIN SELECT RAISE(ABORT, 'invalid inline receipt or result reference'); END",
    "CREATE TRIGGER local_agent_command_receipts_validate_update BEFORE UPDATE OF \
       response_json, result_digest ON local_agent_command_receipts WHEN \
         NEW.result_digest IS NOT OLD.result_digest OR \
         (NEW.result_digest IS NULL AND length(CAST(NEW.response_json AS BLOB)) > 524288) OR \
         (NEW.result_digest IS NOT NULL AND NEW.response_json != '') \
       BEGIN SELECT RAISE(ABORT, 'receipt reference is immutable or inline limit exceeded'); END",
    "CREATE TRIGGER local_agent_command_receipts_collect AFTER DELETE ON \
       local_agent_command_receipts WHEN OLD.result_digest IS NOT NULL BEGIN \
         DELETE FROM local_agent_receipt_results WHERE result_digest = OLD.result_digest AND \
           NOT EXISTS(SELECT 1 FROM local_agent_command_receipts \
             WHERE result_digest = OLD.result_digest); END",
    concat!(
        "CREATE TRIGGER local_agent_command_receipts_prune AFTER INSERT ON \
         local_agent_command_receipts BEGIN \
           INSERT INTO local_agent_receipt_sizes VALUES(NEW.command_id, ",
        response_size!(),
        "); \
           DELETE FROM local_agent_command_receipts WHERE created_at_unix_ms < (\
             SELECT MAX(created_at_unix_ms) - 86400000 FROM local_agent_command_receipts); \
           DELETE FROM local_agent_command_receipts WHERE command_id IN (\
             SELECT r.command_id FROM local_agent_receipt_cache_usage u \
             CROSS JOIN local_agent_command_receipts r \
             WHERE u.singleton = 1 AND u.response_count > 5000 \
             ORDER BY r.created_at_unix_ms DESC, r.command_id DESC LIMIT -1 OFFSET 5000); ",
        prune_bytes!(),
        "END"
    ),
    concat!(
        "CREATE TRIGGER local_agent_command_receipts_resize AFTER UPDATE OF response_json \
         ON local_agent_command_receipts BEGIN \
           INSERT INTO local_agent_receipt_sizes VALUES(NEW.command_id, ",
        response_size!(),
        ") ON CONFLICT(command_id) DO UPDATE SET response_bytes = excluded.response_bytes; ",
        prune_bytes!(),
        "END"
    ),
];

fn digest(json: &str) -> String {
    format!("sha256:v1:{:x}", Sha256::digest(json.as_bytes()))
}

/// The caller holds the business/migration transaction. Shared bodies are
/// immutable and are collected only when their last receipt is deleted.
pub(super) async fn store(
    connection: &mut SqliteConnection,
    json: &str,
) -> Result<String, ClientStorageError> {
    let digest = digest(json);
    let inserted = sqlx::query(
        "INSERT INTO local_agent_receipt_results(result_digest, response_json, response_bytes) \
         VALUES(?, ?, ?) ON CONFLICT(result_digest) DO NOTHING",
    )
    .bind(&digest)
    .bind(json)
    .bind(json.len() as i64)
    .execute(&mut *connection)
    .await
    .db()?;
    if inserted.rows_affected() == 0 {
        // Do not silently attach a new successful command to a corrupted blob
        // (or a hash collision). Compare inside SQLite, without fetching a copy.
        let identical: bool = sqlx::query_scalar(
            "SELECT response_json = ? AND response_bytes = ? FROM local_agent_receipt_results \
             WHERE result_digest = ?",
        )
        .bind(json)
        .bind(json.len() as i64)
        .bind(&digest)
        .fetch_one(&mut *connection)
        .await
        .db()?;
        if !identical {
            return Err(ClientStorageError::InvalidState(
                "receipt result digest collision or corrupt shared body".into(),
            ));
        }
    }
    Ok(digest)
}

pub(super) async fn load(
    connection: &mut SqliteConnection,
    reference: &str,
) -> Result<String, ClientStorageError> {
    let row = sqlx::query(
        "SELECT response_json, response_bytes FROM local_agent_receipt_results WHERE result_digest = ?",
    )
    .bind(reference)
    .fetch_optional(&mut *connection)
    .await
    .db()?
    .ok_or_else(|| ClientStorageError::InvalidState("missing immutable receipt result".into()))?;
    let json: String = row.try_get("response_json").db()?;
    let bytes: i64 = row.try_get("response_bytes").db()?;
    if json.len() <= INLINE_LIMIT || bytes != json.len() as i64 || digest(&json) != reference {
        return Err(ClientStorageError::InvalidState(
            "immutable receipt result failed length/digest verification".into(),
        ));
    }
    Ok(json)
}

pub(super) async fn backfill_results(
    connection: &mut SqliteConnection,
) -> Result<(), ClientStorageError> {
    let mut after = String::new();
    loop {
        // Filter using v36 size metadata, not repeated historical-body scans.
        let row = sqlx::query(
            "SELECT r.command_id, r.response_json FROM local_agent_command_receipts r \
             JOIN local_agent_receipt_sizes s ON s.command_id = r.command_id \
             WHERE s.response_bytes > ? AND r.result_digest IS NULL \
               AND (? = '' OR r.command_id > ?) ORDER BY r.command_id LIMIT 1",
        )
        .bind(INLINE_LIMIT as i64)
        .bind(&after)
        .bind(&after)
        .fetch_optional(&mut *connection)
        .await
        .db()?;
        let Some(row) = row else { break };
        let command_id: String = row.try_get("command_id").db()?;
        let json: String = row.try_get("response_json").db()?;
        let reference = store(connection, &json).await?;
        sqlx::query("UPDATE local_agent_command_receipts SET response_json = '', result_digest = ? WHERE command_id = ?")
            .bind(reference)
            .bind(&command_id)
            .execute(&mut *connection)
            .await
            .db()?;
        // Logical per-command sizes remain unchanged, including shared bodies.
        after = command_id;
        #[cfg(all(test, unix))]
        super::migration_crash_tests::checkpoint("result_row_converted").await;
    }
    Ok(())
}
