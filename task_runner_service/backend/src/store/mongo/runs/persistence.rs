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
                doc! { "status": "queued" },
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
        now: &str,
    ) -> Result<usize, String> {
        let result = self
            .runs
            .update_many(
                doc! {
                    "status": "running",
                    "claim_until": { "$lte": now },
                },
                doc! {
                    "$set": {
                        "status": "failed",
                        "finished_at": now,
                        "updated_at": now,
                        "result_summary": "任务运行节点心跳过期，已标记为失败",
                        "error_message": "worker claim expired",
                        "cancel_requested": false,
                    },
                    "$unset": {
                        "claim_token": "",
                        "claim_until": "",
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.modified_count as usize)
    }
}
