// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use serde_json::Value;
use uuid::Uuid;

use crate::local_now_rfc3339;
use crate::local_runtime::task_runner::{EnqueueLocalTaskRunInput, LocalTaskRunRecord};

use super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn enqueue_local_task_run(
        &self,
        input: EnqueueLocalTaskRunInput,
    ) -> Result<LocalTaskRunRecord> {
        let work_item = self
            .get_local_work_item(input.owner_user_id.as_str(), input.task_id.as_str())
            .await?
            .context("local task run work item was not found")?;
        if work_item.project_id != input.project_id {
            return Err(anyhow::anyhow!(
                "local task run work item belongs to another project"
            ));
        }
        let id = format!("lc_task_run_{}", Uuid::new_v4());
        let turn_id = format!("lc_turn_task_{}", Uuid::new_v4());
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO local_task_runs (
                id, owner_user_id, project_id, requirement_id, task_id,
                session_id, turn_id, execution_group_id, status, priority,
                prompt, model_config_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'queued', ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.as_str())
        .bind(input.owner_user_id.as_str())
        .bind(input.project_id.as_str())
        .bind(input.requirement_id.as_deref())
        .bind(input.task_id.as_str())
        .bind(input.session_id.as_str())
        .bind(turn_id)
        .bind(input.execution_group_id.as_str())
        .bind(input.priority)
        .bind(input.prompt.as_str())
        .bind(input.model_config_id.as_str())
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(self.pool())
        .await
        .context("enqueue local task run")?;
        self.append_local_task_run_event(
            input.owner_user_id.as_str(),
            id.as_str(),
            "task.queued",
            serde_json::json!({ "task_id": input.task_id }),
        )
        .await?;
        self.get_local_task_run(input.owner_user_id.as_str(), id.as_str())
            .await?
            .context("local task run was not persisted")
    }

    pub(crate) async fn request_local_task_run_cancel(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<Option<LocalTaskRunRecord>> {
        let now = local_now_rfc3339();
        let result = sqlx::query(
            r#"
            UPDATE local_task_runs SET
                cancel_requested = 1,
                status = CASE WHEN status = 'queued' THEN 'canceled' ELSE status END,
                finished_at = CASE WHEN status = 'queued' THEN ? ELSE finished_at END,
                updated_at = ?
            WHERE id = ? AND owner_user_id = ? AND status IN ('queued', 'running')
            "#,
        )
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(run_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("request local task run cancellation")?;
        if result.rows_affected() == 0 {
            return self.get_local_task_run(owner_user_id, run_id).await;
        }
        let run = self.get_local_task_run(owner_user_id, run_id).await?;
        if let Some(run) = run.as_ref().filter(|run| run.status == "canceled") {
            sqlx::query("UPDATE project_work_items SET status = 'todo', updated_at = ? WHERE id = ? AND owner_user_id = ?")
                .bind(now.as_str())
                .bind(run.task_id.as_str())
                .bind(owner_user_id)
                .execute(self.pool())
                .await
                .context("reset canceled local work item")?;
        }
        Ok(run)
    }

    pub(crate) async fn retry_local_task_run(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<Option<LocalTaskRunRecord>> {
        let now = local_now_rfc3339();
        let turn_id = format!("lc_turn_task_{}", Uuid::new_v4());
        let result = sqlx::query(
            r#"
            UPDATE local_task_runs SET status = 'queued', turn_id = ?, cancel_requested = 0,
                worker_id = NULL, lease_expires_at = NULL, heartbeat_at = NULL,
                result_content = NULL, result_reasoning = NULL, tool_calls_json = NULL,
                finish_reason = NULL, usage_json = NULL, error = NULL,
                started_at = NULL, finished_at = NULL, updated_at = ?
            WHERE id = ? AND owner_user_id = ?
              AND status IN ('failed', 'canceled', 'interrupted') AND attempt < max_attempts
            "#,
        )
        .bind(turn_id)
        .bind(now.as_str())
        .bind(run_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("retry local task run")?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        sqlx::query(
            "UPDATE project_work_items SET status = 'todo', updated_at = ? WHERE id = (SELECT task_id FROM local_task_runs WHERE id = ?) AND owner_user_id = ?",
        )
        .bind(now.as_str())
        .bind(run_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("reset retried local work item")?;
        self.get_local_task_run(owner_user_id, run_id).await
    }

    pub(crate) async fn append_local_task_run_event(
        &self,
        owner_user_id: &str,
        run_id: &str,
        event_name: &str,
        payload: Value,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO local_task_run_events (run_id, owner_user_id, event_name, payload_json, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(run_id)
        .bind(owner_user_id)
        .bind(event_name)
        .bind(payload.to_string())
        .bind(local_now_rfc3339())
        .execute(self.pool())
        .await
        .context("append local task run event")?;
        Ok(())
    }
}
