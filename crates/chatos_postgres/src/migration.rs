// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::migrate::Migrator;

use crate::{PgPool, PostgresError};

pub async fn run_migrations(
    pool: &PgPool,
    migrator: &'static Migrator,
) -> Result<(), PostgresError> {
    migrator.run(pool).await?;
    Ok(())
}

pub async fn ensure_migrations_applied(
    pool: &PgPool,
    migrator: &'static Migrator,
) -> Result<(), PostgresError> {
    let applied = sqlx::query_scalar::<_, i64>(
        "SELECT version FROM _sqlx_migrations WHERE success = TRUE ORDER BY version",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| {
        PostgresError::Schema(format!(
            "cannot read _sqlx_migrations; run the migration job first: {error}"
        ))
    })?;
    let expected = migrator
        .iter()
        .filter(|migration| migration.migration_type.is_up_migration())
        .map(|migration| migration.version)
        .collect::<Vec<_>>();
    if applied != expected {
        return Err(PostgresError::Schema(format!(
            "applied versions {applied:?} do not match expected versions {expected:?}"
        )));
    }
    Ok(())
}
