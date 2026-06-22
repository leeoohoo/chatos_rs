use super::*;

impl RunService {
    pub(super) async fn wait_for_run_terminal(
        &self,
        run_id: &str,
        parent_run_id: &str,
    ) -> Result<TaskRunRecord, String> {
        let timeout = self.effective_execution_timeout().await? + Duration::from_secs(30);
        let started = Instant::now();
        loop {
            let run = self
                .store
                .get_run(run_id)
                .await?
                .ok_or_else(|| format!("运行不存在: {run_id}"))?;
            if is_terminal_run_status(run.status) {
                return Ok(run);
            }
            if self.store.is_cancel_requested(parent_run_id) {
                return Err("当前任务已请求取消，停止等待前置任务".to_string());
            }
            if started.elapsed() > timeout {
                return Err(format!("等待前置任务运行超时: {run_id}"));
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

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
