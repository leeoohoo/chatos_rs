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
            WHERE id IN (SELECT task_id FROM local_task_runs WHERE status = 'running')
              AND status NOT IN ('done', 'completed', 'archived')
            "#,
        )
        .bind(now.as_str())
        .execute(&mut *transaction)
        .await
        .context("mark interrupted local work items blocked")?;
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
        Ok(result.rows_affected())
    }
}
