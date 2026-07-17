// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_runtime::task_board::{
    format_local_task_board_prompt, LocalTaskBoardTaskRecord, LocalTaskBoardTaskRow,
};

use super::super::LocalDatabase;
use super::validation::require_local_task_scope;

impl LocalDatabase {
    pub(crate) async fn get_local_task_board_task(
        &self,
        owner_user_id: &str,
        session_id: &str,
        task_id: &str,
    ) -> Result<Option<LocalTaskBoardTaskRecord>> {
        require_local_task_scope(self, owner_user_id, session_id, None).await?;
        select_task(self, owner_user_id, session_id, task_id).await
    }

    pub(crate) async fn list_local_task_board_tasks(
        &self,
        owner_user_id: &str,
        session_id: &str,
        turn_id: Option<&str>,
        include_done: bool,
        limit: usize,
    ) -> Result<Vec<LocalTaskBoardTaskRecord>> {
        require_local_task_scope(self, owner_user_id, session_id, turn_id).await?;
        let rows = sqlx::query_as::<_, LocalTaskBoardTaskRow>(
            r#"
            SELECT tasks.id, tasks.session_id, tasks.turn_id, turns.user_message_id AS source_user_message_id,
                   tasks.title, tasks.details, tasks.priority, tasks.status,
                   tasks.tags_json, tasks.prerequisite_task_ids_json, tasks.due_at,
                   tasks.outcome_summary, tasks.outcome_items_json, tasks.resume_hint,
                   tasks.blocker_reason, tasks.blocker_needs_json, tasks.blocker_kind,
                   tasks.completed_at, tasks.last_outcome_at, tasks.created_at, tasks.updated_at
            FROM task_board_tasks AS tasks
            INNER JOIN turns ON turns.id = tasks.turn_id
            WHERE tasks.owner_user_id = ? AND tasks.session_id = ?
              AND (? IS NULL OR tasks.turn_id = ?)
              AND (? = 1 OR tasks.status != 'done')
            ORDER BY tasks.created_at ASC, tasks.id ASC
            LIMIT ?
            "#,
        )
        .bind(owner_user_id)
        .bind(session_id)
        .bind(turn_id)
        .bind(turn_id)
        .bind(include_done)
        .bind(limit.clamp(1, 200) as i64)
        .fetch_all(self.pool())
        .await
        .context("list local task board tasks")?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub(crate) async fn local_task_board_prompt(
        &self,
        owner_user_id: &str,
        session_id: &str,
    ) -> Result<String> {
        let tasks = self
            .list_local_task_board_tasks(owner_user_id, session_id, None, true, 200)
            .await?;
        Ok(format_local_task_board_prompt(tasks.as_slice()))
    }
}

pub(super) async fn select_task(
    database: &LocalDatabase,
    owner_user_id: &str,
    session_id: &str,
    task_id: &str,
) -> Result<Option<LocalTaskBoardTaskRecord>> {
    sqlx::query_as::<_, LocalTaskBoardTaskRow>(
        r#"
        SELECT tasks.id, tasks.session_id, tasks.turn_id, turns.user_message_id AS source_user_message_id,
               tasks.title, tasks.details, tasks.priority, tasks.status,
               tasks.tags_json, tasks.prerequisite_task_ids_json, tasks.due_at,
               tasks.outcome_summary, tasks.outcome_items_json, tasks.resume_hint,
               tasks.blocker_reason, tasks.blocker_needs_json, tasks.blocker_kind,
               tasks.completed_at, tasks.last_outcome_at, tasks.created_at, tasks.updated_at
        FROM task_board_tasks AS tasks
        INNER JOIN turns ON turns.id = tasks.turn_id
        WHERE tasks.id = ? AND tasks.session_id = ? AND tasks.owner_user_id = ?
        "#,
    )
    .bind(task_id)
    .bind(session_id)
    .bind(owner_user_id)
    .fetch_optional(database.pool())
    .await
    .context("get local task board task")
    .map(|record| record.map(Into::into))
}
