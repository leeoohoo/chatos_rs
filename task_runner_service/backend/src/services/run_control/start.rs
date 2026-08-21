// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::auth::CurrentUser;
use chatos_agent::AgentIdentity;
use chrono::Utc;

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
        self.start_run_with_trigger(task_id, input, RunTriggerSource::Manual, None, current_user)
            .await
    }

    pub async fn start_scheduled_run(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
    ) -> Result<TaskRunRecord, String> {
        self.start_run_with_trigger(task_id, input, RunTriggerSource::Scheduler, None, None)
            .await
    }

    pub(super) async fn start_retry_run_with_user(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
        previous_run_id: &str,
        current_user: Option<&CurrentUser>,
    ) -> Result<TaskRunRecord, String> {
        self.start_run_with_trigger(
            task_id,
            input,
            RunTriggerSource::Retry,
            Some(previous_run_id),
            current_user,
        )
        .await
    }

    pub(super) async fn start_automatic_retry_run(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
        previous_run_id: &str,
    ) -> Result<TaskRunRecord, String> {
        self.start_run_with_trigger(
            task_id,
            input,
            RunTriggerSource::AutomaticRetry,
            Some(previous_run_id),
            None,
        )
        .await
    }

    pub(in crate::services) async fn start_dependency_run(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
    ) -> Result<TaskRunRecord, String> {
        self.start_run_with_trigger(task_id, input, RunTriggerSource::Dependency, None, None)
            .await
    }

    pub(crate) fn start_lock_for_task(&self, task_id: &str) -> KeyedAsyncLockHandle {
        self.start_locks.handle(task_id)
    }

    async fn start_run_with_trigger(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
        trigger: RunTriggerSource,
        retry_of_run_id: Option<&str>,
        current_user: Option<&CurrentUser>,
    ) -> Result<TaskRunRecord, String> {
        let _guard = self.start_lock_for_task(task_id).lock_owned().await;
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
        if capability_policy.is_none() && !task.plugin_config.selected_plugins.is_empty() {
            return Err(
                "Plugin Management is required to resolve selected Plugins before execution"
                    .to_string(),
            );
        }
        let agent_key = capability_policy
            .as_ref()
            .and_then(|policy| chatos_agent::parse_system_agent_key(policy.agent_key()))
            .unwrap_or(self.resolve_task_runner_agent_key_for_task(&task).await?);
        let mut runtime_task = task.clone();
        if let Some(policy) = capability_policy.as_ref() {
            policy.validate_task_plugin_selection_for_run(&runtime_task)?;
            policy.apply_to_task(&mut runtime_task)?;
        }
        let effective_workspace_dir =
            ensure_effective_task_workspace_dir(&self.config, &runtime_task, &model_config)?;
        let effective_tools = crate::services::workspace_execution::effective_task_tool_snapshot(
            &runtime_task.mcp_config,
        );
        crate::services::workspace_execution::validate_project_execution_task_runtime_contract(
            &runtime_task,
            &effective_tools,
        )?;
        let task_runtime_capability_fingerprint =
            crate::services::workspace_execution::task_runtime_capability_fingerprint(
                &runtime_task,
            );
        let execution_lane_key = crate::services::workspace_execution::model_execution_lane_key(
            self,
            &runtime_task,
            &effective_tools,
        )
        .await?;
        let run_id = Uuid::new_v4().to_string();
        let (execution_timeout_ms, ai_read_timeout_ms) = self.effective_run_timeouts_ms().await?;
        let execution_timeout =
            chrono::Duration::milliseconds(i64::try_from(execution_timeout_ms).map_err(|_| {
                "Task Runner execution timeout exceeds supported range".to_string()
            })?);
        let deadline_at = Utc::now()
            .checked_add_signed(execution_timeout)
            .ok_or_else(|| "Task Runner execution deadline exceeds supported range".to_string())?;
        let input_snapshot = json!({
            "agent_key": agent_key.as_str(),
            "task_id": task.id,
            "task_title": task.title,
            "objective": task.objective,
            "description": task.description,
            "input_payload": task.input_payload,
            "prompt_override": input.prompt_override,
            "retry_instruction": input.retry_instruction,
            "model_config_id": model_config_id,
            "plugin_config": runtime_task.plugin_config,
            "mcp_config": runtime_task.mcp_config,
            "effective_workspace_dir": effective_workspace_dir.as_str(),
            "task_runtime_capability_fingerprint": task_runtime_capability_fingerprint,
            "retry_of_run_id": retry_of_run_id,
            "started_as_prerequisite": trigger == RunTriggerSource::Dependency,
            "execution_timeout_ms": execution_timeout_ms,
            "ai_read_timeout_ms": ai_read_timeout_ms,
            "deadline_at": deadline_at,
        });
        let agent = chatos_agent::TaskRunnerAgent::new(agent_key);
        let agent_prompt =
            crate::services::plugin_management_prompts::resolve_task_runner_agent_prompt(
                self,
                &agent,
                model_config.prompt_vendor.as_deref(),
                model_config.provider.as_str(),
            )
            .await?;
        let max_iterations =
            u32::try_from(self.effective_task_execution_max_iterations().await?)
                .map_err(|_| "Task Runner max iterations exceeds Cloud Agent range".to_string())?;
        let ordering_lane_key = format!("task:{}", task.id);
        let agent_run_id = format!("task_runner_agent_{run_id}");
        let cloud_run = chatos_cloud_agent_runtime::create_cloud_agent_run(
            &self.cloud_agent_store,
            chatos_cloud_agent_runtime::NewCloudAgentRun {
                ordering_lane_key: ordering_lane_key.clone(),
                agent_run_id: agent_run_id.clone(),
                owner_service: "task-runner".to_string(),
                owner_entity_type: "task_run".to_string(),
                owner_entity_id: run_id.clone(),
                owner_user_id: task
                    .owner_user_id
                    .as_deref()
                    .or(task.creator_user_id.as_deref())
                    .unwrap_or(task.subject_id.as_str())
                    .to_string(),
                agent_key: agent.descriptor().key.as_str().to_string(),
                input: Value::Null,
                model_config_ref: model_config_id.clone(),
                model_runtime_snapshot_ref: format!("task_run:{run_id}:model_runtime"),
                agent_prompt_revision: agent_prompt.revision.to_string(),
                agent_prompt_checksum: agent_prompt.checksum.clone(),
                capability_policy_revision: capability_policy
                    .as_ref()
                    .map(|policy| policy.policy_revision())
                    .unwrap_or("unmanaged")
                    .to_string(),
                mcp_runtime_session_ref: None,
                current_input_items_ref: format!("task_run:{run_id}:input_snapshot"),
                max_iterations,
                deadline_at: Some(deadline_at),
                runtime_routing_key: "cloud_agent.task_runner.runtime".to_string(),
                start_causation_id: run_id.clone(),
                start_payload: json!({"task_run_id": run_id}),
            },
        )
        .await?;
        let lane_seq = cloud_run.ordering.lane_seq;
        let now = now_rfc3339();
        let mut run = TaskRunRecord::queued(
            run_id.clone(),
            task.id.clone(),
            model_config_id.clone(),
            task.memory_thread_id.clone(),
            input_snapshot,
            now,
        );
        run.effective_tools = effective_tools;
        run.agent_run_id = Some(agent_run_id);
        run.dispatch_event_pending = false;
        run.agent_ordering_lane_key = Some(ordering_lane_key);
        run.agent_lane_seq = Some(lane_seq);
        run.execution_lane_key = execution_lane_key;
        let requested_dispatch_paused = task.task_tool_state.execution_paused;
        run.dispatch_paused = requested_dispatch_paused || retry_of_run_id.is_some();
        if let Err(error) = self.store.save_run(run.clone()).await {
            warn!(
                run_id = run.id.as_str(),
                agent_run_id = run.agent_run_id.as_deref().unwrap_or_default(),
                error = error.as_str(),
                "Task Run persistence failed after Cloud Agent creation"
            );
            return Err(error);
        }
        if let Some(previous_run_id) = retry_of_run_id {
            Box::pin(self.prepare_retry_task_session(
                task.id.as_str(),
                previous_run_id,
                requested_dispatch_paused,
                &mut run,
            ))
            .await?;
        }
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
        // Keep project execution links on the newest run while it is queued.
        // The user-visible started callback is emitted only after real execution begins.
        self.try_send_task_callback("task.run.queued", task_id, Some(&run))
            .await;
        Ok(run)
    }

    async fn prepare_retry_task_session(
        &self,
        _task_id: &str,
        _previous_run_id: &str,
        requested_dispatch_paused: bool,
        run: &mut TaskRunRecord,
    ) -> Result<(), String> {
        run.dispatch_paused = requested_dispatch_paused;
        run.updated_at = now_rfc3339();
        self.store.save_run(run.clone()).await?;
        Ok(())
    }
}

fn contact_async_trigger_is_allowed(trigger: RunTriggerSource) -> bool {
    matches!(
        trigger,
        RunTriggerSource::Scheduler
            | RunTriggerSource::Retry
            | RunTriggerSource::AutomaticRetry
            | RunTriggerSource::Dependency
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
    fn contact_async_allows_scheduler_and_retry_sources() {
        assert!(contact_async_trigger_is_allowed(
            RunTriggerSource::Scheduler
        ));
        assert!(contact_async_trigger_is_allowed(RunTriggerSource::Retry));
        assert!(contact_async_trigger_is_allowed(
            RunTriggerSource::AutomaticRetry
        ));
        assert!(!contact_async_trigger_is_allowed(RunTriggerSource::Manual));
    }

    #[test]
    fn cancelled_task_can_only_be_reopened_by_explicit_retry() {
        assert!(cancelled_task_trigger_is_allowed(RunTriggerSource::Retry));
        assert!(!cancelled_task_trigger_is_allowed(
            RunTriggerSource::AutomaticRetry
        ));
        assert!(!cancelled_task_trigger_is_allowed(RunTriggerSource::Manual));
        assert!(!cancelled_task_trigger_is_allowed(
            RunTriggerSource::Scheduler
        ));
    }
}
