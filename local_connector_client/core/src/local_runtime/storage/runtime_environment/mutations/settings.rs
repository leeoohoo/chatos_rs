// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_now_rfc3339;
use crate::local_runtime::environment::LocalRuntimeEnvironmentRecord;

use super::super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn ensure_local_runtime_environment(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<LocalRuntimeEnvironmentRecord> {
        if self.get_project(project_id, owner_user_id).await?.is_none() {
            return Err(anyhow::anyhow!("local project was not found"));
        }
        if let Some(record) = self
            .get_local_runtime_environment(owner_user_id, project_id)
            .await?
        {
            return Ok(record);
        }
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO project_runtime_environments (
                project_id, owner_user_id, status, sandbox_enabled,
                sandbox_provider, file_provider, created_at, updated_at
            ) VALUES (?, ?, 'pending', 1, 'local_connector', 'local_connector', ?, ?)
            "#,
        )
        .bind(project_id)
        .bind(owner_user_id)
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(self.pool())
        .await
        .context("create local project runtime environment")?;
        self.get_local_runtime_environment(owner_user_id, project_id)
            .await?
            .context("local project runtime environment was not persisted")
    }

    pub(crate) async fn set_local_environment_enabled(
        &self,
        owner_user_id: &str,
        project_id: &str,
        enabled: bool,
    ) -> Result<LocalRuntimeEnvironmentRecord> {
        self.ensure_local_runtime_environment(owner_user_id, project_id)
            .await?;
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            UPDATE project_runtime_environments
            SET sandbox_enabled = ?, status = CASE WHEN ? THEN 'pending' ELSE 'disabled' END,
                last_error = NULL, updated_at = ?
            WHERE project_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(enabled)
        .bind(enabled)
        .bind(now)
        .bind(project_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("update local project runtime environment settings")?;
        if !enabled {
            sqlx::query("DELETE FROM project_runtime_environment_images WHERE project_id = ? AND owner_user_id = ?")
                .bind(project_id)
                .bind(owner_user_id)
                .execute(self.pool())
                .await
                .context("delete local environment image plans")?;
        }
        self.get_local_runtime_environment(owner_user_id, project_id)
            .await?
            .context("local project runtime environment is unavailable")
    }
}
