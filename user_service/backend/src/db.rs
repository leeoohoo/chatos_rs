// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;

pub async fn connect_database(config: &AppConfig) -> Result<chatos_postgres::PgPool, String> {
    let postgres_config = chatos_postgres::PostgresConfig::from_env(
        config.database_url.clone(),
        "user-service",
        "USER_SERVICE",
    )
    .map_err(|err| err.to_string())?;
    chatos_postgres::connect(&postgres_config)
        .await
        .map_err(|err| format!("connect user service PostgreSQL failed: {err}"))
}
