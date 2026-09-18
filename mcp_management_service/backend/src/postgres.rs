// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::migrate::Migrator;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

pub fn required_timestamp(
    unix_seconds: i64,
    context: &str,
) -> Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::DateTime::from_timestamp(unix_seconds, 0)
        .ok_or_else(|| format!("{context} is outside the supported timestamp range"))
}

pub async fn connect(database_url: &str) -> Result<chatos_postgres::PgPool, String> {
    let config = chatos_postgres::PostgresConfig::from_env(
        database_url.to_string(),
        "mcp-management",
        "MCP_MANAGEMENT",
    )
    .map_err(|error| error.to_string())?;
    let pool = chatos_postgres::connect(&config)
        .await
        .map_err(|error| error.to_string())?;
    chatos_postgres::ensure_migrations_applied(&pool, &MIGRATOR)
        .await
        .map_err(|error| error.to_string())?;
    Ok(pool)
}
