// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use chatos_mcp::TaskUpdatePatch;

use crate::local_now_rfc3339;
use crate::local_runtime::task_board::{
    normalize_task_patch, validate_terminal_state, LocalTaskBoardTaskRecord,
};

use super::super::LocalDatabase;
use super::queries::select_task;
use super::validation::require_local_task_scope;

impl LocalDatabase {
    pub(crate) async fn update_local_task_board_task(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_id: &str,
        patch: TaskUpdatePatch,
    ) -> Result<LocalTaskBoardTaskRecord> {
        require_local_task_scope(self, owner_user_id, session_id, None).await?;
        let current = select_task(self, owner_user_id, session_id, task_id)
            .await?
            .context("local task board task was not found")?;
        let patch = normalize_task_patch(patch).map_err(anyhow::Error::msg)?;
        let next = merge_task(current, patch);
        validate_terminal_state(
            next.status.as_str(),
            next.outcome_summary.as_str(),
            next.outcome_items.as_slice(),
            next.blocker_reason.as_str(),
        )
        .map_err(anyhow::Error::msg)?;
        persist_task(self, owner_user_id, next).await
    }

    pub(crate) async fn complete_local_task_board_task(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_id: &str,
        mut patch: TaskUpdatePatch,
    ) -> Result<LocalTaskBoardTaskRecord> {
        patch.status = Some("done".to_string());
        self.update_local_task_board_task(owner_user_id, session_id, task_id, patch)
            .await
    }

    pub(crate) async fn delete_local_task_board_task(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_id: &str,
    ) -> Result<bool> {
        require_local_task_scope(self, owner_user_id, session_id, None).await?;
        let result = sqlx::query(
            "DELETE FROM task_board_tasks WHERE id = ? AND session_id = ? AND owner_user_id = ?",
        )
        .bind(task_id)
        .bind(session_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("delete local task board task")?;
        Ok(result.rows_affected() > 0)
    }
}

fn merge_task(
    mut task: LocalTaskBoardTaskRecord,
    patch: TaskUpdatePatch,
) -> LocalTaskBoardTaskRecord {
    if let Some(value) = patch.title {
        task.title = value;
    }
    if let Some(value) = patch.details {
        task.details = value;
    }
    if let Some(value) = patch.priority {
        task.priority = value;
    }
    if let Some(value) = patch.status {
        task.status = value;
    }
    if let Some(value) = patch.tags {
        task.tags = value;
    }
    if let Some(value) = patch.due_at {
        task.due_at = value;
    }
    if let Some(value) = patch.outcome_summary {
        task.outcome_summary = value;
    }
    if let Some(value) = patch.outcome_items {
        task.outcome_items = value;
    }
    if let Some(value) = patch.resume_hint {
        task.resume_hint = value;
    }
    if let Some(value) = patch.blocker_reason {
        task.blocker_reason = value;
    }
    if let Some(value) = patch.blocker_needs {
        task.blocker_needs = value;
    }
    if let Some(value) = patch.blocker_kind {
        task.blocker_kind = value;
    }
    if let Some(value) = patch.completed_at {
        task.completed_at = value;
    }
    if let Some(value) = patch.last_outcome_at {
        task.last_outcome_at = value;
    }
    apply_terminal_defaults(&mut task);
    task.updated_at = local_now_rfc3339();
    task
}

fn apply_terminal_defaults(task: &mut LocalTaskBoardTaskRecord) {
    if task.status == "done" && task.completed_at.is_none() {
        task.completed_at = Some(local_now_rfc3339());
    } else if task.status == "blocked" {
        task.completed_at = None;
    }
    if matches!(task.status.as_str(), "done" | "blocked")
        && task.last_outcome_at.is_none()
        && (!task.outcome_summary.is_empty() || !task.outcome_items.is_empty())
    {
        task.last_outcome_at = Some(local_now_rfc3339());
    }
}

async fn persist_task(
    database: &LocalDatabase,
    owner_user_id: &str,
    task: LocalTaskBoardTaskRecord,
) -> Result<LocalTaskBoardTaskRecord> {
    sqlx::query(
        r#"
        UPDATE task_board_tasks SET
            title = ?, details = ?, priority = ?, status = ?, tags_json = ?,
            due_at = ?, outcome_summary = ?, outcome_items_json = ?, resume_hint = ?,
            blocker_reason = ?, blocker_needs_json = ?, blocker_kind = ?,
            completed_at = ?, last_outcome_at = ?, updated_at = ?
        WHERE id = ? AND session_id = ? AND owner_user_id = ?
        "#,
    )
    .bind(task.title.as_str())
    .bind(task.details.as_str())
    .bind(task.priority.as_str())
    .bind(task.status.as_str())
    .bind(serde_json::to_string(&task.tags)?)
    .bind(task.due_at.as_deref())
    .bind(task.outcome_summary.as_str())
    .bind(serde_json::to_string(&task.outcome_items)?)
    .bind(task.resume_hint.as_str())
    .bind(task.blocker_reason.as_str())
    .bind(serde_json::to_string(&task.blocker_needs)?)
    .bind(task.blocker_kind.as_str())
    .bind(task.completed_at.as_deref())
    .bind(task.last_outcome_at.as_deref())
    .bind(task.updated_at.as_str())
    .bind(task.id.as_str())
    .bind(task.conversation_id.as_str())
    .bind(owner_user_id)
    .execute(database.pool())
    .await
    .context("update local task board task")?;
    Ok(task)
}
