use super::*;

impl SqliteStore {
    pub(in crate::store) async fn get_runtime_settings(
        &self,
    ) -> Result<Option<RuntimeSettingsRecord>, String> {
        let row = sqlx::query(
            "SELECT
                id,
                task_execution_max_iterations,
                execution_timeout_ms,
                tool_result_model_max_chars,
                tool_results_model_total_max_chars,
                execution_environment_mode,
                sandbox_manager_base_url,
                sandbox_lease_ttl_seconds,
                created_at,
                updated_at
             FROM runtime_settings
             WHERE id = 'system'
             LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        row.as_ref().map(runtime_settings_from_row).transpose()
    }

    pub(in crate::store) async fn save_runtime_settings(
        &self,
        settings: RuntimeSettingsRecord,
    ) -> Result<RuntimeSettingsRecord, String> {
        sqlx::query(
            "INSERT INTO runtime_settings (
                id,
                task_execution_max_iterations,
                execution_timeout_ms,
                tool_result_model_max_chars,
                tool_results_model_total_max_chars,
                execution_environment_mode,
                sandbox_manager_base_url,
                sandbox_lease_ttl_seconds,
                created_at,
                updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                task_execution_max_iterations = excluded.task_execution_max_iterations,
                execution_timeout_ms = excluded.execution_timeout_ms,
                tool_result_model_max_chars = excluded.tool_result_model_max_chars,
                tool_results_model_total_max_chars = excluded.tool_results_model_total_max_chars,
                execution_environment_mode = excluded.execution_environment_mode,
                sandbox_manager_base_url = excluded.sandbox_manager_base_url,
                sandbox_lease_ttl_seconds = excluded.sandbox_lease_ttl_seconds,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
        )
        .bind(&settings.id)
        .bind(settings.task_execution_max_iterations as i64)
        .bind(settings.execution_timeout_ms.map(|value| value as i64))
        .bind(settings.tool_result_model_max_chars as i64)
        .bind(settings.tool_results_model_total_max_chars as i64)
        .bind(&settings.execution_environment_mode)
        .bind(&settings.sandbox_manager_base_url)
        .bind(settings.sandbox_lease_ttl_seconds as i64)
        .bind(&settings.created_at)
        .bind(&settings.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(settings)
    }
}
