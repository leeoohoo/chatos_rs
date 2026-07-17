// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_runtime::task_runner::LocalTaskRunRecord;

use super::super::LocalDatabase;

pub(super) const TASK_RUN_COLUMNS: &str = r#"
    id, owner_user_id, project_id, requirement_id, task_id, session_id,
    turn_id, execution_group_id, status, priority, prompt, model_config_id,
    attempt, max_attempts, worker_id, lease_expires_at, heartbeat_at,
    cancel_requested, result_content, result_reasoning, tool_calls_json,
    finish_reason, usage_json, error, created_at, started_at, finished_at, updated_at
"#;

impl LocalDatabase {
    pub(crate) async fn get_local_task_run(
        &self,
        owner_user_id: &str,
        run_id: &str,
    ) -> Result<Option<LocalTaskRunRecord>> {
        let sql = format!(
            "SELECT {TASK_RUN_COLUMNS} FROM local_task_runs WHERE id = ? AND owner_user_id = ?"
        );
        sqlx::query_as::<_, LocalTaskRunRecord>(sql.as_str())
            .bind(run_id)
            .bind(owner_user_id)
            .fetch_optional(self.pool())
            .await
            .context("get local task run")
    }

    pub(crate) async fn list_local_requirement_task_runs(
        &self,
        owner_user_id: &str,
        project_id: &str,
        requirement_id: &str,
    ) -> Result<Vec<LocalTaskRunRecord>> {
        let sql = format!(
            r#"
            SELECT {TASK_RUN_COLUMNS} FROM local_task_runs
            WHERE owner_user_id = ? AND project_id = ? AND requirement_id = ?
            ORDER BY created_at DESC, id DESC
            "#
        );
        sqlx::query_as::<_, LocalTaskRunRecord>(sql.as_str())
            .bind(owner_user_id)
            .bind(project_id)
            .bind(requirement_id)
            .fetch_all(self.pool())
            .await
            .context("list local requirement task runs")
    }

    pub(crate) async fn local_task_run_cancel_requested(&self, run_id: &str) -> Result<bool> {
        sqlx::query_scalar::<_, bool>("SELECT cancel_requested FROM local_task_runs WHERE id = ?")
            .bind(run_id)
            .fetch_optional(self.pool())
            .await
            .context("read local task run cancellation")
            .map(|value| value.unwrap_or(true))
    }
}
