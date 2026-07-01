// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl SqliteStore {
    pub(in crate::store) async fn save_run(
        &self,
        run: TaskRunRecord,
    ) -> Result<TaskRunRecord, String> {
        if run.cancel_requested {
            self.cancel_requested_runs.write().insert(run.id.clone());
        } else {
            self.cancel_requested_runs.write().remove(&run.id);
        }
        sqlx::query(
            "INSERT INTO task_runs (
                id, task_id, model_config_id, memory_thread_id, status, started_at, finished_at,
                input_snapshot_json, context_snapshot_json, result_summary, error_message,
                usage_json, report_json, cancel_requested, summary_job_run_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        .bind(&run.created_at)
        .bind(&run.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(run)
    }
}
