use futures_util::TryStreamExt;
use mongodb::{
    bson::{doc, Bson, Document},
    Cursor,
};
use serde_json::Value;

use crate::db::Db;
use crate::models::EngineRecord;

pub(crate) fn record_collection(db: &Db) -> mongodb::Collection<EngineRecord> {
    db.collection::<EngineRecord>("engine_records")
}

pub(crate) async fn collect_records(
    cursor: Cursor<EngineRecord>,
) -> Result<Vec<EngineRecord>, String> {
    cursor.try_collect().await.map_err(|err| err.to_string())
}

pub(crate) fn build_record_filter(
    thread_id: &str,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    role: Option<&str>,
    record_type: Option<&str>,
    summary_status: Option<&str>,
) -> Document {
    let mut filter = doc! {
        "thread_id": thread_id,
    };
    if let Some(value) = tenant_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("tenant_id", value);
    }
    if let Some(value) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("source_id", value);
    }
    if let Some(value) = role.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("role", value);
    }
    if let Some(value) = record_type.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("record_type", value);
    }
    if let Some(value) = summary_status
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if value == "pending" {
            filter.insert(
                "$or",
                vec![
                    doc! {"summary_status": "pending"},
                    doc! {"summary_status": {"$exists": false}},
                    doc! {"summary_status": Bson::Null},
                    doc! {"summary_status": ""},
                ],
            );
        } else {
            filter.insert("summary_status", value);
        }
    }
    filter
}

pub(crate) fn summary_status_is_pending(value: Option<&str>) -> bool {
    match value.map(str::trim) {
        None => true,
        Some("") => true,
        Some("pending") => true,
        _ => false,
    }
}

pub(crate) fn estimate_record_summary_tokens(
    created_at: &str,
    role: &str,
    content: &str,
    structured_payload: Option<&Value>,
    metadata: Option<&Value>,
) -> i64 {
    let mut parts = vec![format!("[{}][{}]", created_at, role)];

    if !content.trim().is_empty() {
        parts.push(content.to_string());
    }

    if let Some(metadata) = metadata {
        if let Some(reasoning) = metadata
            .get("reasoning")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            parts.push(format!("[reasoning]\n{}", reasoning));
        }

        if let Some(tool_calls) = metadata
            .get("tool_calls")
            .or_else(|| metadata.get("toolCalls"))
            .filter(|value| !value.is_null())
        {
            parts.push(format!("[tool_calls]\n{}", tool_calls));
        }

        if let Some(structured_result) = metadata
            .get("structured_result")
            .filter(|value| !value.is_null())
        {
            parts.push(format!("[tool_result]\n{}", structured_result));
        }
    }

    if let Some(payload) = structured_payload.filter(|value| !value.is_null()) {
        parts.push(format!("[structured_payload]\n{}", payload));
    }

    estimate_tokens_text(parts.join("\n").as_str())
}

pub(crate) fn estimate_pending_record_tokens(record: &EngineRecord) -> i64 {
    estimate_record_summary_tokens(
        record.created_at.as_str(),
        record.role.as_str(),
        record.content.as_str(),
        record.structured_payload.as_ref(),
        record.metadata.as_ref(),
    )
}

fn estimate_tokens_text(text: &str) -> i64 {
    let char_count = text.chars().count() as i64;
    let byte_count = text.len() as i64;
    let extra_bytes = byte_count.saturating_sub(char_count);
    ((char_count * 3 + extra_bytes * 4 + 11) / 12).max(1)
}
