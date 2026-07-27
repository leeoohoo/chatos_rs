// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_now_rfc3339;

use super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn recover_local_task_runs(&self) -> Result<u64> {
        let now = local_now_rfc3339();
        let mut transaction = self
            .begin_write()
            .await
            .context("recover local task runs")?;
        sqlx::query(
            r#"
            UPDATE project_work_items SET status = 'blocked', updated_at = ?
            WHERE id IN (
                SELECT task_id FROM local_task_runs
                WHERE status = 'running' AND task_kind = 'project_work_item'
            )
              AND LOWER(TRIM(status)) NOT IN
                  ('done', 'completed', 'succeeded', 'success', 'archived')
            "#,
        )
        .bind(now.as_str())
        .execute(&mut *transaction)
        .await
        .context("mark interrupted local work items blocked")?;
        sqlx::query(
            r#"
            UPDATE task_board_tasks SET status = 'blocked',
                blocker_reason = 'Local Connector stopped while this task was running',
                updated_at = ?
            WHERE id IN (
                SELECT task_id FROM local_task_runs
                WHERE status = 'running' AND task_kind = 'conversation_task'
            ) AND status NOT IN ('done', 'blocked')
            "#,
        )
        .bind(now.as_str())
        .execute(&mut *transaction)
        .await
        .context("mark interrupted local conversation tasks blocked")?;
        sqlx::query(
            r#"
            UPDATE turns SET status = 'failed', error_code = 'local_task_run_interrupted',
                error_message = 'Local Connector stopped while this task was running',
                finished_at = ?, updated_at = ?
            WHERE id IN (SELECT turn_id FROM local_task_runs WHERE status = 'running')
              AND status = 'running'
            "#,
        )
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(&mut *transaction)
        .await
        .context("mark interrupted local task turns failed")?;
        let result = sqlx::query(
            r#"
            UPDATE local_task_runs SET status = 'interrupted',
                error = 'Local Connector stopped while this task was running',
                worker_id = NULL, lease_expires_at = NULL, heartbeat_at = NULL,
                finished_at = ?, updated_at = ?
            WHERE status = 'running'
            "#,
        )
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(&mut *transaction)
        .await
        .context("recover interrupted local task runs")?;
        transaction
            .commit()
            .await
            .context("commit task run recovery")?;
        let terminal_sessions = sqlx::query_as::<_, (String, String, String, String)>(
            r#"
            SELECT runs.owner_user_id, runs.session_id, runs.id, runs.status
            FROM local_task_runs AS runs
            WHERE runs.status IN ('completed', 'failed', 'blocked', 'canceled', 'interrupted')
              AND EXISTS (
                  SELECT 1 FROM task_board_tasks AS tasks
                  WHERE tasks.owner_user_id = runs.owner_user_id
                    AND tasks.session_id = runs.session_id
                    AND tasks.task_session_id = runs.id
                    AND tasks.task_kind = 'task_manager'
              )
            "#,
        )
        .fetch_all(self.pool())
        .await
        .context("load terminal local Task Manager sessions during recovery")?;
        for (owner_user_id, session_id, run_id, status) in terminal_sessions {
            self.finalize_local_task_manager_session(
                owner_user_id.as_str(),
                session_id.as_str(),
                run_id.as_str(),
                status.as_str(),
            )
            .await
            .with_context(|| format!("finalize recovered local Task Manager session {run_id}"))?;
        }
        Ok(result.rows_affected())
    }
}
