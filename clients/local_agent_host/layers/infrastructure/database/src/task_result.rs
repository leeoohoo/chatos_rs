// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{ClientStorageError, SqliteResultExt};
use serde_json::Value;
use sqlx::{sqlite::SqliteRow, Row};

pub(super) fn decode_result_gate(
    row: &SqliteRow,
) -> Result<(Option<String>, Option<String>), ClientStorageError> {
    let stored: Option<String> = row.try_get("latest_result_report_json").db()?;
    let Some(stored) = stored else {
        return Ok((None, None));
    };
    let stored: Value = serde_json::from_str(&stored)?;
    let parse = |key| {
        stored
            .get(key)
            .and_then(Value::as_str)
            .map(serde_json::from_str::<Value>)
            .transpose()
    };
    result_gate_from_reports(parse("result_json")?, parse("arguments_json")?)
}

fn result_gate_from_reports(
    result: Option<Value>,
    arguments: Option<Value>,
) -> Result<(Option<String>, Option<String>), ClientStorageError> {
    let report = result
        .filter(|value| value.get("reported").and_then(Value::as_bool) == Some(true))
        .or(arguments);
    let status = report
        .as_ref()
        .and_then(|value| value.get("result_status").or_else(|| value.get("status")))
        .and_then(Value::as_str)
        .filter(|status| matches!(*status, "succeeded" | "failed" | "blocked"))
        .map(ToOwned::to_owned);
    let reason = report
        .as_ref()
        .and_then(|value| value.get("reason"))
        .and_then(Value::as_str)
        .and_then(non_empty)
        .map(ToOwned::to_owned);
    Ok((status, reason))
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_result_gate_prefers_the_accepted_tool_result() {
        assert_eq!(
            result_gate_from_reports(
                Some(serde_json::json!({
                    "reported": true,
                    "result_status": "blocked",
                    "reason": "External approval is required."
                })),
                Some(serde_json::json!({
                    "result_status": "succeeded",
                    "reason": "Untrusted model argument"
                }))
            )
            .unwrap(),
            (
                Some("blocked".to_string()),
                Some("External approval is required.".to_string())
            )
        );
    }
}
