// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::Serialize;
use sqlx::types::Json;

pub(crate) fn timestamp(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| format!("invalid RFC3339 timestamp {value:?}: {error}"))
}

pub(crate) fn optional_timestamp(value: Option<&str>) -> Result<Option<DateTime<Utc>>, String> {
    value.map(timestamp).transpose()
}

pub(crate) fn json<T: Serialize>(value: &T) -> Result<Json<serde_json::Value>, String> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| error.to_string())
}

pub(crate) fn decode<T: DeserializeOwned>(value: Json<serde_json::Value>) -> Result<T, String> {
    serde_json::from_value(value.0).map_err(|error| error.to_string())
}
