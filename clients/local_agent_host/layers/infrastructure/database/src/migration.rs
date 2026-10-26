// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    hook_event_store::SCHEMA_V39,
    plugin_hook_store::SCHEMA_V38,
    receipt_cache::SCHEMA_V36,
    receipt_result::{FINISH_SCHEMA_V37, SCHEMA_V37},
    schema::{
        SCHEMA_V1, SCHEMA_V10, SCHEMA_V11, SCHEMA_V12, SCHEMA_V13, SCHEMA_V14, SCHEMA_V15,
        SCHEMA_V16, SCHEMA_V17, SCHEMA_V18, SCHEMA_V19, SCHEMA_V2, SCHEMA_V20, SCHEMA_V21,
        SCHEMA_V22, SCHEMA_V23, SCHEMA_V24, SCHEMA_V25, SCHEMA_V26, SCHEMA_V27, SCHEMA_V28,
        SCHEMA_V29, SCHEMA_V3, SCHEMA_V30, SCHEMA_V31, SCHEMA_V32, SCHEMA_V33, SCHEMA_V34,
        SCHEMA_V35, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6, SCHEMA_V7, SCHEMA_V8, SCHEMA_V9,
    },
    ClientStorageError, SqliteClientStorage, SqliteResultExt,
};
use sqlx::SqliteConnection;

pub(super) const SCHEMA_VERSION: i64 = 39;

pub(super) const fn requires_compaction(previous_version: i64) -> bool {
    previous_version < 33
}

impl SqliteClientStorage {
    pub(super) async fn current_schema_version(&self) -> Result<i64, ClientStorageError> {
        let mut connection = self.pool.acquire().await.db()?;
        let has_migration_table: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type = 'table' AND name = 'client_schema_migrations'",
        )
        .fetch_one(&mut *connection)
        .await
        .db()?;
        if has_migration_table == 0 {
            return Ok(0);
        }
        Ok(sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(version) FROM client_schema_migrations",
        )
        .fetch_one(&mut *connection)
        .await
        .db()?
        .unwrap_or(0))
    }

    pub(super) async fn migrate(&self) -> Result<(), ClientStorageError> {
        let mut connection = self.acquire_write().await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS client_schema_migrations (\
             version INTEGER PRIMARY KEY NOT NULL, applied_at_unix_ms INTEGER NOT NULL)",
        )
        .execute(&mut *connection)
        .await
        .db()?;
        let version = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(version) FROM client_schema_migrations",
        )
        .fetch_one(&mut *connection)
        .await
        .db()?
        .unwrap_or(0);
        if version > SCHEMA_VERSION {
            return Err(ClientStorageError::InvalidState(format!(
                "database schema version {version} is newer than supported {SCHEMA_VERSION}"
            )));
        }
        for (next_version, statements) in [
            (1, SCHEMA_V1),
            (2, SCHEMA_V2),
            (3, SCHEMA_V3),
            (4, SCHEMA_V4),
            (5, SCHEMA_V5),
            (6, SCHEMA_V6),
            (7, SCHEMA_V7),
            (8, SCHEMA_V8),
            (9, SCHEMA_V9),
            (10, SCHEMA_V10),
            (11, SCHEMA_V11),
            (12, SCHEMA_V12),
            (13, SCHEMA_V13),
            (14, SCHEMA_V14),
            (15, SCHEMA_V15),
            (16, SCHEMA_V16),
            (17, SCHEMA_V17),
            (18, SCHEMA_V18),
            (19, SCHEMA_V19),
            (20, SCHEMA_V20),
            (21, SCHEMA_V21),
            (22, SCHEMA_V22),
            (23, SCHEMA_V23),
            (24, SCHEMA_V24),
            (25, SCHEMA_V25),
            (26, SCHEMA_V26),
            (27, SCHEMA_V27),
            (28, SCHEMA_V28),
            (29, SCHEMA_V29),
            (30, SCHEMA_V30),
            (31, SCHEMA_V31),
            (32, SCHEMA_V32),
            (33, SCHEMA_V33),
            (34, SCHEMA_V34),
            (35, SCHEMA_V35),
            (36, SCHEMA_V36),
            (37, SCHEMA_V37),
            (38, SCHEMA_V38),
            (39, SCHEMA_V39),
        ] {
            if version < next_version {
                Self::begin_immediate(&mut connection).await.db()?;
                let result = apply_schema(&mut connection, next_version, statements).await;
                #[cfg(all(test, unix))]
                if result.is_ok() {
                    super::migration_crash_tests::checkpoint(&format!(
                        "schema_v{next_version}_before_commit"
                    ))
                    .await;
                }
                Self::finish_write(&mut connection, result).await.db()?;
                #[cfg(all(test, unix))]
                super::migration_crash_tests::checkpoint(&format!(
                    "schema_v{next_version}_after_commit"
                ))
                .await;
            }
        }
        Ok(())
    }
}

async fn apply_schema(
    connection: &mut SqliteConnection,
    version: i64,
    statements: &[&str],
) -> Result<(), ClientStorageError> {
    for (statement_index, statement) in statements.iter().enumerate() {
        if let Err(error) = sqlx::query(sqlx::AssertSqlSafe(*statement))
            .execute(&mut *connection)
            .await
        {
            return Err(ClientStorageError::database(format!(
                "schema migration v{version} statement {} failed: {error}; SQL: {statement}",
                statement_index + 1
            )));
        }
    }
    if version == 35 {
        super::receipt_store::backfill_tombstones(connection).await?;
    }
    if version == 36 {
        super::receipt_cache::backfill_sizes(connection).await?;
    }
    if version == 37 {
        super::receipt_result::backfill_results(connection).await?;
        for statement in FINISH_SCHEMA_V37 {
            sqlx::query(sqlx::AssertSqlSafe(*statement))
                .execute(&mut *connection)
                .await
                .db()?;
        }
    }
    sqlx::query(
        "INSERT INTO client_schema_migrations(version, applied_at_unix_ms) \
         VALUES(?, CAST(strftime('%s','now') AS INTEGER) * 1000)",
    )
    .bind(version)
    .execute(&mut *connection)
    .await
    .db()?;
    Ok(())
}
