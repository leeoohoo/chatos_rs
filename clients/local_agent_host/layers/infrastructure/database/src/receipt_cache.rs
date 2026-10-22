// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteClientStorage, SqliteResultExt};
use serde::Serialize;
use sqlx::{Row, SqliteConnection};
use std::io::{self, Write};

// Share exactly the same eviction program between inserts, body updates,
// policy changes and the initial migration. Only size metadata is scanned.
macro_rules! prune_bytes {
    () => {
        r#"
        DELETE FROM local_agent_command_receipts WHERE command_id IN (
            SELECT command_id FROM local_agent_receipt_sizes
            WHERE response_bytes > (
                SELECT budget_bytes FROM local_agent_receipt_cache_usage WHERE singleton = 1
            )
        );
        DELETE FROM local_agent_command_receipts WHERE command_id IN (
            SELECT command_id FROM (
                SELECT r.command_id, u.budget_bytes,
                    SUM(s.response_bytes) OVER (
                        ORDER BY r.created_at_unix_ms DESC, r.command_id DESC
                        ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
                    ) AS retained_bytes
                FROM local_agent_receipt_cache_usage u
                CROSS JOIN local_agent_command_receipts r
                CROSS JOIN local_agent_receipt_sizes s
                WHERE u.singleton = 1 AND u.response_bytes > u.budget_bytes
                    AND s.command_id = r.command_id
            ) WHERE retained_bytes > budget_bytes
        );
    "#
    };
}

pub(super) use prune_bytes;

pub(super) const SCHEMA_V36: &[&str] = &[
    "DROP TRIGGER local_agent_command_receipts_prune",
    "CREATE TABLE local_agent_receipt_cache_usage (\
       singleton INTEGER PRIMARY KEY CHECK(singleton = 1),\
       response_bytes INTEGER NOT NULL CHECK(response_bytes >= 0),\
       response_count INTEGER NOT NULL CHECK(response_count >= 0),\
       budget_bytes INTEGER NOT NULL CHECK(budget_bytes > 0 AND budget_bytes <= 134217728)\
     )",
    "INSERT INTO local_agent_receipt_cache_usage VALUES(1, 0, 0, 134217728)",
    "CREATE TABLE local_agent_receipt_sizes (\
       command_id TEXT PRIMARY KEY NOT NULL REFERENCES local_agent_command_receipts(command_id) \
         ON DELETE CASCADE ON UPDATE CASCADE,\
       response_bytes INTEGER NOT NULL CHECK(response_bytes >= 0)\
     )",
    "CREATE INDEX local_agent_receipt_sizes_bytes ON local_agent_receipt_sizes(response_bytes)",
    "CREATE TRIGGER local_agent_receipt_sizes_insert AFTER INSERT ON local_agent_receipt_sizes \
       BEGIN UPDATE local_agent_receipt_cache_usage SET \
         response_bytes = response_bytes + NEW.response_bytes, response_count = response_count + 1 \
         WHERE singleton = 1; END",
    "CREATE TRIGGER local_agent_receipt_sizes_delete AFTER DELETE ON local_agent_receipt_sizes \
       BEGIN UPDATE local_agent_receipt_cache_usage SET \
         response_bytes = response_bytes - OLD.response_bytes, response_count = response_count - 1 \
         WHERE singleton = 1; END",
    "CREATE TRIGGER local_agent_receipt_sizes_update AFTER UPDATE OF response_bytes \
       ON local_agent_receipt_sizes BEGIN UPDATE local_agent_receipt_cache_usage SET \
         response_bytes = response_bytes - OLD.response_bytes + NEW.response_bytes \
         WHERE singleton = 1; END",
    concat!(
        "CREATE TRIGGER local_agent_command_receipts_prune AFTER INSERT ON \
         local_agent_command_receipts BEGIN \
           INSERT INTO local_agent_receipt_sizes VALUES(NEW.command_id, length(CAST(NEW.response_json AS BLOB))); \
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
           INSERT INTO local_agent_receipt_sizes VALUES(NEW.command_id, length(CAST(NEW.response_json AS BLOB))) \
             ON CONFLICT(command_id) DO UPDATE SET response_bytes = excluded.response_bytes; ",
        prune_bytes!(),
        "END"
    ),
    concat!(
        "CREATE TRIGGER local_agent_receipt_cache_rebudget AFTER UPDATE OF budget_bytes \
         ON local_agent_receipt_cache_usage BEGIN ",
        prune_bytes!(),
        "END"
    ),
];

/// Logical response JSON bytes charged per command, including referenced bodies
/// even when physically shared. Not database/WAL file size or tombstone usage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptCacheUsage {
    pub response_bytes: i64,
    pub response_count: i64,
    pub budget_bytes: i64,
}

impl SqliteClientStorage {
    /// O(1) counters maintained in the same transaction as response mutations.
    pub async fn receipt_cache_usage(&self) -> Result<ReceiptCacheUsage, ClientStorageError> {
        let row = sqlx::query(
            "SELECT response_bytes, response_count, budget_bytes \
             FROM local_agent_receipt_cache_usage WHERE singleton = 1",
        )
        .fetch_one(&self.pool)
        .await
        .db()?;
        Ok(ReceiptCacheUsage {
            response_bytes: row.try_get("response_bytes").db()?,
            response_count: row.try_get("response_count").db()?,
            budget_bytes: row.try_get("budget_bytes").db()?,
        })
    }
}

pub(super) async fn backfill_sizes(
    connection: &mut SqliteConnection,
) -> Result<(), ClientStorageError> {
    // SQLite measures UTF-8 bytes without pulling historical bodies into Rust.
    // v35 has already preserved all identities before any v36 eviction occurs.
    sqlx::query(
        "INSERT INTO local_agent_receipt_sizes(command_id, response_bytes) \
         SELECT command_id, length(CAST(response_json AS BLOB)) \
         FROM local_agent_command_receipts ORDER BY command_id",
    )
    .execute(&mut *connection)
    .await
    .db()?;
    #[cfg(all(test, unix))]
    super::migration_crash_tests::checkpoint("sizes_backfilled").await;
    sqlx::raw_sql(prune_bytes!())
        .execute(&mut *connection)
        .await
        .db()?;
    Ok(())
}

/// Continue serializing after overflow to preserve validation/error semantics,
/// but stop retaining bytes. Never build a full oversized JSON String first.
pub(super) fn serialize_bounded<T: Serialize>(
    response: &T,
    limit: usize,
) -> Result<Option<String>, ClientStorageError> {
    let mut writer = CappedResponseWriter {
        bytes: Vec::new(),
        limit,
        overflowed: false,
    };
    serde_json::to_writer(&mut writer, response)?;
    if writer.overflowed {
        return Ok(None);
    }
    String::from_utf8(writer.bytes)
        .map(Some)
        .map_err(|error| ClientStorageError::InvalidState(error.to_string()))
}

struct CappedResponseWriter {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}

impl Write for CappedResponseWriter {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if !self.overflowed {
            if input.len() > self.limit.saturating_sub(self.bytes.len()) {
                self.overflowed = true;
                self.bytes = Vec::new();
            } else {
                let required = self.bytes.len() + input.len();
                if required > self.bytes.capacity() {
                    let capacity = required
                        .max(self.bytes.capacity().saturating_mul(2).max(4096))
                        .min(self.limit);
                    self.bytes.reserve_exact(capacity - self.bytes.len());
                }
                self.bytes.extend_from_slice(input);
            }
        }
        Ok(input.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialization_keeps_exact_budget_boundary_including_utf8_and_escaping() {
        for value in ["hello", "你好🌏", "line\nquote\""] {
            let json = serde_json::to_string(value).unwrap();
            assert_eq!(
                serialize_bounded(&value, json.len()).unwrap(),
                Some(json.clone())
            );
            assert!(serialize_bounded(&value, json.len() - 1).unwrap().is_none());
        }
    }

    #[test]
    fn writer_growth_is_capped_and_overflow_releases_retained_buffer() {
        let mut writer = CappedResponseWriter {
            bytes: Vec::new(),
            limit: 10,
            overflowed: false,
        };
        for _ in 0..10 {
            writer.write_all(b"x").unwrap();
            assert!(writer.bytes.capacity() <= writer.limit);
        }
        assert_eq!(writer.bytes.len(), 10);
        writer.write_all(b"x").unwrap();
        assert!(writer.overflowed);
        assert_eq!(writer.bytes.capacity(), 0);
        writer.write_all(b"ignored data after overflow").unwrap();
        assert_eq!(writer.bytes.capacity(), 0);
    }
}
