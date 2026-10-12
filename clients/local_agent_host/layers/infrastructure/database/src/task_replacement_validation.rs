// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{task_store::SCOPED_TASK_SELECT, ClientStorageError, SqliteResultExt};
use chatos_local_agent_protocol::CreateTaskGraphCommand;
use sqlx::{Row, SqliteConnection};
use std::collections::HashSet;

/// Recheck stop-before-replace under the graph creation write transaction.
/// The earlier application check alone can race a user retry of the old task.
pub(super) async fn validate_cancelled_sources(
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
            if status != "cancelled" {
                return Err(ClientStorageError::Conflict(format!(
                    "superseded task is no longer cancelled: {id}"
                )));
            }
        }
    }
    Ok(())
}
