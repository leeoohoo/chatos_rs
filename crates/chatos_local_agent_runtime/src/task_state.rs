// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_client_storage::{
    AgentRunStateRecord, PutRecord, RecordQuery, StorageError, StorageResult,
    TransactionRepositories,
};
use chatos_local_agent_protocol::LocalAgentRunStatus;
use serde_json::{json, Value};

/// Mirrors the authoritative Run into its local Task record inside the same
/// database transaction. Run remains the execution state machine; Task is the
/// user-facing aggregate and must never lag behind a committed Run transition.
pub(crate) async fn sync_task_from_run(
    repositories: &mut dyn TransactionRepositories,
    run: &AgentRunStateRecord,
) -> StorageResult<()> {
    if run.run.owner_entity_type != "task" {
        return Ok(());
    }
    let query = RecordQuery {
        scope: run.metadata.scope.clone(),
        id: run.run.owner_entity_id.clone(),
    };
    let mut task =
        repositories
            .tasks()
            .get(&query)
            .await?
            .ok_or_else(|| StorageError::InvalidData {
                reason: format!(
                    "Task Runner run {} has no local Task record {}",
                    run.run.run_id, run.run.owner_entity_id
                ),
            })?;
    if task.state.get("run_id").and_then(Value::as_str) != Some(run.run.run_id.as_str())
        || task.state.get("project_id").and_then(Value::as_str) != run.run.project_id.as_deref()
    {
        return Err(StorageError::InvalidData {
            reason: "Task record does not match its authoritative Run identity".to_string(),
        });
    }
    let status = task_status(run.run.status);
    let state = task
        .state
        .as_object_mut()
        .ok_or_else(|| StorageError::InvalidData {
            reason: "Task state must be a JSON object".to_string(),
        })?;
    let updates = [
        ("run_status", json!(run.run.status)),
        ("run_version", json!(run.run.version)),
        ("step_seq", json!(run.run.step_seq)),
        ("iteration", json!(run.run.iteration)),
        ("retry_count", json!(run.run.retry_count)),
        ("pending_interaction", json!(run.run.pending_interaction)),
        ("terminal_outcome", json!(run.run.terminal_outcome)),
        ("run_updated_at", json!(run.run.updated_at)),
    ];
    let unchanged = task.status == status
        && updates
            .iter()
            .all(|(key, value)| state.get(*key) == Some(value));
    if unchanged {
        return Ok(());
    }
    for (key, value) in updates {
        state.insert(key.to_string(), value);
    }
    let revision = task.metadata.revision;
    task.status = status.to_string();
    task.metadata.updated_at = run.run.updated_at;
    repositories
        .tasks()
        .put(PutRecord {
            record: task,
            expected_revision: Some(revision),
        })
        .await?;
    Ok(())
}

fn task_status(status: LocalAgentRunStatus) -> &'static str {
    match status {
        LocalAgentRunStatus::Queued => "queued",
        LocalAgentRunStatus::ModelReady
        | LocalAgentRunStatus::ModelRunning
        | LocalAgentRunStatus::WaitingToolResult
        | LocalAgentRunStatus::ContinuationReady
        | LocalAgentRunStatus::RetryScheduled => "running",
        LocalAgentRunStatus::Paused => "paused",
        LocalAgentRunStatus::NeedsReview => "needs_review",
        LocalAgentRunStatus::Succeeded => "succeeded",
        LocalAgentRunStatus::Failed => "failed",
        LocalAgentRunStatus::Cancelled => "cancelled",
    }
}
