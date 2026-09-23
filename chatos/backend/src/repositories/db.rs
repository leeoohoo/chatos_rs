// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::{future::Future, pin::Pin, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use sqlx::types::Json;

use crate::db::{self, Database};

pub async fn get_db() -> Result<Arc<Database>, String> {
    db::get_db().await
}

pub async fn with_db<'env, T: 'env, F>(f: F) -> Result<T, String>
where
    F: FnOnce(
        &'static chatos_postgres::PgPool,
    ) -> Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'env>>,
{
    let _ = get_db().await?;
    f(db::get_pool()?).await
}

pub fn timestamp(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| format!("invalid RFC3339 timestamp {value:?}: {error}"))
}

pub fn optional_timestamp(value: Option<&str>) -> Result<Option<DateTime<Utc>>, String> {
    value.map(timestamp).transpose()
}

pub fn json<T: Serialize>(value: &T) -> Result<Json<Value>, String> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| error.to_string())
}

pub fn decode_optional<T: DeserializeOwned>(
    value: Option<Json<Value>>,
) -> Result<Option<T>, String> {
    value
        .map(|Json(value)| serde_json::from_value(value).map_err(|error| error.to_string()))
        .transpose()
}

pub fn decode_all<T: DeserializeOwned>(values: Vec<Json<Value>>) -> Result<Vec<T>, String> {
    values
        .into_iter()
        .map(|Json(value)| serde_json::from_value(value).map_err(|error| error.to_string()))
        .collect()
}

pub fn db_error(error: sqlx::Error) -> String {
    error.to_string()
}
