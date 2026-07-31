// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod graph;
mod mutations;
pub(in crate::local_runtime::api) mod response;

use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::local_runtime::task_board::LocalTaskBoardTaskRecord;
use crate::LocalRuntime;

use super::context::owner_context;
use super::error::LocalRuntimeApiError;
use response::task_response;

#[derive(Debug, Default, Deserialize)]
pub(super) struct TaskBoardQuery {
    turn_id: Option<String>,
    source_user_message_id: Option<String>,
    include_done: Option<bool>,
    limit: Option<usize>,
}

pub(super) fn router() -> Router<LocalRuntime> {
    Router::new()
        .route(
            "/api/local/runtime/sessions/{session_id}/task-board/tasks",
            get(list_tasks),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/task-board/graph",
            get(graph::get_graph),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/task-board/tasks/{task_id}",
            get(get_task)
                .patch(mutations::update_task)
                .delete(mutations::delete_task),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/task-board/tasks/{task_id}/complete",
            axum::routing::post(mutations::complete_task),
        )
        .route(
            "/api/local/runtime/sessions/{session_id}/task-board/active-message-tasks",
            get(active_message_tasks),
        )
}

async fn list_tasks(
    Path(session_id): Path<String>,
    Query(query): Query<TaskBoardQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let (records, source_turn_id, source_user_message_id) =
        load_tasks(&runtime, session_id.as_str(), &query).await?;
    Ok(Json(json!({
        "items": records.iter().map(task_response).collect::<Vec<_>>(),
        "source_session_id": session_id,
        "source_turn_id": source_turn_id,
        "source_user_message_id": source_user_message_id,
    })))
}

async fn get_task(
    Path((session_id, task_id)): Path<(String, String)>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let record = runtime
        .local_database()?
        .get_local_task_board_task(
            owner.owner_user_id.as_str(),
            session_id.as_str(),
            task_id.as_str(),
        )
        .await?
        .ok_or_else(|| {
            LocalRuntimeApiError::not_found("local_task_not_found", "Local task was not found")
        })?;
    Ok(Json(task_response(&record)))
}

async fn active_message_tasks(
    Path(session_id): Path<String>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let owner = owner_context(&runtime).await?;
    let records = runtime
        .local_database()?
        .list_local_task_board_tasks(
            owner.owner_user_id.as_str(),
            session_id.as_str(),
            None,
            false,
            200,
        )
        .await?;
    Ok(Json(active_message_task_summary(records)))
}

fn active_message_task_summary(records: Vec<LocalTaskBoardTaskRecord>) -> Value {
    let mut active_counts = BTreeMap::<(String, String), usize>::new();
    let mut running_counts = BTreeMap::<(String, String), usize>::new();
    for record in records {
        if record.task_kind != "task_runner" || !matches!(record.status.as_str(), "todo" | "doing")
        {
            continue;
        }
        let Some(message_id) = record.source_user_message_id else {
            continue;
        };
        let key = (message_id, record.source_turn_id);
        *active_counts.entry(key.clone()).or_default() += 1;
        if record.status != "doing" {
            continue;
        }
        *running_counts.entry(key).or_default() += 1;
    }
    let items = active_counts
        .iter()
        .map(|((message_id, turn_id), active_count)| {
            let running_count = running_counts
                .get(&(message_id.clone(), turn_id.clone()))
                .copied()
                .unwrap_or(0);
            json!({
                "source_user_message_id": message_id,
                "source_turn_id": turn_id,
                "running_count": running_count,
                "active_count": active_count,
            })
        })
        .collect::<Vec<_>>();
    let active_ids = active_counts
        .keys()
        .map(|(message_id, _)| message_id)
        .collect::<Vec<_>>();
    let running_ids = running_counts
        .keys()
        .map(|(message_id, _)| message_id)
        .collect::<Vec<_>>();
    json!({
        "active_source_user_message_ids": active_ids,
        "running_source_user_message_ids": running_ids,
        "items": items,
    })
}

pub(super) async fn load_tasks(
    runtime: &LocalRuntime,
    session_id: &str,
    query: &TaskBoardQuery,
) -> Result<
    (
        Vec<LocalTaskBoardTaskRecord>,
        Option<String>,
        Option<String>,
    ),
    LocalRuntimeApiError,
> {
    let owner = owner_context(runtime).await?;
    let turn_id = normalized(query.turn_id.as_deref());
    let mut records = runtime
        .local_database()?
        .list_local_task_board_tasks(
            owner.owner_user_id.as_str(),
            session_id,
            turn_id,
            query.include_done.unwrap_or(true),
            query.limit.unwrap_or(200),
        )
        .await?;
    let source_message_id = normalized(query.source_user_message_id.as_deref());
    if let Some(message_id) = source_message_id {
        records.retain(|record| record.source_user_message_id.as_deref() == Some(message_id));
    }
    let source_turn_id = turn_id
        .map(ToOwned::to_owned)
        .or_else(|| records.first().map(|record| record.source_turn_id.clone()));
    let source_user_message_id = source_message_id.map(ToOwned::to_owned).or_else(|| {
        records
            .first()
            .and_then(|record| record.source_user_message_id.clone())
    });
    Ok((records, source_turn_id, source_user_message_id))
}

fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests;
