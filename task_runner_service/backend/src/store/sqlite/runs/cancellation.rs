use super::*;

impl SqliteStore {
    pub(in crate::store) async fn mark_cancel_requested(
        &self,
        run_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        self.cancel_requested_runs
            .write()
            .insert(run_id.to_string());
        sqlx::query(
            "UPDATE task_runs SET cancel_requested = 1, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        )
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        self.get_run(run_id).await
    }

    pub(in crate::store) fn clear_cancel_requested(&self, run_id: &str) {
        self.cancel_requested_runs.write().remove(run_id);
        let pool = self.pool.clone();
        let run_id = run_id.to_string();
        tokio::spawn(async move {
            if let Err(err) = sqlx::query("UPDATE task_runs SET cancel_requested = 0 WHERE id = ?")
                .bind(run_id)
                .execute(&pool)
                .await
            {
                warn!("failed to clear cancel_requested flag: {err}");
            }
        });
    }

    pub(in crate::store) fn is_cancel_requested(&self, run_id: &str) -> bool {
        self.cancel_requested_runs.read().contains(run_id)
    }

    pub(in crate::store) async fn has_active_run_for_task(
        &self,
        task_id: &str,
    ) -> Result<bool, String> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM task_runs WHERE task_id = ? AND status IN ('queued', 'running')",
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(count > 0)
    }
}
