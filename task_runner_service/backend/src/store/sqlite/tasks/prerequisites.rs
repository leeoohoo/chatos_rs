use super::*;

impl SqliteStore {
    pub(in crate::store) async fn list_task_prerequisites(
        &self,
        task_id: &str,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        let rows = sqlx::query(
            "SELECT task_id, prerequisite_task_id, created_at
             FROM task_prerequisites
             WHERE task_id = ?
             ORDER BY datetime(created_at) ASC, prerequisite_task_id ASC",
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;

        Ok(rows
            .iter()
            .map(|row| TaskPrerequisiteRecord {
                task_id: row.get("task_id"),
                prerequisite_task_id: row.get("prerequisite_task_id"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    pub(in crate::store) async fn list_task_dependents(
        &self,
        prerequisite_task_id: &str,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        let rows = sqlx::query(
            "SELECT task_id, prerequisite_task_id, created_at
             FROM task_prerequisites
             WHERE prerequisite_task_id = ?
             ORDER BY datetime(created_at) ASC, task_id ASC",
        )
        .bind(prerequisite_task_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;

        Ok(rows
            .iter()
            .map(|row| TaskPrerequisiteRecord {
                task_id: row.get("task_id"),
                prerequisite_task_id: row.get("prerequisite_task_id"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    pub(in crate::store) async fn set_task_prerequisites(
        &self,
        task_id: &str,
        prerequisite_task_ids: Vec<String>,
    ) -> Result<Vec<TaskPrerequisiteRecord>, String> {
        let mut tx = self.pool.begin().await.map_err(|err| err.to_string())?;
        sqlx::query("DELETE FROM task_prerequisites WHERE task_id = ?")
            .bind(task_id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;

        let now = now_rfc3339();
        for prerequisite_task_id in prerequisite_task_ids {
            sqlx::query(
                "INSERT OR IGNORE INTO task_prerequisites
                 (task_id, prerequisite_task_id, created_at)
                 VALUES (?, ?, ?)",
            )
            .bind(task_id)
            .bind(prerequisite_task_id)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        }
        tx.commit().await.map_err(|err| err.to_string())?;
        self.list_task_prerequisites(task_id).await
    }
}
