use super::*;

impl SqliteStore {
    pub(in crate::store) async fn save_model_config(
        &self,
        model: ModelConfigRecord,
    ) -> Result<ModelConfigRecord, String> {
        sqlx::query(
            "INSERT INTO model_configs (
                id, name, provider, base_url, api_key, model, usage_scenario, temperature, max_output_tokens,
                thinking_level, supports_responses, instructions, request_cwd,
                include_prompt_cache_retention, request_body_limit_bytes, enabled,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                provider = excluded.provider,
                base_url = excluded.base_url,
                api_key = excluded.api_key,
                model = excluded.model,
                usage_scenario = excluded.usage_scenario,
                temperature = excluded.temperature,
                max_output_tokens = excluded.max_output_tokens,
                thinking_level = excluded.thinking_level,
                supports_responses = excluded.supports_responses,
                instructions = excluded.instructions,
                request_cwd = excluded.request_cwd,
                include_prompt_cache_retention = excluded.include_prompt_cache_retention,
                request_body_limit_bytes = excluded.request_body_limit_bytes,
                enabled = excluded.enabled,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
        )
        .bind(&model.id)
        .bind(&model.name)
        .bind(&model.provider)
        .bind(&model.base_url)
        .bind(&model.api_key)
        .bind(&model.model)
        .bind(model.usage_scenario.clone())
        .bind(model.temperature)
        .bind(model.max_output_tokens)
        .bind(model.thinking_level.clone())
        .bind(bool_to_int(model.supports_responses))
        .bind(model.instructions.clone())
        .bind(model.request_cwd.clone())
        .bind(bool_to_int(model.include_prompt_cache_retention))
        .bind(model.request_body_limit_bytes.map(|value| value as i64))
        .bind(bool_to_int(model.enabled))
        .bind(&model.created_at)
        .bind(&model.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(model)
    }

    pub(in crate::store) async fn delete_model_config(&self, id: &str) -> Result<bool, String> {
        let mut tx = self.pool.begin().await.map_err(|err| err.to_string())?;
        let result = sqlx::query("DELETE FROM model_configs WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        sqlx::query(
            "UPDATE tasks SET default_model_config_id = NULL WHERE default_model_config_id = ?",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
        tx.commit().await.map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }
}
