// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RunService {
    pub(super) async fn active_run_for_task(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        Ok(self
            .store
            .list_runs(Some(task_id))
            .await?
            .into_iter()
            .find(|run| matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running)))
    }

    pub(super) async fn latest_successful_run(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        Ok(self
            .store
            .list_runs(Some(task_id))
            .await?
            .into_iter()
            .find(|run| run.status == TaskRunStatus::Succeeded))
    }
}
