// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RunService {
    pub(super) async fn wait_for_run_terminal(
        &self,
        run_id: &str,
        parent_run: &TaskRunRecord,
    ) -> Result<TaskRunRecord, String> {
        let timeout = self.effective_execution_timeout().await? + Duration::from_secs(30);
        let worker_id = parent_run.worker_id.as_deref().ok_or_else(|| {
            format!(
                "父 Run {} 缺少 Worker 标识，不能订阅前置 Run 终态事件",
                parent_run.id
            )
        })?;
        let terminal_token = tokio_util::sync::CancellationToken::new();
        let parent_cancel_token = tokio_util::sync::CancellationToken::new();
        self.register_run_terminal_waiter(run_id, parent_run.id.as_str(), terminal_token.clone());
        self.register_runtime_abort_token(parent_run.id.as_str(), parent_cancel_token.clone());
        let subscription = crate::store::RunTerminalSubscriptionRecord::new(
            run_id,
            parent_run.id.as_str(),
            worker_id,
        );

        let result = async {
            let current = self
                .store
                .subscribe_run_terminal(subscription.clone())
                .await?;
            if is_terminal_run_status(current.status) {
                return Ok(current);
            }

            tokio::select! {
                _ = terminal_token.cancelled() => {
                    let run = self
                        .store
                        .get_run(run_id)
                        .await?
                        .ok_or_else(|| format!("运行不存在: {run_id}"))?;
                    if !is_terminal_run_status(run.status) {
                        return Err(format!("前置 Run 终态事件与持久化状态不一致: {run_id}"));
                    }
                    Ok(run)
                }
                _ = parent_cancel_token.cancelled() => {
                    Err("当前任务已请求取消，停止等待前置任务".to_string())
                }
                _ = tokio::time::sleep(timeout) => {
                    Err(format!("等待前置任务运行超时: {run_id}"))
                }
            }
        }
        .await;

        self.unregister_run_terminal_waiter(run_id, parent_run.id.as_str());
        self.unregister_runtime_abort_token(parent_run.id.as_str());
        if let Err(err) = self
            .store
            .acknowledge_run_terminal_subscription(subscription.id.as_str())
            .await
        {
            warn!(
                run_id,
                parent_run_id = parent_run.id.as_str(),
                error = err.as_str(),
                "failed to clean dependency run terminal subscription"
            );
        }
        result
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
