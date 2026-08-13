// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoStore {
    pub(in crate::store) async fn repair_stale_cancel_requested_runs(&self) -> Result<u64, String> {
        self.runs
            .update_many(
                doc! {
                    "cancel_requested": true,
                    "status": { "$nin": ["queued", "running"] },
                },
                doc! {
                    "$set": {
                        "cancel_requested": false,
                        "cancel_event_pending": false,
                        "updated_at": Utc::now().to_rfc3339(),
                    }
                },
                None,
            )
            .await
            .map(|result| result.modified_count)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn mark_cancel_requested(
        &self,
        run_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        let Some(current) = self.get_run(run_id).await? else {
            return Ok(None);
        };
        let cancel_event_pending =
            current.status == TaskRunStatus::Running && current.worker_id.is_some();
        let result = self
            .runs
            .update_one(
                doc! { "id": run_id },
                doc! {
                    "$set": {
                        "cancel_requested": true,
                        "cancel_event_pending": cancel_event_pending,
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

    pub(in crate::store) async fn list_pending_run_cancel_events(
        &self,
        limit: usize,
    ) -> Result<Vec<TaskRunRecord>, String> {
        self.runs
            .find(
                doc! {
                    "status": "running",
                    "cancel_requested": true,
                    "cancel_event_pending": true,
                    "worker_id": { "$type": "string" },
                },
                FindOptions::builder()
                    .sort(doc! { "updated_at": 1, "id": 1 })
                    .limit(i64::try_from(limit.max(1)).unwrap_or(i64::MAX))
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect::<Vec<TaskRunRecord>>()
            .await
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn acknowledge_run_cancel_event(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.runs
            .update_one(
                doc! { "id": run_id, "cancel_event_pending": true },
                doc! { "$set": { "cancel_event_pending": false } },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|err| err.to_string())
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
                            "cancel_event_pending": false,
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

    pub(in crate::store) fn signal_local_run_abort(&self, run_id: &str) {
        self.cancel_requested_runs
            .write()
            .insert(run_id.to_string());
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
