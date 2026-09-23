// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::{PostgresConfig, PostgresError};

pub async fn connect(config: &PostgresConfig) -> Result<sqlx::PgPool, PostgresError> {
    config.validate()?;
    let options = PgConnectOptions::from_str(config.database_url.as_str())?
        .application_name(config.application_name.as_str())
        .options([
            ("timezone", "UTC".to_string()),
            (
                "statement_timeout",
                duration_millis(config.statement_timeout),
            ),
            ("lock_timeout", duration_millis(config.lock_timeout)),
            (
                "idle_in_transaction_session_timeout",
                duration_millis(config.idle_in_transaction_session_timeout),
            ),
        ]);
    PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.acquire_timeout)
        .idle_timeout(config.idle_timeout)
        .max_lifetime(config.max_lifetime)
        .connect_with(options)
        .await
        .map_err(PostgresError::from)
}

fn duration_millis(value: std::time::Duration) -> String {
    format!("{}ms", value.as_millis())
}
