// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoStore {
    pub(in crate::store) async fn save_run(
        &self,
        mut run: TaskRunRecord,
    ) -> Result<TaskRunRecord, String> {
        if let Some(claim_token) = run.claim_token.as_deref() {
            let mut filter = doc! {
                "id": &run.id,
                "claim_token": claim_token,
            };
            if let Some(worker_id) = run.worker_id.as_deref() {
                filter.insert("worker_id", worker_id);
            }
            let current = self
                .runs
                .find_one(filter.clone(), None)
                .await
                .map_err(|err| err.to_string())?
                .ok_or_else(|| lost_run_claim_error(&run.id))?;
            if current.cancel_requested {
                run.cancel_requested = true;
                run.cancel_event_pending |= current.cancel_event_pending;
            }
            let persisted = prepare_run_for_claim_guarded_persist(run.clone());
            let result = self
                .runs
                .replace_one(filter, &persisted, None)
                .await
                .map_err(|err| err.to_string())?;
            if result.matched_count == 0 {
                return Err(lost_run_claim_error(&run.id));
            }
            self.sync_cancel_requested_cache(&persisted);
            return Ok(persisted);
        }

        if let Some(current) = self
            .runs
            .find_one(doc! { "id": &run.id }, None)
            .await
            .map_err(|err| err.to_string())?
        {
            merge_run_async_progress(&mut run, &current);
        }
        let run = prepare_run_for_claim_guarded_persist(run);
        self.runs
            .replace_one(
                doc! { "id": &run.id },
                &run,
                ReplaceOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| {
                let message = err.to_string();
                if is_mongo_active_run_conflict(message.as_str()) {
                    "当前任务已有正在执行的运行".to_string()
                } else if is_mongo_execution_lane_conflict(message.as_str()) {
                    EXECUTION_LANE_BUSY_ERROR.to_string()
                } else {
                    message
                }
            })?;
        self.sync_cancel_requested_cache(&run);
        Ok(run)
    }

    fn sync_cancel_requested_cache(&self, run: &TaskRunRecord) {
        let mut cancel_requested_runs = self.cancel_requested_runs.write();
        if run.cancel_requested {
            cancel_requested_runs.insert(run.id.clone());
        } else {
            cancel_requested_runs.remove(&run.id);
        }
    }

    pub(in crate::store) async fn set_queued_runs_dispatch_paused(
        &self,
        task_ids: &[String],
        paused: bool,
    ) -> Result<u64, String> {
        if task_ids.is_empty() {
            return Ok(0);
        }
        self.runs
            .update_many(
                doc! {
                    "task_id": { "$in": task_ids },
                    "status": "queued",
                },
                doc! {
                    "$set": {
                        "dispatch_paused": paused,
                        "dispatch_event_pending": !paused,
                        "updated_at": Utc::now().to_rfc3339(),
                    }
                },
                None,
            )
            .await
            .map(|result| result.modified_count)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn list_pending_run_post_processes(
        &self,
        limit: usize,
    ) -> Result<Vec<TaskRunRecord>, String> {
        self.runs
            .find(
                doc! {
                    "$or": [
                        { "status": { "$in": ["succeeded", "failed", "cancelled", "blocked"] } },
                        { "workspace_execution.integration_status": { "$in": ["pending", "integrating", "failed"] } },
                    ],
                    "post_process_event_pending": true,
                    "post_process_dead_lettered": { "$ne": true },
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

    pub(in crate::store) async fn acknowledge_run_post_process_event(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.runs
            .update_one(
                doc! {
                    "id": run_id,
                    "post_process_event_pending": true,
                    "post_process_completed": { "$ne": true },
                    "post_process_dead_lettered": { "$ne": true },
                },
                doc! {
                    "$set": {
                        "post_process_event_pending": false,
                        "post_process_event_enqueued": true,
                        "updated_at": now_rfc3339(),
                    }
                },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn record_run_post_process_failure(
        &self,
        run_id: &str,
        error: &str,
    ) -> Result<bool, String> {
        self.runs
            .update_one(
                doc! {
                    "id": run_id,
                    "post_process_completed": { "$ne": true },
                    "post_process_dead_lettered": { "$ne": true },
                },
                doc! {
                    "$set": {
                        "post_process_last_error": error,
                        "updated_at": now_rfc3339(),
                    },
                    "$inc": { "post_process_attempt_count": 1_i64 },
                },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn mark_run_memory_summary_processed(
        &self,
        run_id: &str,
        summary_job_run_id: Option<&str>,
    ) -> Result<bool, String> {
        let mut set_doc = doc! {
            "memory_summary_processed": true,
            "updated_at": now_rfc3339(),
        };
        if let Some(summary_job_run_id) = summary_job_run_id {
            set_doc.insert("summary_job_run_id", summary_job_run_id);
        }
        self.runs
            .update_one(
                doc! {
                    "id": run_id,
                    "post_process_dead_lettered": { "$ne": true },
                },
                doc! { "$set": set_doc },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn mark_run_chatos_followup_processed(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.runs
            .update_one(
                doc! {
                    "id": run_id,
                    "post_process_dead_lettered": { "$ne": true },
                },
                doc! {
                    "$set": {
                        "chatos_followup_processed": true,
                        "updated_at": now_rfc3339(),
                    }
                },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn mark_run_post_process_completed(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.runs
            .update_one(
                doc! {
                    "id": run_id,
                    "post_process_dead_lettered": { "$ne": true },
                },
                doc! {
                    "$set": {
                        "post_process_event_pending": false,
                        "post_process_event_enqueued": false,
                        "post_process_completed": true,
                        "updated_at": now_rfc3339(),
                    },
                    "$unset": { "post_process_last_error": "" },
                },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn mark_run_post_process_dead_lettered(
        &self,
        run_id: &str,
        error: &str,
    ) -> Result<bool, String> {
        self.runs
            .update_one(
                doc! {
                    "id": run_id,
                    "post_process_completed": { "$ne": true },
                },
                doc! {
                    "$set": {
                        "post_process_event_pending": false,
                        "post_process_event_enqueued": false,
                        "post_process_dead_lettered": true,
                        "post_process_last_error": error,
                        "updated_at": now_rfc3339(),
                    }
                },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn rearm_run_post_process_dead_letter(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.runs
            .update_one(
                doc! {
                    "id": run_id,
                    "$or": [
                        { "status": { "$in": ["succeeded", "failed", "cancelled", "blocked"] } },
                        { "workspace_execution.integration_status": { "$in": ["pending", "integrating", "failed"] } },
                    ],
                    "post_process_completed": { "$ne": true },
                    "post_process_dead_lettered": true,
                },
                doc! {
                    "$set": {
                        "post_process_dead_lettered": false,
                        "post_process_attempt_count": 0_i64,
                        "post_process_event_pending": true,
                        "post_process_event_enqueued": false,
                        "updated_at": now_rfc3339(),
                    },
                    "$unset": { "post_process_last_error": "" },
                },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn rearm_run_workspace_integration(
        &self,
        run_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        let result = self
            .runs
            .update_one(
                doc! {
                    "id": run_id,
                    "status": "blocked",
                    "workspace_execution.integration_status": "conflict",
                },
                doc! {
                    "$set": {
                        "status": "running",
                        "workspace_execution.integration_status": "pending",
                        "post_process_event_pending": true,
                        "post_process_event_enqueued": false,
                        "post_process_completed": false,
                        "post_process_dead_lettered": false,
                        "post_process_attempt_count": 0_i64,
                        "memory_summary_processed": false,
                        "chatos_followup_processed": false,
                        "updated_at": now_rfc3339(),
                    },
                    "$unset": {
                        "finished_at": "",
                        "error_message": "",
                        "chatos_callback_delivery": "",
                        "post_process_last_error": "",
                        "workspace_execution.integration_started_at": "",
                        "workspace_execution.integrated_at": "",
                        "workspace_execution.conflict_files": "",
                        "workspace_execution.conflict_message": "",
                        "workspace_execution.integration_last_error": "",
                    },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        if result.modified_count == 0 {
            return Ok(None);
        }
        self.get_run(run_id).await
    }

    pub(in crate::store) async fn waive_run_workspace_integration(
        &self,
        run_id: &str,
        reason: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        let now = now_rfc3339();
        let result = self
            .runs
            .update_one(
                doc! {
                    "id": run_id,
                    "status": "blocked",
                    "workspace_execution.integration_status": "conflict",
                },
                doc! {
                    "$set": {
                        "status": "succeeded",
                        "finished_at": now.as_str(),
                        "workspace_execution.integration_status": "waived",
                        "workspace_execution.waived_at": now.as_str(),
                        "workspace_execution.waiver_reason": reason,
                        "post_process_event_pending": true,
                        "post_process_event_enqueued": false,
                        "post_process_completed": false,
                        "post_process_dead_lettered": false,
                        "post_process_attempt_count": 0_i64,
                        "memory_summary_processed": false,
                        "chatos_followup_processed": false,
                        "updated_at": now.as_str(),
                    },
                    "$unset": {
                        "error_message": "",
                        "chatos_callback_delivery": "",
                        "post_process_last_error": "",
                        "workspace_execution.integration_last_error": "",
                    },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        if result.modified_count == 0 {
            return Ok(None);
        }
        self.get_run(run_id).await
    }
}
