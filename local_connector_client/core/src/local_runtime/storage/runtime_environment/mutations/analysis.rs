// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::local_now_rfc3339;
use crate::local_runtime::environment::{
    LocalEnvironmentAnalysisResult, LocalRuntimeEnvironmentRecord,
};

use super::super::super::LocalDatabase;

impl LocalDatabase {
    pub(crate) async fn start_local_environment_analysis(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
    ) -> Result<LocalRuntimeEnvironmentRecord> {
        self.ensure_local_runtime_environment(owner_user_id, project_id)
            .await?;
        let now = local_now_rfc3339();
        sqlx::query(
            r#"
            UPDATE project_runtime_environments
            SET status = 'analyzing', last_agent_run_id = ?, last_error = NULL,
                analysis_summary = NULL, not_runnable_reason = NULL, updated_at = ?
            WHERE project_id = ? AND owner_user_id = ? AND sandbox_enabled = 1
            "#,
        )
        .bind(run_id)
        .bind(now.as_str())
        .bind(project_id)
        .bind(owner_user_id)
        .execute(self.pool())
        .await
        .context("start local project runtime environment analysis")?;
        self.update_local_environment_progress(
            owner_user_id,
            project_id,
            Some(run_id),
            "scanning_project",
            "running",
            Some(10),
            "Scanning local project manifests",
            None,
            false,
        )
        .await?;
        self.get_local_runtime_environment(owner_user_id, project_id)
            .await?
            .context("local project runtime environment is unavailable")
    }

    pub(crate) async fn finish_local_environment_analysis(
        &self,
        owner_user_id: &str,
        project_id: &str,
        run_id: &str,
        analysis: &LocalEnvironmentAnalysisResult,
    ) -> Result<LocalRuntimeEnvironmentRecord> {
        let now = local_now_rfc3339();
        let mut transaction = self
            .begin_write()
            .await
            .context("finish local environment")?;
        sqlx::query(
            r#"
            UPDATE project_runtime_environments SET
                status = ?, analysis_summary = ?, not_runnable_reason = ?,
                detected_stack_json = ?, required_services_json = ?, env_vars_json = ?,
                generated_config_files_json = ?, last_agent_run_id = ?, last_error = NULL,
                updated_at = ?
            WHERE project_id = ? AND owner_user_id = ?
            "#,
        )
        .bind(analysis.status.as_str())
        .bind(analysis.analysis_summary.as_str())
        .bind(analysis.not_runnable_reason.as_deref())
        .bind(serde_json::to_string(&analysis.detected_stack)?)
        .bind(serde_json::to_string(&analysis.required_services)?)
        .bind(serde_json::to_string(&analysis.env_vars)?)
        .bind(serde_json::to_string(&analysis.generated_config_files)?)
        .bind(run_id)
        .bind(now.as_str())
        .bind(project_id)
        .bind(owner_user_id)
        .execute(&mut *transaction)
        .await
        .context("save local environment analysis")?;
        replace_image_plans(
            &mut transaction,
            owner_user_id,
            project_id,
            now.as_str(),
            analysis,
        )
        .await?;
        transaction
            .commit()
            .await
            .context("commit local environment")?;
        self.get_local_runtime_environment(owner_user_id, project_id)
            .await?
            .context("local project runtime environment is unavailable")
    }
}

async fn replace_image_plans(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    owner_user_id: &str,
    project_id: &str,
    now: &str,
    analysis: &LocalEnvironmentAnalysisResult,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM project_runtime_environment_images WHERE project_id = ? AND owner_user_id = ?",
    )
    .bind(project_id)
    .bind(owner_user_id)
    .execute(&mut **transaction)
    .await
    .context("replace local environment image plans")?;
    for plan in &analysis.images {
        sqlx::query(
            r#"
            INSERT INTO project_runtime_environment_images (
                id, project_id, owner_user_id, environment_key, environment_type,
                display_name, image_ref, image_provider, features_json, ports_json,
                env_vars_json, status, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, 'local_connector', ?, ?, ?, 'planned', ?, ?)
            "#,
        )
        .bind(format!("lc_env_image_{}", Uuid::new_v4()))
        .bind(project_id)
        .bind(owner_user_id)
        .bind(plan.environment_key.trim())
        .bind(plan.environment_type.trim())
        .bind(plan.display_name.trim())
        .bind(plan.image_ref.as_deref())
        .bind(serde_json::to_string(&plan.features)?)
        .bind(serde_json::to_string(&plan.ports)?)
        .bind(serde_json::to_string(&plan.env_vars)?)
        .bind(now)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .context("save local environment image plan")?;
    }
    Ok(())
}
