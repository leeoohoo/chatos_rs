// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_now_rfc3339;
use crate::local_runtime::project_management::{
    LocalProjectProfileRecord, UpsertLocalProjectProfileInput,
};

use super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn get_local_project_profile(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<Option<LocalProjectProfileRecord>> {
        self.require_local_project(owner_user_id, project_id)
            .await?;
        sqlx::query_as::<_, LocalProjectProfileRecord>(
            r#"
            SELECT project_id, description, git_url, background, introduction,
                   created_at, updated_at
            FROM project_profiles WHERE project_id = ?
            "#,
        )
        .bind(project_id)
        .fetch_optional(self.pool())
        .await
        .context("get local project profile")
    }

    pub(crate) async fn upsert_local_project_profile(
        &self,
        owner_user_id: &str,
        project_id: &str,
        input: UpsertLocalProjectProfileInput,
    ) -> Result<LocalProjectProfileRecord> {
        self.require_local_project(owner_user_id, project_id)
            .await?;
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO project_profiles (
                project_id, description, git_url, background, introduction,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(project_id) DO UPDATE SET
                description = COALESCE(excluded.description, project_profiles.description),
                git_url = COALESCE(excluded.git_url, project_profiles.git_url),
                background = COALESCE(excluded.background, project_profiles.background),
                introduction = COALESCE(excluded.introduction, project_profiles.introduction),
                updated_at = excluded.updated_at
            "#,
        )
        .bind(project_id)
        .bind(input.description.as_deref())
        .bind(input.git_url.as_deref())
        .bind(input.background.as_deref())
        .bind(input.introduction.as_deref())
        .bind(now.as_str())
        .bind(now.as_str())
        .execute(self.pool())
        .await
        .context("upsert local project profile")?;
        self.get_local_project_profile(owner_user_id, project_id)
            .await?
            .context("local project profile was not persisted")
    }

    pub(crate) async fn update_local_project_identity(
        &self,
        owner_user_id: &str,
        project_id: &str,
        name: Option<&str>,
        root_relative_path: Option<&str>,
    ) -> Result<()> {
        self.require_local_project(owner_user_id, project_id)
            .await?;
        sqlx::query(
            r#"
            UPDATE local_projects SET
                project_name = COALESCE(?, project_name),
                root_relative_path = COALESCE(?, root_relative_path),
                updated_at = ?
            WHERE project_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(name)
        .bind(root_relative_path)
        .bind(local_now_rfc3339())
        .bind(project_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("update local project identity")?;
        Ok(())
    }
}
