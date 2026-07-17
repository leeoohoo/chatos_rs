// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use serde_json::Value;

use crate::local_runtime::storage::LocalMessageRecord;
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;

#[derive(Debug, Serialize)]
pub(super) struct LocalMessageResponse {
    id: String,
    conversation_id: String,
    turn_id: Option<String>,
    sequence_no: i64,
    role: String,
    content: String,
    message_mode: Option<String>,
    message_source: Option<String>,
    reasoning: Option<String>,
    tool_calls: Option<Value>,
    tool_call_id: Option<String>,
    metadata: Option<Value>,
    created_at: String,
}

pub(super) async fn list_messages(
    Path(session_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Vec<LocalMessageResponse>>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_invalid_request",
            "session_id is required",
        ));
    }
    let records = runtime
        .local_database()?
        .list_messages(owner.owner_user_id.as_str(), session_id)
        .await?;
    Ok(Json(records.into_iter().map(message_response).collect()))
}

pub(super) fn message_response(record: LocalMessageRecord) -> LocalMessageResponse {
    let metadata = message_metadata(record.metadata_json, record.sequence_no);
    let message_mode = metadata_text(&metadata, "message_mode");
    let message_source = metadata_text(&metadata, "message_source");
    LocalMessageResponse {
        id: record.id,
        conversation_id: record.session_id,
        turn_id: record.turn_id,
        sequence_no: record.sequence_no,
        role: record.role,
        content: record.content,
        message_mode,
        message_source,
        reasoning: record.reasoning,
        tool_calls: parse_optional_json(record.tool_calls_json),
        tool_call_id: record.tool_call_id,
        metadata: Some(metadata),
        created_at: record.created_at,
    }
}

fn metadata_text(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn message_metadata(raw: Option<String>, sequence_no: i64) -> Value {
    let mut metadata = match parse_optional_json(raw) {
        Some(Value::Object(values)) => values,
        Some(value) => serde_json::Map::from_iter([("metadata".to_string(), value)]),
        None => serde_json::Map::new(),
    };
    metadata.insert(
        "local_sequence_no".to_string(),
        Value::Number(sequence_no.into()),
    );
    Value::Object(metadata)
}

fn parse_optional_json(raw: Option<String>) -> Option<Value> {
    raw.and_then(|raw| serde_json::from_str(raw.as_str()).ok())
}
