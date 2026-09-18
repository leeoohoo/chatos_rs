// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sqlx::types::Json;

use crate::models::EngineThread;
use crate::repositories::postgres::decode;

pub(crate) fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub(crate) fn decode_threads(
    rows: Vec<Json<serde_json::Value>>,
) -> Result<Vec<EngineThread>, String> {
    rows.into_iter().map(decode).collect()
}
