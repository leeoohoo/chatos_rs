// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};

use crate::local_runtime::environment::{
    LocalEnvironmentProgressRecord, LocalRuntimeEnvironmentImageRecord,
    LocalRuntimeEnvironmentRecord,
};

use super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn get_local_runtime_environment(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<Option<LocalRuntimeEnvironmentRecord>> {
        sqlx::query_as::<_, LocalRuntimeEnvironmentRecord>(
            r#"
            SELECT project_id, owner_user_id, status, sandbox_enabled,
                   sandbox_provider, file_provider, analysis_summary,
                   not_runnable_reason, detected_stack_json,
                   required_services_json, env_vars_json,
                   generated_config_files_json, last_agent_run_id, last_error,
                   created_at, updated_at
            FROM project_runtime_environments
            WHERE project_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(project_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool())
        .await
        .context("get local project runtime environment")
    }

    pub(crate) async fn list_local_runtime_environment_images(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<Vec<LocalRuntimeEnvironmentImageRecord>> {
        sqlx::query_as::<_, LocalRuntimeEnvironmentImageRecord>(
            r#"
            SELECT id, project_id, environment_key, environment_type,
                   display_name, image_id, image_ref, image_provider,
                   features_json, ports_json, env_vars_json, status, error,
                   created_at, updated_at
            FROM project_runtime_environment_images
            WHERE project_id = ? AND owner_user_id = ?
            ORDER BY environment_type ASC, display_name ASC, id ASC
            "#,
        )
        .bind(project_id)
        .bind(owner_user_id)
        .fetch_all(self.pool())
        .await
        .context("list local project runtime environment images")
    }

    pub(crate) async fn get_local_environment_progress(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<Option<LocalEnvironmentProgressRecord>> {
        sqlx::query_as::<_, LocalEnvironmentProgressRecord>(
            r#"
            SELECT project_id, run_id, phase, status, progress_percent,
                   provider, started_at, updated_at, finished_at, logs, error
            FROM project_runtime_environment_progress
            WHERE project_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(project_id)
        .bind(owner_user_id)
        .fetch_optional(self.pool())
        .await
        .context("get local project runtime environment progress")
    }
}
