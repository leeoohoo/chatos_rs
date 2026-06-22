use super::*;

impl SqliteStore {
    pub(in crate::store) async fn list_run_events(
        &self,
        run_id: &str,
    ) -> Result<Vec<TaskRunEventRecord>, String> {
        let rows = sqlx::query(
            "SELECT * FROM task_run_events WHERE run_id = ? ORDER BY datetime(created_at) ASC, id ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        rows.iter().map(task_run_event_from_row).collect()
    }

    pub(in crate::store) async fn append_run_event(
        &self,
        event: TaskRunEventRecord,
    ) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO task_run_events (id, run_id, event_type, message, payload_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&event.id)
        .bind(&event.run_id)
        .bind(&event.event_type)
        .bind(event.message.clone())
        .bind(encode_json_option(&event.payload)?)
        .bind(&event.created_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        let _ = self.run_event_sender.send(event);
        Ok(())
    }
}
