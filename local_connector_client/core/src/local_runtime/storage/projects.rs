// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_now_rfc3339;

use super::{LocalDatabase, LocalProjectRecord, UpsertLocalProjectInput};

impl LocalDatabase {
    pub(crate) async fn upsert_project(
        &self,
        input: UpsertLocalProjectInput,
    ) -> Result<LocalProjectRecord> {
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO local_projects (
                project_id, owner_user_id, device_id, workspace_id, project_name,
                root_relative_path, execution_plane, runtime_schema_version,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, 'local_connector', 1, ?, ?)
            ON CONFLICT(project_id) DO UPDATE SET
                owner_user_id = excluded.owner_user_id,
                device_id = excluded.device_id,
                workspace_id = excluded.workspace_id,
                project_name = excluded.project_name,
                root_relative_path = excluded.root_relative_path,
                execution_plane = 'local_connector',
                runtime_schema_version = 1,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(input.project_id.as_str())
        .bind(input.owner_user_id.as_str())
        .bind(input.device_id.as_str())
        .bind(input.workspace_id.as_str())
        .bind(input.project_name.as_str())
        .bind(input.root_relative_path.as_deref())
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(self.pool())
        .await
        .context("upsert local runtime project")?;

        self.get_project(input.project_id.as_str(), input.owner_user_id.as_str())
            .await?
            .ok_or_else(|| anyhow::anyhow!("local runtime project was not persisted"))
    }

    pub(crate) async fn get_project(
        &self,
        project_id: &str,
        owner_user_id: &str,
    ) -> Result<Option<LocalProjectRecord>> {
        sqlx::query_as::<_, LocalProjectRecord>(
            r#"
            SELECT project_id, owner_user_id, device_id, workspace_id, project_name,
                   root_relative_path, execution_plane, runtime_schema_version,
                   created_at, updated_at
            FROM local_projects
            WHERE project_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(project_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool())
        .await
        .context("get local runtime project")
    }

    pub(crate) async fn list_projects(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<LocalProjectRecord>> {
        sqlx::query_as::<_, LocalProjectRecord>(
            r#"
            SELECT project_id, owner_user_id, device_id, workspace_id, project_name,
                   root_relative_path, execution_plane, runtime_schema_version,
                   created_at, updated_at
            FROM local_projects
            WHERE owner_user_id = ?
            ORDER BY updated_at DESC, project_id ASC
            "#,
        )
        .bind(owner_user_id)
        .fetch_all(self.pool())
        .await
        .context("list local runtime projects")
    }

    pub(crate) async fn delete_project(
        &self,
        project_id: &str,
        owner_user_id: &str,
    ) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM local_projects WHERE project_id = ? AND owner_user_id = ?")
                .bind(project_id)
                .bind(owner_user_id)
                .execute(self.pool())
                .await
                .context("delete local runtime project")?;
        Ok(result.rows_affected() > 0)
    }
}
