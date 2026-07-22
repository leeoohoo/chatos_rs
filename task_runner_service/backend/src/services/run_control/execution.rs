// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::services::TaskRunnerCapabilityPolicy;

impl RunService {
    pub async fn execute_claimed_run(&self, mut run: TaskRunRecord) {
        let task = match self.store.get_task(&run.task_id).await {
            Ok(Some(task)) => task,
            Ok(None) => {
                let task_id = run.task_id.clone();
                self.finish_claimed_run_without_task(
                    &mut run,
                    format!("task not found: {task_id}"),
                )
                .await;
                return;
            }
            Err(err) => {
                self.finish_claimed_run_without_task(&mut run, err).await;
                return;
            }
        };
        let task = match save_task_if_tenant_aligned(&self.store, task).await {
            Ok(task) => task,
            Err(err) => {
                self.finish_claimed_run_without_task(&mut run, err).await;
                return;
            }
        };
        let model_config = match self.store.get_model_config(&run.model_config_id).await {
            Ok(Some(model_config)) => model_config,
            Ok(None) => {
                let model_config_id = run.model_config_id.clone();
                self.finish_failed_before_execution(
                    &task,
                    &mut run,
                    ".",
                    format!("model config not found: {model_config_id}"),
                )
                .await;
                return;
            }
            Err(err) => {
                self.finish_failed_before_execution(&task, &mut run, ".", err)
                    .await;
                return;
            }
        };
        if !model_config.enabled {
            self.finish_failed_before_execution(
                &task,
                &mut run,
                ".",
                format!("model config is disabled: {}", model_config.id),
            )
            .await;
            return;
        }
        let capability_policy = match self.resolve_task_runner_policy_for_task(&task).await {
            Ok(Some(policy)) => policy,
            Ok(None) => {
                self.finish_failed_before_execution(
                    &task,
                    &mut run,
                    ".",
                    "Plugin Management capability configuration is required before Task Runner Agent execution"
                        .to_string(),
                )
                .await;
                return;
            }
            Err(err) => {
                self.finish_failed_before_execution(&task, &mut run, ".", err)
                    .await;
                return;
            }
        };
        let mut task = task;
        if let Err(err) = capability_policy.apply_to_task(&mut task) {
            self.finish_failed_before_execution(&task, &mut run, ".", err)
                .await;
            return;
        }
        let routed_task =
            task_with_runtime_mcp_routing_authoritative(&self.config, &self.store, task.clone())
                .await;
        let task = match routed_task {
            Ok(task) => task,
            Err(err) => {
                self.finish_failed_before_execution(&task, &mut run, ".", err)
                    .await;
                return;
            }
        };

        let input = StartTaskRunRequest {
            model_config_id: Some(run.model_config_id.clone()),
            prompt_override: run
                .input_snapshot
                .get("prompt_override")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
        };
        let effective_workspace_dir = run
            .input_snapshot
            .get("effective_workspace_dir")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                ensure_effective_task_workspace_dir(&self.config, &task, &model_config).ok()
            })
            .unwrap_or_else(|| self.config.default_workspace_dir.clone());

        self.execute_run(
            task,
            model_config,
            run,
            input,
            effective_workspace_dir,
            Some(capability_policy),
        )
        .await;
    }

    pub(super) async fn execute_run(
        &self,
        task: TaskRecord,
        model_config: ModelConfigRecord,
        mut run: TaskRunRecord,
        input: StartTaskRunRequest,
        effective_workspace_dir: String,
        capability_policy: Option<TaskRunnerCapabilityPolicy>,
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
            capability_policy,
        )
        .await;
    }

    pub(in crate::services) async fn ensure_task_thread(
        &self,
        task: &TaskRecord,
    ) -> Result<(), String> {
        ensure_task_thread_for_config(&self.config, task).await
    }

    async fn finish_claimed_run_without_task(&self, run: &mut TaskRunRecord, message: String) {
        run.status = TaskRunStatus::Failed;
        run.finished_at = Some(now_rfc3339());
        run.updated_at = now_rfc3339();
        run.result_summary = Some(message.clone());
        run.error_message = Some(message.clone());
        run.cancel_requested = false;
        match self.store.save_run(run.clone()).await {
            Ok(saved) => {
                *run = saved;
            }
            Err(err) => {
                warn!("failed to persist failed claimed run {}: {}", run.id, err);
                return;
            }
        }
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "failed",
                Some(message),
                None,
            ))
            .await
        {
            warn!("failed to append failed event for run {}: {}", run.id, err);
        }
    }
}
