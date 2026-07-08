// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Query, State};
use axum::Json;
use serde_json::{json, Value};

use crate::{LocalRuntime, MAX_COMMAND_HISTORY_ENTRIES};

use super::super::types::{CommandHistoryQuery, LocalApiError};

const DEFAULT_COMMAND_HISTORY_LIMIT: usize = 200;

pub(crate) async fn local_command_history(
    State(runtime): State<LocalRuntime>,
    Query(query): Query<CommandHistoryQuery>,
) -> Result<Json<Value>, LocalApiError> {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_COMMAND_HISTORY_LIMIT)
        .clamp(1, MAX_COMMAND_HISTORY_ENTRIES);
    let source = query
        .source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let state = runtime.state.read().await;
    let entries = state
        .command_history
        .iter()
        .rev()
        .filter(|entry| source.map(|source| entry.source == source).unwrap_or(true))
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    Ok(Json(json!({ "entries": entries })))
}

pub(crate) async fn local_clear_command_history(
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalApiError> {
    let mut state = runtime.state.write().await;
    state.command_history.clear();
    state.save(runtime.state_path.as_path())?;
    Ok(Json(json!({ "entries": [] })))
}
