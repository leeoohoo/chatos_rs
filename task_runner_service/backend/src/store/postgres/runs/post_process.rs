// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::WorkspaceIntegrationStatus;

impl PostgresStore {
    pub(in crate::store) async fn list_pending_run_post_processes(
        &self,
        limit: usize,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
            "SELECT data FROM task_runs WHERE post_process_event_pending AND NOT post_process_dead_lettered \
             ORDER BY updated_at,id LIMIT $1",
        )
        .bind(i64::try_from(limit.max(1)).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;
        let runs = rows
            .into_iter()
            .map(decode_json)
            .collect::<Result<Vec<TaskRunRecord>, _>>()?;
        Ok(runs
            .into_iter()
            .filter(TaskRunRecord::requires_post_process)
            .collect())
    }

    pub(in crate::store) async fn acknowledge_run_post_process_event(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.mutate_run(run_id, |run| {
            if !run.post_process_event_pending
                || run.post_process_completed
                || run.post_process_dead_lettered
            {
                return false;
            }
            run.post_process_event_pending = false;
            run.post_process_event_enqueued = true;
            run.updated_at = now_rfc3339();
            true
        })
        .await
        .map(|run| run.is_some())
    }

    pub(in crate::store) async fn record_run_post_process_failure(
        &self,
        run_id: &str,
        error: &str,
    ) -> Result<bool, String> {
        self.mutate_run(run_id, |run| {
            if run.post_process_completed || run.post_process_dead_lettered {
                return false;
            }
            run.post_process_attempt_count = run.post_process_attempt_count.saturating_add(1);
            run.post_process_last_error = Some(error.to_string());
            run.updated_at = now_rfc3339();
            true
        })
        .await
        .map(|run| run.is_some())
    }

    pub(in crate::store) async fn mark_run_chatos_followup_processed(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.mutate_run(run_id, |run| {
            if run.post_process_dead_lettered {
                return false;
            }
            run.chatos_followup_processed = true;
            run.updated_at = now_rfc3339();
            true
        })
        .await
        .map(|run| run.is_some())
    }

    pub(in crate::store) async fn mark_run_post_process_completed(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.mutate_run(run_id, |run| {
            if run.post_process_dead_lettered {
                return false;
            }
            run.post_process_event_pending = false;
            run.post_process_event_enqueued = false;
            run.post_process_completed = true;
            run.post_process_last_error = None;
            run.updated_at = now_rfc3339();
            true
        })
        .await
        .map(|run| run.is_some())
    }

    pub(in crate::store) async fn mark_run_post_process_dead_lettered(
        &self,
        run_id: &str,
        error: &str,
    ) -> Result<bool, String> {
        self.mutate_run(run_id, |run| {
            if run.post_process_completed {
                return false;
            }
            run.post_process_event_pending = false;
            run.post_process_event_enqueued = false;
            run.post_process_dead_lettered = true;
            run.post_process_last_error = Some(error.to_string());
            run.updated_at = now_rfc3339();
            true
        })
        .await
        .map(|run| run.is_some())
    }

    pub(in crate::store) async fn rearm_run_post_process_dead_letter(
        &self,
        run_id: &str,
    ) -> Result<bool, String> {
        self.mutate_run(run_id, |run| {
            if !run.requires_post_process()
                || run.post_process_completed
                || !run.post_process_dead_lettered
            {
                return false;
            }
            run.post_process_dead_lettered = false;
            run.post_process_attempt_count = 0;
            run.post_process_event_pending = true;
            run.post_process_event_enqueued = false;
            run.post_process_last_error = None;
            run.updated_at = now_rfc3339();
            true
        })
        .await
        .map(|run| run.is_some())
    }

    pub(in crate::store) async fn rearm_run_workspace_integration(
        &self,
        run_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        self.mutate_run(run_id, |run| {
            let Some(execution) = run.workspace_execution.as_mut() else {
                return false;
            };
            if run.status != TaskRunStatus::Blocked
                || execution.integration_status != WorkspaceIntegrationStatus::Conflict
            {
                return false;
            }
            run.status = TaskRunStatus::Running;
            run.finished_at = None;
            run.error_message = None;
            run.chatos_callback_delivery = None;
            run.post_process_event_pending = true;
            run.post_process_event_enqueued = false;
            run.post_process_completed = false;
            run.post_process_dead_lettered = false;
            run.post_process_attempt_count = 0;
            run.post_process_last_error = None;
            run.chatos_followup_processed = false;
            execution.integration_status = WorkspaceIntegrationStatus::Pending;
            execution.integration_started_at = None;
            execution.integrated_at = None;
            execution.conflict_files.clear();
            execution.conflict_message = None;
            execution.integration_last_error = None;
            run.updated_at = now_rfc3339();
            true
        })
        .await
    }

    pub(in crate::store) async fn waive_run_workspace_integration(
        &self,
        run_id: &str,
        reason: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        self.mutate_run(run_id, |run| {
            let Some(execution) = run.workspace_execution.as_mut() else {
                return false;
            };
            if run.status != TaskRunStatus::Blocked
                || execution.integration_status != WorkspaceIntegrationStatus::Conflict
            {
                return false;
            }
            let now = now_rfc3339();
            run.status = TaskRunStatus::Succeeded;
            run.finished_at = Some(now.clone());
            run.error_message = None;
            run.chatos_callback_delivery = None;
            run.post_process_event_pending = true;
            run.post_process_event_enqueued = false;
            run.post_process_completed = false;
            run.post_process_dead_lettered = false;
            run.post_process_attempt_count = 0;
            run.post_process_last_error = None;
            run.chatos_followup_processed = false;
            execution.integration_status = WorkspaceIntegrationStatus::Waived;
            execution.waived_at = Some(now.clone());
            execution.waiver_reason = Some(reason.to_string());
            execution.integration_last_error = None;
            run.updated_at = now;
            true
        })
        .await
    }
}
