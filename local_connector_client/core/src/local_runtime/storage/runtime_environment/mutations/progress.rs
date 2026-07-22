// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_now_rfc3339;
use crate::local_runtime::environment::LocalEnvironmentProgressRecord;

use super::super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn fail_local_environment_analysis(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        error: &str,
    ) -> Result<()> {
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            UPDATE project_runtime_environments SET status = 'failed',
                analysis_summary = '本地项目运行环境分析失败。', last_agent_run_id = ?,
                last_error = ?, updated_at = ?
            WHERE project_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(run_id)
        .bind(error)
        .bind(now)
        .bind(project_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("fail local environment analysis")?;
        self.update_local_environment_progress(
            owner_user_id,
            project_id,
            Some(run_id),
            "failed",
            "failed",
            Some(100),
            "Local environment analysis failed",
            Some(error),
            true,
        )
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn update_local_environment_progress(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: Option<&str>,
        phase: &str,
        status: &str,
        progress_percent: Option<i64>,
        logs: &str,
        error: Option<&str>,
        finished: bool,
    ) -> Result<LocalEnvironmentProgressRecord> {
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO project_runtime_environment_progress (
                project_id, owner_user_id, run_id, phase, status, progress_percent,
                provider, started_at, updated_at, finished_at, logs, error
            ) VALUES (?, ?, ?, ?, ?, ?, 'local_connector', ?, ?, ?, ?, ?)
            ON CONFLICT(project_id) DO UPDATE SET
                run_id = excluded.run_id, phase = excluded.phase, status = excluded.status,
                progress_percent = excluded.progress_percent, updated_at = excluded.updated_at,
                started_at = CASE
                    WHEN project_runtime_environment_progress.run_id IS NOT excluded.run_id
                        THEN excluded.started_at
                    ELSE project_runtime_environment_progress.started_at
                END,
                finished_at = excluded.finished_at, logs = excluded.logs, error = excluded.error
            "#,
        )
        .bind(project_id)
        .bind(owner_user_id)
        .bind(run_id)
        .bind(phase)
        .bind(status)
        .bind(progress_percent)
        .bind(now.as_str())
        .bind(now.as_str())
        .bind(finished.then_some(now.as_str()))
        .bind(logs)
        .bind(error)
        .execute(self.pool())
        .await
        .context("update local environment analysis progress")?;
        self.get_local_environment_progress(owner_user_id, project_id)
            .await?
            .context("local environment analysis progress is unavailable")
    }
}
