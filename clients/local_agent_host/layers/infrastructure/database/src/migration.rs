// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    schema::{SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5},
    ClientStorageError, IntoClientStorageResult, SqliteClientStorage,
};
use sqlx::SqliteConnection;

const SCHEMA_VERSION: i64 = 5;

impl SqliteClientStorage {
    pub(super) async fn migrate(&self) -> Result<(), ClientStorageError> {
        let mut connection = self.pool.acquire().await.into_storage()?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS client_schema_migrations (\
             version INTEGER PRIMARY KEY NOT NULL, applied_at_unix_ms INTEGER NOT NULL)",
        )
        .execute(&mut *connection)
        .await
        .into_storage()?;
        let version = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(version) FROM client_schema_migrations",
        )
        .fetch_one(&mut *connection)
        .await
        .into_storage()?
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
        ] {
            if version < next_version {
                Self::begin_immediate(&mut connection)
                    .await
                    .into_storage()?;
                let result = apply_schema(&mut connection, next_version, statements).await;
                Self::finish_write(&mut connection, result)
                    .await
                    .into_storage()?;
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
    for statement in statements {
        sqlx::query(statement)
            .execute(&mut *connection)
            .await
            .into_storage()?;
    }
    sqlx::query(
        "INSERT INTO client_schema_migrations(version, applied_at_unix_ms) \
         VALUES(?, CAST(strftime('%s','now') AS INTEGER) * 1000)",
    )
    .bind(version)
    .execute(&mut *connection)
    .await
    .into_storage()?;
    Ok(())
}
