// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoStore {
    pub(in crate::store) async fn save_run(
        &self,
        run: TaskRunRecord,
    ) -> Result<TaskRunRecord, String> {
        if let Some(claim_token) = run.claim_token.as_deref() {
            let persisted = prepare_run_for_claim_guarded_persist(run.clone());
            let mut filter = doc! {
                "id": &run.id,
                "claim_token": claim_token,
            };
            if let Some(worker_id) = run.worker_id.as_deref() {
                filter.insert("worker_id", worker_id);
            }
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

        let run = prepare_run_for_claim_guarded_persist(run);
        self.runs
            .replace_one(
                doc! { "id": &run.id },
                &run,
                ReplaceOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| {
                if is_mongo_active_run_conflict(&err.to_string()) {
                    "当前任务已有正在执行的运行".to_string()
                } else {
                    err.to_string()
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

    pub(in crate::store) async fn claim_next_queued_run(
        &self,
        worker_id: &str,
        claim_token: &str,
        claim_until: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        let now = Utc::now().to_rfc3339();
        self.runs
            .find_one_and_update(
                doc! {
                    "status": "queued",
                    "dispatch_paused": { "$ne": true },
                },
                doc! {
                    "$set": {
                        "status": "running",
                        "worker_id": worker_id,
                        "claim_token": claim_token,
                        "claim_until": claim_until,
                        "started_at": now.as_str(),
                        "updated_at": now.as_str(),
                    },
                    "$inc": { "attempt": 1_i64 },
                },
                FindOneAndUpdateOptions::builder()
                    .sort(doc! { "created_at": 1, "id": 1 })
                    .return_document(ReturnDocument::After)
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())
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
                        "updated_at": Utc::now().to_rfc3339(),
                    }
                },
                None,
            )
            .await
            .map(|result| result.modified_count)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn renew_run_claim(
        &self,
        run_id: &str,
        worker_id: &str,
        claim_token: &str,
        claim_until: &str,
    ) -> Result<bool, String> {
        let result = self
            .runs
            .update_one(
                doc! {
                    "id": run_id,
                    "status": "running",
                    "worker_id": worker_id,
                    "claim_token": claim_token,
                },
                doc! {
                    "$set": {
                        "claim_until": claim_until,
                        "updated_at": Utc::now().to_rfc3339(),
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.matched_count > 0)
    }

    pub(in crate::store) async fn fail_expired_run_claims(
        &self,
        expired_before: &str,
        failed_at: &str,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let candidates = self
            .runs
            .find(
                doc! {
                    "status": "running",
                    "claim_until": { "$lte": expired_before },
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect::<Vec<TaskRunRecord>>()
            .await
            .map_err(|err| err.to_string())?;
        let mut failed_runs = Vec::new();
        for mut run in candidates {
            let was_cancel_requested = run.cancel_requested;
            let (
                terminal_status,
                terminal_status_text,
                result_summary,
                error_message,
                callback_event,
            ) = if was_cancel_requested {
                (
                    TaskRunStatus::Cancelled,
                    "cancelled",
                    "任务取消请求已生效；运行节点心跳过期后按取消收尾",
                    None,
                    "task.cancelled",
                )
            } else {
                (
                    TaskRunStatus::Failed,
                    "failed",
                    "任务运行节点心跳过期，已标记为失败",
                    Some("worker claim expired"),
                    "task.failed",
                )
            };
            let mut set_doc = doc! {
                "status": terminal_status_text,
                "finished_at": failed_at,
                "updated_at": failed_at,
                "result_summary": result_summary,
                "cancel_requested": false,
                "chatos_callback_delivery": bson::to_bson(&ChatosCallbackDeliveryState {
                    event: callback_event.to_string(),
                    status: ChatosCallbackDeliveryStatus::Pending,
                    attempt_count: 0,
                    next_attempt_at: Some(failed_at.to_string()),
                    last_error: None,
                    updated_at: failed_at.to_string(),
                }).map_err(|err| err.to_string())?,
            };
            if let Some(error_message) = error_message {
                set_doc.insert("error_message", error_message);
            }
            let mut unset_doc = doc! {
                "claim_token": "",
                "claim_until": "",
            };
            if was_cancel_requested {
                unset_doc.insert("error_message", "");
            }
            let result = self
                .runs
                .update_one(
                    doc! {
                        "id": run.id.as_str(),
                        "status": "running",
                        "claim_until": { "$lte": expired_before },
                    },
                    doc! {
                        "$set": set_doc,
                        "$unset": unset_doc,
                    },
                    None,
                )
                .await
                .map_err(|err| err.to_string())?;
            if result.modified_count == 0 {
                continue;
            }
            run.status = terminal_status;
            run.finished_at = Some(failed_at.to_string());
            run.updated_at = failed_at.to_string();
            run.result_summary = Some(result_summary.to_string());
            run.error_message = error_message.map(ToOwned::to_owned);
            run.cancel_requested = false;
            run.claim_token = None;
            run.claim_until = None;
            ensure_terminal_callback_pending(&mut run);
            self.sync_cancel_requested_cache(&run);
            failed_runs.push(run);
        }
        Ok(failed_runs)
    }
}
