// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl PostgresStore {
    pub(in crate::store) async fn list_model_configs(
        &self,
    ) -> Result<Vec<ModelConfigRecord>, String> {
        self.request_user_service_model_catalog("/api/internal/task-runner/model-configs")
            .await
    }

    pub(in crate::store) async fn get_model_config(
        &self,
        id: &str,
    ) -> Result<Option<ModelConfigRecord>, String> {
        let id = id.trim();
        if id.is_empty() {
            return Ok(None);
        }
        let path = format!(
            "/api/internal/task-runner/model-configs/{}",
            urlencoding::encode(id)
        );
        match self
            .request_user_service_model::<ModelConfigRecord>(&path)
            .await
        {
            Ok(model) => Ok(Some(model)),
            Err(error) if error.starts_with("404 ") => Ok(None),
            Err(error) => Err(error),
        }
    }

    #[cfg(test)]
    pub(in crate::store) async fn save_model_config(
        &self,
        model: ModelConfigRecord,
    ) -> Result<ModelConfigRecord, String> {
        let _ = model;
        Err("model configurations are managed exclusively by User Service".to_string())
    }

    pub(in crate::store) async fn get_runtime_settings(
        &self,
    ) -> Result<Option<RuntimeSettingsRecord>, String> {
        sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM runtime_settings WHERE id='system'",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(db_error)?
        .map(decode_json)
        .transpose()
    }
}
