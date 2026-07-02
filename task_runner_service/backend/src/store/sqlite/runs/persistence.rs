// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl SqliteStore {
    pub(in crate::store) async fn save_run(
        &self,
        run: TaskRunRecord,
    ) -> Result<TaskRunRecord, String> {
        if let Some(claim_token) = run.claim_token.as_deref() {
            let persisted = prepare_run_for_claim_guarded_persist(run.clone());
            let sql = if run.worker_id.is_some() {
                "UPDATE task_runs SET
                    task_id = ?,
                    model_config_id = ?,
                    memory_thread_id = ?,
                    status = ?,
                    started_at = ?,
                    finished_at = ?,
                    input_snapshot_json = ?,
                    context_snapshot_json = ?,
                    result_summary = ?,
                    error_message = ?,
                    usage_json = ?,
                    report_json = ?,
                    cancel_requested = ?,
                    summary_job_run_id = ?,
                    worker_id = ?,
                    claim_token = ?,
                    claim_until = ?,
                    attempt = ?,
                    created_at = ?,
                    updated_at = ?
                 WHERE id = ? AND claim_token = ? AND worker_id = ?"
            } else {
                "UPDATE task_runs SET
                    task_id = ?,
                    model_config_id = ?,
                    memory_thread_id = ?,
                    status = ?,
                    started_at = ?,
                    finished_at = ?,
                    input_snapshot_json = ?,
                    context_snapshot_json = ?,
                    result_summary = ?,
                    error_message = ?,
                    usage_json = ?,
                    report_json = ?,
                    cancel_requested = ?,
                    summary_job_run_id = ?,
                    worker_id = ?,
                    claim_token = ?,
                    claim_until = ?,
                    attempt = ?,
                    created_at = ?,
                    updated_at = ?
                 WHERE id = ? AND claim_token = ? AND worker_id IS NULL"
            };
            let mut query = sqlx::query(sql)
                .bind(&persisted.task_id)
                .bind(&persisted.model_config_id)
                .bind(&persisted.memory_thread_id)
                .bind(task_run_status_to_str(persisted.status))
                .bind(persisted.started_at.clone())
                .bind(persisted.finished_at.clone())
                .bind(encode_json(&persisted.input_snapshot)?)
                .bind(encode_json_option(&persisted.context_snapshot)?)
                .bind(persisted.result_summary.clone())
                .bind(persisted.error_message.clone())
                .bind(encode_json_option(&persisted.usage)?)
                .bind(encode_json_option(&persisted.report)?)
                .bind(bool_to_int(persisted.cancel_requested))
                .bind(persisted.summary_job_run_id.clone())
                .bind(persisted.worker_id.clone())
                .bind(persisted.claim_token.clone())
                .bind(persisted.claim_until.clone())
                .bind(persisted.attempt)
                .bind(&persisted.created_at)
                .bind(&persisted.updated_at)
                .bind(&run.id)
                .bind(claim_token);
            if let Some(worker_id) = run.worker_id.as_deref() {
                query = query.bind(worker_id);
            }
            let result = query
                .execute(&self.pool)
                .await
                .map_err(|err| err.to_string())?;
            if result.rows_affected() == 0 {
                return Err(lost_run_claim_error(&run.id));
            }
            self.sync_cancel_requested_cache(&persisted);
            return Ok(persisted);
        }

        sqlx::query(
            "INSERT INTO task_runs (
                id, task_id, model_config_id, memory_thread_id, status, started_at, finished_at,
                input_snapshot_json, context_snapshot_json, result_summary, error_message,
                usage_json, report_json, cancel_requested, summary_job_run_id, worker_id,
                claim_token, claim_until, attempt, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                task_id = excluded.task_id,
                model_config_id = excluded.model_config_id,
                memory_thread_id = excluded.memory_thread_id,
                status = excluded.status,
                started_at = excluded.started_at,
                finished_at = excluded.finished_at,
                input_snapshot_json = excluded.input_snapshot_json,
                context_snapshot_json = excluded.context_snapshot_json,
                result_summary = excluded.result_summary,
                error_message = excluded.error_message,
                usage_json = excluded.usage_json,
                report_json = excluded.report_json,
                cancel_requested = excluded.cancel_requested,
                summary_job_run_id = excluded.summary_job_run_id,
                worker_id = excluded.worker_id,
                claim_token = excluded.claim_token,
                claim_until = excluded.claim_until,
                attempt = excluded.attempt,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
        )
        .bind(&run.id)
        .bind(&run.task_id)
        .bind(&run.model_config_id)
        .bind(&run.memory_thread_id)
        .bind(task_run_status_to_str(run.status))
        .bind(run.started_at.clone())
        .bind(run.finished_at.clone())
        .bind(encode_json(&run.input_snapshot)?)
        .bind(encode_json_option(&run.context_snapshot)?)
        .bind(run.result_summary.clone())
        .bind(run.error_message.clone())
        .bind(encode_json_option(&run.usage)?)
        .bind(encode_json_option(&run.report)?)
        .bind(bool_to_int(run.cancel_requested))
        .bind(run.summary_job_run_id.clone())
        .bind(run.worker_id.clone())
        .bind(run.claim_token.clone())
        .bind(run.claim_until.clone())
        .bind(run.attempt)
        .bind(&run.created_at)
        .bind(&run.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        self.sync_cancel_requested_cache(&run);
        Ok(run)
    }

    fn sync_cancel_requested_cache(&self, run: &TaskRunRecord) {
        if run.cancel_requested {
            self.cancel_requested_runs.write().insert(run.id.clone());
        } else {
            self.cancel_requested_runs.write().remove(&run.id);
        }
    }

    pub(in crate::store) async fn claim_next_queued_run(
        &self,
        worker_id: &str,
        claim_token: &str,
        claim_until: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        let now = now_rfc3339();
        let result = sqlx::query(
            "UPDATE task_runs
             SET status = 'running',
                 worker_id = ?,
                 claim_token = ?,
                 claim_until = ?,
                 attempt = attempt + 1,
                 started_at = COALESCE(started_at, ?),
                 updated_at = ?
             WHERE id = (
                 SELECT id FROM task_runs
                 WHERE status = 'queued'
                 ORDER BY datetime(created_at) ASC, id ASC
                 LIMIT 1
             )",
        )
        .bind(worker_id)
        .bind(claim_token)
        .bind(claim_until)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        let row = sqlx::query("SELECT * FROM task_runs WHERE claim_token = ?")
            .bind(claim_token)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(task_run_from_row).transpose()
    }

    pub(in crate::store) async fn renew_run_claim(
        &self,
        run_id: &str,
        worker_id: &str,
        claim_token: &str,
        claim_until: &str,
    ) -> Result<bool, String> {
        let result = sqlx::query(
            "UPDATE task_runs
             SET claim_until = ?, updated_at = ?
             WHERE id = ?
               AND status = 'running'
               AND worker_id = ?
               AND claim_token = ?",
        )
        .bind(claim_until)
        .bind(now_rfc3339())
        .bind(run_id)
        .bind(worker_id)
        .bind(claim_token)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }

    pub(in crate::store) async fn fail_expired_run_claims(
        &self,
        now: &str,
    ) -> Result<usize, String> {
        let result = sqlx::query(
            "UPDATE task_runs
             SET status = 'failed',
                 finished_at = ?,
                 updated_at = ?,
                 result_summary = '任务运行节点心跳过期，已标记为失败',
                 error_message = 'worker claim expired',
                 cancel_requested = 0,
                 claim_token = NULL,
                 claim_until = NULL
             WHERE status = 'running'
               AND claim_until IS NOT NULL
               AND claim_until <= ?",
        )
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(result.rows_affected() as usize)
    }
}
