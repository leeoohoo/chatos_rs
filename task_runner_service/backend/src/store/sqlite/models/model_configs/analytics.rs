use super::*;

impl SqliteStore {
    pub(in crate::store) async fn list_model_config_usage(
        &self,
    ) -> Result<Vec<ModelConfigUsageRecord>, String> {
        let rows = sqlx::query(
            "WITH task_counts AS (
                SELECT default_model_config_id AS model_config_id, COUNT(1) AS task_count
                FROM tasks
                WHERE default_model_config_id IS NOT NULL
                GROUP BY default_model_config_id
            ),
            run_counts AS (
                SELECT model_config_id, COUNT(1) AS run_count
                FROM task_runs
                GROUP BY model_config_id
            ),
            model_ids AS (
                SELECT model_config_id FROM task_counts
                UNION
                SELECT model_config_id FROM run_counts
            )
            SELECT
                model_ids.model_config_id AS model_config_id,
                COALESCE(task_counts.task_count, 0) AS task_count,
                COALESCE(run_counts.run_count, 0) AS run_count
            FROM model_ids
            LEFT JOIN task_counts ON task_counts.model_config_id = model_ids.model_config_id
            LEFT JOIN run_counts ON run_counts.model_config_id = model_ids.model_config_id
            ORDER BY model_ids.model_config_id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows
            .into_iter()
            .map(|row| ModelConfigUsageRecord {
                model_config_id: row.get("model_config_id"),
                task_count: row.get::<i64, _>("task_count") as usize,
                run_count: row.get::<i64, _>("run_count") as usize,
            })
            .collect())
    }
}
