use super::*;

impl SqliteStore {
    pub(in crate::store) async fn list_model_configs(
        &self,
    ) -> Result<Vec<ModelConfigRecord>, String> {
        let rows =
            sqlx::query("SELECT * FROM model_configs ORDER BY datetime(updated_at) DESC, id DESC")
                .fetch_all(&self.pool)
                .await
                .map_err(|err| err.to_string())?;
        rows.iter().map(model_config_from_row).collect()
    }

    pub(in crate::store) async fn get_model_config(
        &self,
        id: &str,
    ) -> Result<Option<ModelConfigRecord>, String> {
        let row = sqlx::query("SELECT * FROM model_configs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(model_config_from_row).transpose()
    }
}
