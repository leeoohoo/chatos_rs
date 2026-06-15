use super::*;

impl MongoStore {
    pub(in crate::store) async fn mark_cancel_requested(
        &self,
        run_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        let result = self
            .runs
            .update_one(
                doc! { "id": run_id },
                doc! {
                    "$set": {
                        "cancel_requested": true,
                        "updated_at": Utc::now().to_rfc3339(),
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        if result.matched_count == 0 {
            return Ok(None);
        }
        self.cancel_requested_runs
            .write()
            .insert(run_id.to_string());
        self.get_run(run_id).await
    }

    pub(in crate::store) fn clear_cancel_requested(&self, run_id: &str) {
        self.cancel_requested_runs.write().remove(run_id);
        let runs = self.runs.clone();
        let run_id = run_id.to_string();
        tokio::spawn(async move {
            if let Err(err) = runs
                .update_one(
                    doc! { "id": &run_id },
                    doc! {
                        "$set": {
                            "cancel_requested": false,
                            "updated_at": Utc::now().to_rfc3339(),
                        }
                    },
                    None,
                )
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
        let count = self
            .runs
            .count_documents(
                doc! {
                    "task_id": task_id,
                    "status": {
                        "$in": ["queued", "running"]
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(count > 0)
    }
}
