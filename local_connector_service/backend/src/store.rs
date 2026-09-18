// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use sqlx::migrate::Migrator;
use sqlx::types::Json;

mod postgres;

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

#[derive(Debug)]
pub enum SessionAcquireError {
    AlreadyActive,
    Store(String),
}

#[derive(Clone)]
pub struct ConnectorStore {
    pool: chatos_postgres::PgPool,
}

impl ConnectorStore {
    pub async fn connect(database_url: &str) -> Result<Self, String> {
        let config = chatos_postgres::PostgresConfig::from_env(
            database_url.to_string(),
            "local-connector",
            "LOCAL_CONNECTOR",
        )
        .map_err(|err| err.to_string())?;
        let pool = chatos_postgres::connect(&config)
            .await
            .map_err(|err| format!("connect Local Connector PostgreSQL failed: {err}"))?;
        chatos_postgres::ensure_migrations_applied(&pool, &MIGRATOR)
            .await
            .map_err(|err| err.to_string())?;
        Ok(Self { pool })
    }

    pub(crate) fn pool(&self) -> &chatos_postgres::PgPool {
        &self.pool
    }
}

fn timestamp(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|err| format!("invalid RFC3339 timestamp {value:?}: {err}"))
}

fn json<T: Serialize>(value: &T) -> Result<Json<Value>, String> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|err| err.to_string())
}

fn decode_optional<T: DeserializeOwned>(value: Option<Json<Value>>) -> Result<Option<T>, String> {
    value
        .map(|Json(value)| serde_json::from_value(value).map_err(|err| err.to_string()))
        .transpose()
}

fn decode_all<T: DeserializeOwned>(values: Vec<Json<Value>>) -> Result<Vec<T>, String> {
    values
        .into_iter()
        .map(|Json(value)| serde_json::from_value(value).map_err(|err| err.to_string()))
        .collect()
}

fn db_error(error: sqlx::Error) -> String {
    error.to_string()
}
