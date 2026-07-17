// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use chatos_ai_runtime::TaskRunReport;
use chrono::{Duration, Utc};

use crate::local_now_rfc3339;
use crate::local_runtime::task_runner::LocalTaskRunRecord;

use super::super::LocalDatabase;
use super::queries::TASK_RUN_COLUMNS;

impl LocalDatabase {
    pub(crate) async fn claim_next_local_task_run(
        &self,
        worker_id: &str,
    ) -> Result<Option<LocalTaskRunRecord>> {
        let mut transaction = self.begin_write().await.context("claim local task run")?;
        let sql = format!(
            r#"
            SELECT {TASK_RUN_COLUMNS} FROM local_task_runs AS runs
            WHERE runs.status = 'queued' AND runs.cancel_requested = 0
              AND NOT EXISTS (
                SELECT 1 FROM work_item_dependencies AS dependencies
                INNER JOIN project_work_items AS prerequisite
                    ON prerequisite.id = dependencies.prerequisite_work_item_id
                WHERE dependencies.work_item_id = runs.task_id
                  AND prerequisite.status NOT IN ('done', 'completed')
              )
            ORDER BY runs.priority DESC, runs.created_at ASC, runs.id ASC
            LIMIT 1
            "#
        );
        let Some(candidate) = sqlx::query_as::<_, LocalTaskRunRecord>(sql.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .context("select queued local task run")?
        else {
            transaction
                .commit()
                .await
                .context("commit empty task claim")?;
            return Ok(None);
        };
        let now = local_now_rfc3339();
        let lease_expires_at = (Utc::now() + Duration::seconds(30)).to_rfc3339();
        let updated = sqlx::query(
            r#"
            UPDATE local_task_runs SET status = 'running', attempt = attempt + 1,
                worker_id = ?, lease_expires_at = ?, heartbeat_at = ?,
                started_at = COALESCE(started_at, ?), updated_at = ?
            WHERE id = ? AND status = 'queued' AND cancel_requested = 0
            "#,
        )
        .bind(worker_id)
        .bind(lease_expires_at)
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(candidate.id.as_str())
        .execute(&mut *transaction)
        .await
        .context("claim queued local task run")?;
        transaction
            .commit()
            .await
            .context("commit local task claim")?;
        if updated.rows_affected() == 0 {
            return Ok(None);
        }
        self.get_local_task_run(candidate.owner_user_id.as_str(), candidate.id.as_str())
            .await
    }

    pub(crate) async fn heartbeat_local_task_run(
        &self,
        run_id: &str,
        worker_id: &str,
    ) -> Result<bool> {
        let now = local_now_rfc3339();
        let lease_expires_at = (Utc::now() + Duration::seconds(30)).to_rfc3339();
        sqlx::query(
            r#"
            UPDATE local_task_runs SET heartbeat_at = ?, lease_expires_at = ?, updated_at = ?
            WHERE id = ? AND worker_id = ? AND status = 'running'
            "#,
        )
        .bind(now.as_str())
        .bind(lease_expires_at)
        .bind(now.as_str())
        .bind(run_id)
        .bind(worker_id)
        .execute(self.pool())
        .await
        .context("heartbeat local task run")
        .map(|result| result.rows_affected() == 1)
    }

    pub(crate) async fn complete_local_task_run(
        &self,
        run: &LocalTaskRunRecord,
        report: &TaskRunReport,
    ) -> Result<()> {
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            UPDATE local_task_runs SET status = 'completed', result_content = ?,
                result_reasoning = ?, tool_calls_json = ?, finish_reason = ?, usage_json = ?,
                error = NULL, worker_id = NULL, lease_expires_at = NULL,
                heartbeat_at = NULL, finished_at = ?, updated_at = ?
            WHERE id = ? AND status = 'running'
            "#,
        )
        .bind(report.content.as_deref())
        .bind(report.reasoning.as_deref())
        .bind(report.tool_calls.as_ref().map(ToString::to_string))
        .bind(report.finish_reason.as_deref())
        .bind(report.usage.as_ref().map(ToString::to_string))
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(run.id.as_str())
        .execute(self.pool())
        .await
        .context("complete local task run")?;
        Ok(())
    }

    pub(crate) async fn fail_local_task_run(
        &self,
        run: &LocalTaskRunRecord,
        status: &str,
        error: &str,
    ) -> Result<()> {
        let status = if status == "canceled" {
            "canceled"
        } else {
            "failed"
        };
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            UPDATE local_task_runs SET status = ?, error = ?, worker_id = NULL,
                lease_expires_at = NULL, heartbeat_at = NULL,
                finished_at = ?, updated_at = ?
            WHERE id = ? AND status = 'running'
            "#,
        )
        .bind(status)
        .bind(error)
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(run.id.as_str())
        .execute(self.pool())
        .await
        .context("fail local task run")?;
        Ok(())
    }
}
