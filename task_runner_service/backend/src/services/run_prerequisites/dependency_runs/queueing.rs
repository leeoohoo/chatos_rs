// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
impl RunService {
    pub(super) async fn queue_dependency_run(
        &self,
        task: TaskRecord,
        input: StartTaskRunRequest,
    ) -> Result<TaskRunRecord, String> {
        let task = save_task_if_tenant_aligned(&self.store, task).await?;
        if task.status == TaskStatus::Cancelled {
            return Err(format!("前置任务已取消，不能执行: {}", task.id));
        }
        if self.store.has_active_run_for_task(task.id.as_str()).await? {
            return self
                .active_run_for_task(task.id.as_str())
                .await?
                .ok_or_else(|| "前置任务已有运行中记录，但读取失败".to_string());
        }
        self.start_dependency_run(task.id.as_str(), input).await
    }
}
