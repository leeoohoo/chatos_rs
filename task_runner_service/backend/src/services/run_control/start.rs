// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::auth::CurrentUser;

impl RunService {
    pub async fn start_run(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
    ) -> Result<TaskRunRecord, String> {
        self.start_run_with_user(task_id, input, None).await
    }

    pub async fn start_run_for_user(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
        current_user: &CurrentUser,
    ) -> Result<TaskRunRecord, String> {
        self.start_run_with_user(task_id, input, Some(current_user))
            .await
    }

    async fn start_run_with_user(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
        current_user: Option<&CurrentUser>,
    ) -> Result<TaskRunRecord, String> {
        self.start_run_with_trigger(task_id, input, RunTriggerSource::Manual, current_user)
            .await
    }

    pub async fn start_scheduled_run(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
    ) -> Result<TaskRunRecord, String> {
        self.start_run_with_trigger(task_id, input, RunTriggerSource::Scheduler, None)
            .await
    }

    pub(super) async fn start_retry_run_with_user(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
        current_user: Option<&CurrentUser>,
    ) -> Result<TaskRunRecord, String> {
        self.start_run_with_trigger(task_id, input, RunTriggerSource::Retry, current_user)
            .await
    }

    pub(super) fn start_lock_for_task(&self, task_id: &str) -> Arc<AsyncMutex<()>> {
        let mut locks = self.start_locks.lock();
        locks
            .entry(task_id.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    async fn start_run_with_trigger(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
        trigger: RunTriggerSource,
        current_user: Option<&CurrentUser>,
    ) -> Result<TaskRunRecord, String> {
        let start_lock = self.start_lock_for_task(task_id);
        let _guard = start_lock.lock().await;
        let task = self
            .store
            .get_task(task_id)
            .await?
            .ok_or_else(|| format!("task not found: {task_id}"))?;
        let task = save_task_if_tenant_aligned(&self.store, task).await?;
        info!(
            task_id = task.id.as_str(),
            task_title = task.title.as_str(),
            task_status = task.status.status_string(),
            schedule_mode = task.schedule.mode.mode_key(),
            parent_task_id = task.parent_task_id.as_deref().unwrap_or(""),
            source_run_id = task.source_run_id.as_deref().unwrap_or(""),
            requested_model_config_id = input.model_config_id.as_deref().unwrap_or(""),
            has_prompt_override = input
                .prompt_override
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            "task runner received start_run request"
        );
        if matches!(task.schedule.mode, TaskScheduleMode::ContactAsync)
            && !contact_async_trigger_is_allowed(trigger)
        {
            return Err("contact_async tasks can only be started by the scheduler".to_string());
        }
        if task.status == TaskStatus::Cancelled && !cancelled_task_trigger_is_allowed(trigger) {
            return Err(format!("task has been cancelled: {task_id}"));
        }
        if self.store.has_active_run_for_task(task_id).await? {
            info!(
                task_id = task.id.as_str(),
                task_title = task.title.as_str(),
                "task runner rejected start_run because an active run already exists"
            );
            return Err("an active run already exists for this task".to_string());
        }
        self.ensure_task_thread(&task).await?;

        let model_config_id = normalized_optional(input.model_config_id.clone())
            .or(task.default_model_config_id.clone())
            .ok_or_else(|| {
                "task has no bound model config and this run request did not provide one"
                    .to_string()
            })?;
        let model_config = self
            .store
            .get_model_config(&model_config_id)
            .await?
            .ok_or_else(|| format!("model config not found: {model_config_id}"))?;
        if !model_config.enabled {
            return Err(format!("model config is disabled: {model_config_id}"));
        }
        if let Some(current_user) = current_user {
            if !current_user.can_access_owned_resource(model_config.owner_user_id.as_deref()) {
                return Err(format!("model config not found: {model_config_id}"));
            }
        }
        let capability_policy = self.resolve_task_runner_policy_for_task(&task).await?;
        let agent_key = crate::models::task_runner_agent_key_for(
            task.task_profile.as_str(),
            task.mcp_config.requires_execution,
        );
        let mut runtime_task = task.clone();
        if let Some(policy) = capability_policy.as_ref() {
            policy.apply_to_task(&mut runtime_task)?;
            runtime_task = task_with_runtime_mcp_routing_authoritative(
                &self.config,
                &self.store,
                runtime_task,
            )
            .await?;
        } else {
            runtime_task =
                task_with_runtime_mcp_routing(&self.config, &self.store, runtime_task).await?;
        }
        let effective_workspace_dir =
            ensure_effective_task_workspace_dir(&self.config, &runtime_task, &model_config)?;
        let execution_environment_mode = self
            .effective_execution_environment_mode()
            .await
            .unwrap_or_else(|_| self.config.default_execution_environment_mode.clone());
        let sandbox_enabled = self
            .should_route_task_to_sandbox(&runtime_task, capability_policy.is_some())
            .await
            .unwrap_or(false);

        let run_id = Uuid::new_v4().to_string();
        let skill_snapshots = capability_policy
            .as_ref()
            .map(|policy| policy.skill_snapshots(&runtime_task))
            .transpose()?
            .unwrap_or_default();
        let input_snapshot = json!({
            "agent_key": agent_key.as_str(),
            "task_id": task.id,
            "task_title": task.title,
            "objective": task.objective,
            "description": task.description,
            "input_payload": task.input_payload,
            "prompt_override": input.prompt_override,
            "model_config_id": model_config_id,
            "mcp_config": runtime_task.mcp_config,
            "skill_snapshots": skill_snapshots,
            "effective_workspace_dir": effective_workspace_dir.as_str(),
            "execution_environment_mode": execution_environment_mode,
            "sandbox_enabled": sandbox_enabled,
        });
        let now = now_rfc3339();
        let run = TaskRunRecord::queued(
            run_id.clone(),
            task.id.clone(),
            model_config_id.clone(),
            task.memory_thread_id.clone(),
            input_snapshot,
            now,
        );
        self.store.save_run(run.clone()).await?;
        info!(
            run_id = run.id.as_str(),
            task_id = task.id.as_str(),
            task_title = task.title.as_str(),
            model_config_id = model_config_id.as_str(),
            workspace_dir = effective_workspace_dir.as_str(),
            schedule_mode = task.schedule.mode.mode_key(),
            parent_task_id = task.parent_task_id.as_deref().unwrap_or(""),
            source_run_id = task.source_run_id.as_deref().unwrap_or(""),
            "task runner queued run"
        );
        if let Ok(Some(mut task_record)) = self.store.get_task(task_id).await {
            if task_record.status != TaskStatus::Cancelled
                || cancelled_task_trigger_is_allowed(trigger)
            {
                task_record.status = TaskStatus::Queued;
                task_record.last_run_id = Some(run.id.clone());
                task_record.updated_at = now_rfc3339();
                if let Err(err) = self.store.save_task(task_record).await {
                    warn!(
                        "failed to persist queued task state for task {} and run {}: {}",
                        task_id, run.id, err
                    );
                }
            }
        }
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "queued",
                Some("task run queued".to_string()),
                None,
            ))
            .await?;

        Ok(run)
    }
}

fn contact_async_trigger_is_allowed(trigger: RunTriggerSource) -> bool {
    matches!(
        trigger,
        RunTriggerSource::Scheduler | RunTriggerSource::Retry
    )
}

fn cancelled_task_trigger_is_allowed(trigger: RunTriggerSource) -> bool {
    matches!(trigger, RunTriggerSource::Retry)
}

#[cfg(test)]
mod tests {
    use super::{
        cancelled_task_trigger_is_allowed, contact_async_trigger_is_allowed, RunTriggerSource,
    };

    #[test]
    fn contact_async_allows_scheduler_and_explicit_retry_only() {
        assert!(contact_async_trigger_is_allowed(
            RunTriggerSource::Scheduler
        ));
        assert!(contact_async_trigger_is_allowed(RunTriggerSource::Retry));
        assert!(!contact_async_trigger_is_allowed(RunTriggerSource::Manual));
    }

    #[test]
    fn cancelled_task_can_only_be_reopened_by_explicit_retry() {
        assert!(cancelled_task_trigger_is_allowed(RunTriggerSource::Retry));
        assert!(!cancelled_task_trigger_is_allowed(RunTriggerSource::Manual));
        assert!(!cancelled_task_trigger_is_allowed(
            RunTriggerSource::Scheduler
        ));
    }
}
