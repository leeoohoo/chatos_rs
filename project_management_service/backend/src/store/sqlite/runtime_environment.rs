// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::sqlite_rows::{runtime_environment_from_row, runtime_environment_image_from_row};
use super::SqliteStore;
use crate::models::*;

impl SqliteStore {
    pub async fn get_project_runtime_environment(
        &self,
        project_id: &str,
    ) -> Result<Option<ProjectRuntimeEnvironmentRecord>, String> {
        let row = sqlx::query("SELECT * FROM project_runtime_environments WHERE project_id = ?1")
            .bind(project_id.trim())
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.as_ref().map(runtime_environment_from_row))
    }

    pub async fn upsert_project_runtime_environment(
        &self,
        environment: &ProjectRuntimeEnvironmentRecord,
    ) -> Result<ProjectRuntimeEnvironmentRecord, String> {
        sqlx::query(
            "INSERT INTO project_runtime_environments (
                project_id, status, sandbox_enabled, sandbox_provider, file_provider,
                analysis_summary, not_runnable_reason, detected_stack_json,
                required_services_json, env_vars_json, last_agent_run_id, last_error,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(project_id) DO UPDATE SET
                status = excluded.status,
                sandbox_enabled = excluded.sandbox_enabled,
                sandbox_provider = excluded.sandbox_provider,
                file_provider = excluded.file_provider,
                analysis_summary = excluded.analysis_summary,
                not_runnable_reason = excluded.not_runnable_reason,
                detected_stack_json = excluded.detected_stack_json,
                required_services_json = excluded.required_services_json,
                env_vars_json = excluded.env_vars_json,
                last_agent_run_id = excluded.last_agent_run_id,
                last_error = excluded.last_error,
                updated_at = excluded.updated_at",
        )
        .bind(&environment.project_id)
        .bind(environment.status.as_str())
        .bind(if environment.sandbox_enabled { 1 } else { 0 })
        .bind(environment.sandbox_provider.as_str())
        .bind(environment.file_provider.as_str())
        .bind(&environment.analysis_summary)
        .bind(&environment.not_runnable_reason)
        .bind(json_string(&environment.detected_stack)?)
        .bind(json_string(&environment.required_services)?)
        .bind(json_string(&environment.env_vars)?)
        .bind(&environment.last_agent_run_id)
        .bind(&environment.last_error)
        .bind(&environment.created_at)
        .bind(&environment.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(environment.clone())
    }

    pub async fn list_project_runtime_environment_images(
        &self,
        project_id: &str,
    ) -> Result<Vec<ProjectRuntimeEnvironmentImageRecord>, String> {
        let rows = sqlx::query(
            "SELECT * FROM project_runtime_environment_images
             WHERE project_id = ?1
             ORDER BY environment_key ASC, id ASC",
        )
        .bind(project_id.trim())
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows
            .iter()
            .map(runtime_environment_image_from_row)
            .collect())
    }

    pub async fn replace_project_runtime_environment_images(
        &self,
        project_id: &str,
        images: &[ProjectRuntimeEnvironmentImageRecord],
    ) -> Result<Vec<ProjectRuntimeEnvironmentImageRecord>, String> {
        let mut tx = self.pool.begin().await.map_err(|err| err.to_string())?;
        sqlx::query("DELETE FROM project_runtime_environment_images WHERE project_id = ?1")
            .bind(project_id.trim())
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        for image in images {
            sqlx::query(
                "INSERT INTO project_runtime_environment_images (
                    id, project_id, environment_key, environment_type, display_name,
                    image_id, image_ref, image_provider, features_json, ports_json,
                    env_vars_json, status, error, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )
            .bind(&image.id)
            .bind(project_id.trim())
            .bind(&image.environment_key)
            .bind(&image.environment_type)
            .bind(&image.display_name)
            .bind(&image.image_id)
            .bind(&image.image_ref)
            .bind(image.image_provider.as_str())
            .bind(json_string(&image.features)?)
            .bind(json_string(&image.ports)?)
            .bind(json_string(&image.env_vars)?)
            .bind(&image.status)
            .bind(&image.error)
            .bind(&image.created_at)
            .bind(&image.updated_at)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        }
        tx.commit().await.map_err(|err| err.to_string())?;
        Ok(images.to_vec())
    }
}

fn json_string(value: &serde_json::Value) -> Result<String, String> {
    serde_json::to_string(value).map_err(|err| err.to_string())
}
