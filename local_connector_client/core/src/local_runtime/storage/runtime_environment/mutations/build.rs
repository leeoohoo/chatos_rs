// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_now_rfc3339;
use crate::local_runtime::environment::LocalRuntimeEnvironmentRecord;

use super::super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn start_local_environment_build(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
    ) -> Result<LocalRuntimeEnvironmentRecord> {
        self.ensure_local_runtime_environment(owner_user_id, project_id)
            .await?;
        let now = local_now_rfc3339();
        let mut transaction = self
            .begin_write()
            .await
            .context("start local environment build")?;
        sqlx::query(
            r#"
            UPDATE project_runtime_environments
            SET status = 'pending_image_build', last_error = NULL, updated_at = ?
            WHERE project_id = ? AND owner_user_id = ? AND sandbox_enabled = 1
            "#,
        )
        .bind(now.as_str())
        .bind(project_id)
        .bind(owner_user_id)
        .execute(&mut *transaction)
        .await
        .context("mark local environment build running")?;
        sqlx::query(
            r#"
            UPDATE project_runtime_environment_images
            SET status = CASE
                    WHEN lower(environment_type) IN ('application', 'runtime')
                        THEN 'building'
                    ELSE 'preparing'
                END,
                error = NULL, updated_at = ?
            WHERE project_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(now.as_str())
        .bind(project_id)
        .bind(owner_user_id)
        .execute(&mut *transaction)
        .await
        .context("mark local environment images building")?;
        transaction
            .commit()
            .await
            .context("commit local environment build start")?;
        self.update_local_environment_progress(
            owner_user_id,
            project_id,
            Some(run_id),
            "building_image",
            "running",
            Some(10),
            "Preparing managed Docker Compose artifacts",
            None,
            false,
        )
        .await?;
        self.get_local_runtime_environment(owner_user_id, project_id)
            .await?
            .context("local project runtime environment is unavailable")
    }

    pub(crate) async fn finish_local_environment_build(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        image_refs: &[(String, String)],
        logs: &str,
    ) -> Result<LocalRuntimeEnvironmentRecord> {
        let now = local_now_rfc3339();
        let mut transaction = self
            .begin_write()
            .await
            .context("finish local environment build")?;
        sqlx::query(
            r#"
            UPDATE project_runtime_environments
            SET status = 'ready',
                analysis_summary = ?,
                last_error = NULL,
                updated_at = ?
            WHERE project_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(format!(
            "本地项目级 Docker Compose 环境已构建并启动，共运行 {} 个服务。",
            image_refs.len()
        ))
        .bind(now.as_str())
        .bind(project_id)
        .bind(owner_user_id)
        .execute(&mut *transaction)
        .await
        .context("mark local environment ready")?;
        sqlx::query(
            r#"
            UPDATE project_runtime_environment_images
            SET status = 'planned', error = NULL, updated_at = ?
            WHERE project_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(now.as_str())
        .bind(project_id)
        .bind(owner_user_id)
        .execute(&mut *transaction)
        .await
        .context("reset local environment image build states")?;
        for (image_id, image_ref) in image_refs {
            sqlx::query(
                r#"
                UPDATE project_runtime_environment_images
                SET image_ref = ?, status = 'running', error = NULL, updated_at = ?
                WHERE id = ? AND project_id = ? AND owner_user_id = ?
                "#,
            )
            .bind(image_ref)
            .bind(now.as_str())
            .bind(image_id)
            .bind(project_id)
            .bind(owner_user_id)
            .execute(&mut *transaction)
            .await
            .context("mark local environment image running")?;
        }
        transaction
            .commit()
            .await
            .context("commit local environment build result")?;
        self.update_local_environment_progress(
            owner_user_id,
            project_id,
            Some(run_id),
            "completed",
            "succeeded",
            Some(100),
            truncate_build_logs(logs).as_str(),
            None,
            true,
        )
        .await?;
        self.get_local_runtime_environment(owner_user_id, project_id)
            .await?
            .context("local project runtime environment is unavailable")
    }

    pub(crate) async fn fail_local_environment_build(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        error: &str,
    ) -> Result<()> {
        let now = local_now_rfc3339();
        let mut transaction = self
            .begin_write()
            .await
            .context("fail local environment build")?;
        sqlx::query(
            r#"
            UPDATE project_runtime_environments
            SET status = 'failed', last_error = ?, updated_at = ?
            WHERE project_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(error)
        .bind(now.as_str())
        .bind(project_id)
        .bind(owner_user_id)
        .execute(&mut *transaction)
        .await
        .context("mark local environment build failed")?;
        sqlx::query(
            r#"
            UPDATE project_runtime_environment_images
            SET status = 'failed', error = ?, updated_at = ?
            WHERE project_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(error)
        .bind(now.as_str())
        .bind(project_id)
        .bind(owner_user_id)
        .execute(&mut *transaction)
        .await
        .context("mark local environment images failed")?;
        transaction
            .commit()
            .await
            .context("commit local environment build failure")?;
        self.update_local_environment_progress(
            owner_user_id,
            project_id,
            Some(run_id),
            "failed",
            "failed",
            Some(100),
            "Local Docker Compose environment build failed",
            Some(error),
            true,
        )
        .await?;
        Ok(())
    }
}

fn truncate_build_logs(value: &str) -> String {
    const MAX_LOG_BYTES: usize = 80_000;
    if value.len() <= MAX_LOG_BYTES {
        return value.to_string();
    }
    let mut start = value.len().saturating_sub(MAX_LOG_BYTES);
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    format!("... output truncated ...\n{}", &value[start..])
}
