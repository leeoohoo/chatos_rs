// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::local_runtime::storage::LocalRuntimeEventRecord;
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;

#[derive(Debug, Default, Deserialize)]
pub(super) struct LocalRuntimeEventQuery {
    turn_id: Option<String>,
    after: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(super) struct LocalRuntimeEventResponse {
    event_seq: i64,
    event_id: String,
    project_id: Option<String>,
    session_id: Option<String>,
    turn_id: Option<String>,
    event_name: String,
    stream_type: Option<String>,
    payload: Value,
    created_at: String,
}

pub(super) async fn list_events(
    Path(session_id): Path<String>,
    Query(query): Query<LocalRuntimeEventQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Vec<LocalRuntimeEventResponse>>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let session_id = required(session_id, "session_id")?;
    let turn_id = query
        .turn_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let records = runtime
        .local_database()?
        .list_runtime_events(
            owner.owner_user_id.as_str(),
            session_id.as_str(),
            turn_id.as_deref(),
            query.after.unwrap_or_default(),
            query.limit.unwrap_or(100),
        )
        .await?;
    Ok(Json(records.into_iter().map(event_response).collect()))
}

fn event_response(record: LocalRuntimeEventRecord) -> LocalRuntimeEventResponse {
    LocalRuntimeEventResponse {
        event_seq: record.event_seq,
        event_id: record.event_id,
        project_id: record.project_id,
        session_id: record.session_id,
        turn_id: record.turn_id,
        event_name: record.event_name,
        stream_type: record.stream_type,
        payload: serde_json::from_str(record.payload_json.as_str()).unwrap_or(Value::Null),
        created_at: record.created_at,
    }
}

fn required(value: String, name: &str) -> Result<String, LocalRuntimeApiError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(LocalRuntimeApiError::bad_request(
            "local_runtime_invalid_request",
            format!("{name} is required"),
        ));
    }
    Ok(value)
}
