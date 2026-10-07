// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteResultExt};
use serde_json::Value;
use sqlx::{sqlite::SqliteRow, Row};

pub(super) fn decode_result_summary(row: &SqliteRow) -> Result<Option<String>, ClientStorageError> {
    let outcome: Option<String> = row.try_get("latest_terminal_outcome_json").db()?;
    let Some(outcome) = outcome else {
        return Ok(None);
    };
    let outcome: Value = serde_json::from_str(&outcome)?;
    Ok(terminal_outcome_text(&outcome))
}

fn terminal_outcome_text(outcome: &Value) -> Option<String> {
    if let Some(text) = outcome.as_str().and_then(non_empty) {
        return Some(text.to_string());
    }
    let object = outcome.as_object()?;
    for key in [
        "result_summary",
        "content",
        "answer",
        "text",
        "output",
        "error",
        "message",
        "reason",
    ] {
        if let Some(text) = object.get(key).and_then(Value::as_str).and_then(non_empty) {
            return Some(text.to_string());
        }
    }
    object.get("report").and_then(|report| {
        report
            .as_str()
            .or_else(|| report.get("content").and_then(Value::as_str))
            .and_then(non_empty)
            .map(ToOwned::to_owned)
    })
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}
