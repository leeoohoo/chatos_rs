// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RunService {
    pub(super) async fn execute_run(
        &self,
        task: TaskRecord,
        model_config: ModelConfigRecord,
        mut run: TaskRunRecord,
        input: StartTaskRunRequest,
        effective_workspace_dir: String,
    ) {
        let prerequisite_context =
            match self.prepare_prerequisite_context(&task, &run, &input).await {
                Ok(context) => context,
                Err(err) => {
                    self.finish_blocked_by_prerequisite(
                        &task,
                        &mut run,
                        effective_workspace_dir.as_str(),
                        err,
                    )
                    .await;
                    return;
                }
            };
        self.execute_run_model_phase(
            task,
            model_config,
            run,
            input,
            effective_workspace_dir,
            prerequisite_context,
        )
        .await;
    }

    pub(in crate::services) async fn ensure_task_thread(
        &self,
        task: &TaskRecord,
    ) -> Result<(), String> {
        ensure_task_thread_for_config(&self.config, task).await
    }
}
