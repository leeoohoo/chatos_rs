// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use chatos_mcp::TaskDraft;
use uuid::Uuid;

use crate::local_now_rfc3339;
use crate::local_runtime::task_board::{normalize_task_draft, LocalTaskBoardTaskRecord};

use super::super::LocalDatabase;
use super::queries::select_task;
use super::validation::{require_local_task_scope, validate_prerequisites};

impl LocalDatabase {
    pub(crate) async fn create_local_task_board_tasks(
        &self,
        owner_user_id: &str,
        session_id: &str,
        turn_id: &str,
        drafts: Vec<TaskDraft>,
    ) -> Result<Vec<LocalTaskBoardTaskRecord>> {
        require_local_task_scope(self, owner_user_id, session_id, Some(turn_id)).await?;
        let drafts = drafts
            .into_iter()
            .map(normalize_task_draft)
            .collect::<Result<Vec<_>, _>>()
            .map_err(anyhow::Error::msg)?;
        let now = local_now_rfc3339();
        let mut ids = Vec::with_capacity(drafts.len());
        let mut transaction = self
            .begin_write()
            .await
            .context("begin local task creation")?;
        for draft in drafts {
            let task_id = format!("lc_task_{}", Uuid::new_v4());
            validate_prerequisites(
                self,
                owner_user_id,
                session_id,
                task_id.as_str(),
                draft.prerequisite_task_ids.as_slice(),
            )
            .await?;
            sqlx::query(
                r#"
                INSERT INTO task_board_tasks (
                    id, session_id, turn_id, owner_user_id, title, details,
                    priority, status, tags_json, prerequisite_task_ids_json,
                    due_at, outcome_summary, outcome_items_json, resume_hint,
                    blocker_reason, blocker_needs_json, blocker_kind,
                    completed_at, last_outcome_at, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(task_id.as_str())
            .bind(session_id)
            .bind(turn_id)
            .bind(owner_user_id)
            .bind(draft.title)
            .bind(draft.details)
            .bind(draft.priority)
            .bind(draft.status)
            .bind(serde_json::to_string(&draft.tags)?)
            .bind(serde_json::to_string(&draft.prerequisite_task_ids)?)
            .bind(draft.due_at)
            .bind(draft.outcome_summary)
            .bind(serde_json::to_string(&draft.outcome_items)?)
            .bind(draft.resume_hint)
            .bind(draft.blocker_reason)
            .bind(serde_json::to_string(&draft.blocker_needs)?)
            .bind(draft.blocker_kind)
            .bind(Option::<String>::None)
            .bind(Option::<String>::None)
            .bind(now.as_str())
            .bind(now.as_str())
            .execute(&mut *transaction)
            .await
            .context("create local task board task")?;
            ids.push(task_id);
        }
        transaction
            .commit()
            .await
            .context("commit local task creation")?;
        let mut records = Vec::with_capacity(ids.len());
        for task_id in ids {
            records.push(
                select_task(self, owner_user_id, session_id, task_id.as_str())
                    .await?
                    .context("local task board task was not persisted")?,
            );
        }
        Ok(records)
    }
}
