// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{task_store::SCOPED_TASK_SELECT, ClientStorageError, SqliteResultExt};
use chatos_local_agent_protocol::CreateTaskGraphCommand;
use sqlx::{Row, SqliteConnection};
use std::collections::HashSet;

/// Recheck stop-before-replace under the graph creation write transaction.
/// The earlier application check alone can race a user retry of the old task.
pub(super) async fn validate_stopped_sources(
    connection: &mut SqliteConnection,
    graph: &CreateTaskGraphCommand,
) -> Result<(), ClientStorageError> {
    let query = format!("{SCOPED_TASK_SELECT} AND t.task_id = ?");
    for task in &graph.tasks {
        let Some(value) = task.input.get("supersedes_task_ids") else {
            continue;
        };
        let ids = value
            .as_array()
            .filter(|ids| ids.len() <= 50)
            .ok_or_else(|| {
                ClientStorageError::InvalidState("invalid supersedes_task_ids".into())
            })?;
        if ids.is_empty() {
            continue;
        }
        let conversation_id = task
            .input
            .get("source_conversation_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                ClientStorageError::InvalidState("replacement requires conversation context".into())
            })?;
        let mut seen = HashSet::new();
        for value in ids {
            let id = value
                .as_str()
                .filter(|id| !id.trim().is_empty() && seen.insert(*id))
                .ok_or_else(|| {
                    ClientStorageError::InvalidState("invalid superseded task id".into())
                })?;
            let row = sqlx::query(sqlx::AssertSqlSafe(query.clone()))
                .bind(conversation_id)
                .bind(&graph.owner_user_id)
                .bind(&graph.owner_user_id)
                .bind(&graph.owner_user_id)
                .bind(id)
                .fetch_optional(&mut *connection)
                .await
                .db()?
                .ok_or_else(|| ClientStorageError::NotFound(id.into()))?;
            let status: String = row.try_get("status").db()?;
            if !matches!(status.as_str(), "cancelled" | "failed" | "succeeded") {
                return Err(ClientStorageError::Conflict(format!(
                    "superseded task became active again: {id}"
                )));
            }
        }
    }
    Ok(())
}

/// Persists the reverse edge in the same transaction that creates the new
/// Task Graph. `supersedes_task_ids` remains the forward, immutable intent;
/// `replacement_task_ids_json` is the query index on each historical Task.
pub(super) async fn link_replacements(
    connection: &mut SqliteConnection,
    graph: &CreateTaskGraphCommand,
    now_unix_ms: i64,
) -> Result<(), ClientStorageError> {
    for replacement in &graph.tasks {
        let Some(superseded) = replacement
            .input
            .get("supersedes_task_ids")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        for old_task_id in superseded.iter().filter_map(serde_json::Value::as_str) {
            let row = sqlx::query(
                "SELECT t.replacement_task_ids_json FROM local_tasks t \
                 JOIN local_task_graphs g ON g.graph_id = t.graph_id \
                 WHERE t.task_id = ? AND g.owner_user_id = ?",
            )
            .bind(old_task_id)
            .bind(&graph.owner_user_id)
            .fetch_optional(&mut *connection)
            .await
            .db()?
            .ok_or_else(|| ClientStorageError::NotFound(old_task_id.to_string()))?;
            let mut ids = serde_json::from_str::<Vec<String>>(
                &row.try_get::<String, _>("replacement_task_ids_json").db()?,
            )?;
            if !ids.iter().any(|id| id == &replacement.task_id) {
                ids.push(replacement.task_id.clone());
                ids.sort();
                sqlx::query(
                    "UPDATE local_tasks SET replacement_task_ids_json = ?, \
                     version = version + 1, updated_at_unix_ms = ? WHERE task_id = ?",
                )
                .bind(serde_json::to_string(&ids)?)
                .bind(now_unix_ms)
                .bind(old_task_id)
                .execute(&mut *connection)
                .await
                .db()?;
            }
        }
    }
    Ok(())
}
