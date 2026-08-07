// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl MongoStore {
    async fn active_execution_lane_keys(&self) -> Result<Vec<Bson>, String> {
        self.runs
            .distinct(
                "execution_lane_key",
                Some(doc! {
                    "status": "running",
                    "execution_lane_key": { "$type": "string" },
                }),
                None,
            )
            .await
            .map_err(|err| err.to_string())
    }

    async fn queued_run_claim_filter(&self) -> Result<Document, String> {
        let active_execution_lanes = self.active_execution_lane_keys().await?;
        let mut filter = doc! {
            "status": "queued",
            "dispatch_paused": { "$ne": true },
        };
        if !active_execution_lanes.is_empty() {
            filter.insert(
                "$or",
                vec![
                    doc! { "execution_lane_key": { "$exists": false } },
                    doc! { "execution_lane_key": null },
                    doc! { "execution_lane_key": { "$nin": active_execution_lanes } },
                ],
            );
        }
        Ok(filter)
    }

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
        let filter = self.queued_run_claim_filter().await?;
        match self
            .runs
            .find_one_and_update(
                filter,
                doc! {
                    "$set": {
                        "status": "running",
                        "dispatch_event_pending": false,
                        "worker_id": worker_id,
                        "claim_token": claim_token,
                        "claim_until": claim_until,
                        "updated_at": now.as_str(),
                    },
                    "$inc": { "attempt": 1_i64 },
                    "$unset": {
                        "finished_at": "",
                        "result_summary": "",
                        "error_message": "",
                    },
                },
                FindOneAndUpdateOptions::builder()
                    .sort(doc! { "created_at": 1, "id": 1 })
                    .return_document(ReturnDocument::After)
                    .build(),
            )
            .await
        {
            Ok(Some(mut run)) => {
                if run.started_at.is_none() {
                    let result = self
                        .runs
                        .update_one(
                            doc! {
                                "id": run.id.as_str(),
                                "claim_token": claim_token,
                            },
                            doc! {
                                "$set": {
                                    "started_at": now.as_str(),
                                }
                            },
                            None,
                        )
                        .await
                        .map_err(|err| err.to_string())?;
                    if result.modified_count == 0 {
                        return Err(lost_run_claim_error(&run.id));
                    }
                    run.started_at = Some(now);
                }
                Ok(Some(run))
            }
            Ok(None) => Ok(None),
            Err(err) if is_mongo_execution_lane_conflict(&err.to_string()) => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }

    pub(in crate::store) async fn has_queued_run_waiting_for_execution(
        &self,
    ) -> Result<bool, String> {
        self.runs
            .clone_with_type::<Document>()
            .find_one(
                doc! {
                    "status": "queued",
                    "dispatch_paused": { "$ne": true },
                },
                FindOneOptions::builder()
                    .projection(doc! { "_id": 1 })
                    .build(),
            )
            .await
            .map(|run| run.is_some())
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

    pub(in crate::store) async fn list_pending_run_dispatches(
        &self,
        limit: usize,
    ) -> Result<Vec<TaskRunRecord>, String> {
        self.runs
            .find(
                doc! {
                    "status": "queued",
                    "dispatch_paused": { "$ne": true },
                    "dispatch_event_pending": { "$ne": false },
                },
                FindOptions::builder()
                    .sort(doc! { "created_at": 1, "id": 1 })
                    .limit(i64::try_from(limit.max(1)).unwrap_or(i64::MAX))
                    .build(),
            )
            .await
            .map_err(|err| err.to_string())?
            .try_collect::<Vec<TaskRunRecord>>()
            .await
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn acknowledge_run_dispatch_event(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.runs
            .update_one(
                doc! {
                    "id": run_id,
                    "status": "queued",
                    "dispatch_event_pending": { "$ne": false },
                },
                doc! { "$set": { "dispatch_event_pending": false } },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn list_pending_run_post_processes(
        &self,
        limit: usize,
    ) -> Result<Vec<TaskRunRecord>, String> {
        self.runs
            .find(
                doc! {
                    "status": "succeeded",
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
                    "status": "succeeded",
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

    pub(in crate::store) async fn list_pending_terminal_cleanups(
        &self,
        limit: usize,
    ) -> Result<Vec<TaskRunRecord>, String> {
        self.runs
            .find(
                doc! {
                    "terminal_cleanup_event_pending": true,
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

    pub(in crate::store) async fn acknowledge_terminal_cleanup_event(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.runs
            .update_one(
                doc! {
                    "id": run_id,
                    "terminal_cleanup_event_pending": true,
                    "terminal_cleanup_completed": { "$ne": true },
                },
                doc! {
                    "$set": {
                        "terminal_cleanup_event_pending": false,
                        "terminal_cleanup_event_enqueued": true,
                        "updated_at": now_rfc3339(),
                    }
                },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn retry_terminal_cleanup(
        &self,
        run_id: &str,
        error: &str,
    ) -> Result<bool, String> {
        self.runs
            .update_one(
                doc! {
                    "id": run_id,
                    "terminal_cleanup_completed": { "$ne": true },
                },
                doc! {
                    "$set": {
                        "terminal_cleanup_event_pending": true,
                        "terminal_cleanup_event_enqueued": false,
                        "terminal_cleanup_last_error": error,
                        "updated_at": now_rfc3339(),
                    },
                    "$inc": { "terminal_cleanup_attempt_count": 1_i64 },
                },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
            .map_err(|err| err.to_string())
    }

    pub(in crate::store) async fn mark_terminal_cleanup_completed(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.runs
            .update_one(
                doc! { "id": run_id },
                doc! {
                    "$set": {
                        "terminal_cleanup_event_pending": false,
                        "terminal_cleanup_event_enqueued": false,
                        "terminal_cleanup_completed": true,
                        "updated_at": now_rfc3339(),
                    },
                    "$unset": { "terminal_cleanup_last_error": "" },
                },
                None,
            )
            .await
            .map(|result| result.modified_count > 0)
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

    pub(in crate::store) async fn reconcile_expired_run_claims(
        &self,
        expired_before: &str,
        reconciled_at: &str,
        max_attempts: i64,
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
        let mut reconciled_runs = Vec::new();
        for mut run in candidates {
            let was_cancel_requested = run.cancel_requested;
            let should_requeue = !was_cancel_requested && run.attempt < max_attempts.max(1);
            let (next_status, next_status_text, result_summary, error_message, callback_event) =
                if was_cancel_requested {
                    (
                        TaskRunStatus::Cancelled,
                        "cancelled",
                        "任务取消请求已生效；运行节点心跳过期后按取消收尾".to_string(),
                        None,
                        Some("task.cancelled"),
                    )
                } else if should_requeue {
                    (
                        TaskRunStatus::Queued,
                        "queued",
                        "任务运行节点中断，已自动重新排队恢复".to_string(),
                        None,
                        None,
                    )
                } else {
                    (
                        TaskRunStatus::Failed,
                        "failed",
                        format!("任务运行节点连续中断，达到 {max_attempts} 次尝试上限后标记为失败"),
                        Some("worker claim expired"),
                        Some("task.failed"),
                    )
                };
            let mut set_doc = doc! {
                "status": next_status_text,
                "dispatch_event_pending": should_requeue && !run.dispatch_paused,
                "updated_at": reconciled_at,
                "result_summary": result_summary.as_str(),
                "cancel_requested": false,
            };
            if let Some(callback_event) = callback_event {
                set_doc.insert("finished_at", reconciled_at);
                set_doc.insert(
                    "chatos_callback_delivery",
                    bson::to_bson(&ChatosCallbackDeliveryState {
                        event: callback_event.to_string(),
                        status: ChatosCallbackDeliveryStatus::Pending,
                        attempt_count: 0,
                        next_attempt_at: Some(reconciled_at.to_string()),
                        last_error: None,
                        updated_at: reconciled_at.to_string(),
                    })
                    .map_err(|err| err.to_string())?,
                );
            }
            if let Some(error_message) = error_message {
                set_doc.insert("error_message", error_message);
            }
            let mut unset_doc = doc! {
                "claim_token": "",
                "claim_until": "",
            };
            if should_requeue {
                unset_doc.insert("finished_at", "");
                unset_doc.insert("error_message", "");
                unset_doc.insert("usage", "");
                unset_doc.insert("report", "");
                unset_doc.insert("summary_job_run_id", "");
                unset_doc.insert("chatos_callback_delivery", "");
            } else if was_cancel_requested {
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
            run.status = next_status;
            run.finished_at = (!should_requeue).then(|| reconciled_at.to_string());
            run.updated_at = reconciled_at.to_string();
            run.result_summary = Some(result_summary);
            run.error_message = error_message.map(ToOwned::to_owned);
            run.cancel_requested = false;
            run.claim_token = None;
            run.claim_until = None;
            if should_requeue {
                run.usage = None;
                run.report = None;
                run.summary_job_run_id = None;
                run.chatos_callback_delivery = None;
            } else {
                ensure_terminal_callback_pending(&mut run);
            }
            self.sync_cancel_requested_cache(&run);
            reconciled_runs.push(run);
        }
        Ok(reconciled_runs)
    }
}
