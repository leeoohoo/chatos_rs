// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde_json::{json, Value};

use crate::LocalRuntime;

use super::response::task_response;
use super::{load_tasks, LocalRuntimeApiError, TaskBoardQuery};

pub(super) async fn get_graph(
    Path(session_id): Path<String>,
    Query(query): Query<TaskBoardQuery>,
    State(runtime): State<LocalRuntime>,
) -> Result<Json<Value>, LocalRuntimeApiError> {
    let (records, source_turn_id, source_user_message_id) =
        load_tasks(&runtime, session_id.as_str(), &query).await?;
    let ids = records
        .iter()
        .map(|record| record.id.as_str())
        .collect::<BTreeSet<_>>();
    let nodes = records
        .iter()
        .map(|record| {
            json!({
                "task": task_response(record),
                "depth": usize::from(!record.prerequisite_task_ids.is_empty()),
                "is_root": record.prerequisite_task_ids.is_empty(),
                "is_current_message": query.turn_id.as_deref().is_none_or(|turn| record.source_turn_id == turn),
            })
        })
        .collect::<Vec<_>>();
    let edges = records
        .iter()
        .flat_map(|record| {
            record
                .prerequisite_task_ids
                .iter()
                .filter(|source| ids.contains(source.as_str()))
                .map(|source| {
                    json!({
                        "id": format!("{source}->{}", record.id),
                        "source": source,
                        "target": record.id,
                        "kind": "prerequisite",
                    })
                })
        })
        .collect::<Vec<_>>();
    let root_task_ids = records
        .iter()
        .filter(|record| record.prerequisite_task_ids.is_empty())
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "root_task_ids": root_task_ids,
        "nodes": nodes,
        "edges": edges,
        "source_session_id": session_id,
        "source_turn_id": source_turn_id,
        "source_user_message_id": source_user_message_id,
    })))
}
