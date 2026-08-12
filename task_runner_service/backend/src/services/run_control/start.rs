// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::auth::CurrentUser;
use crate::models::{
    normalize_execution_environment_mode, normalize_project_id, PUBLIC_PROJECT_ID,
};
use crate::services::project_management_api_client;

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
            policy.apply_to_task(&mut runtime_task)?;
        }
        let effective_workspace_dir =
            ensure_effective_task_workspace_dir(&self.config, &runtime_task, &model_config)?;
        let configured_execution_environment_mode =
            self.effective_execution_environment_mode().await?;
        let execution_environment_mode = self
            .execution_environment_mode_for_task(
                &runtime_task,
                configured_execution_environment_mode.as_str(),
            )
            .await;
        let sandbox_enabled = self
            .should_route_task_to_sandbox(&runtime_task, capability_policy.is_some())
            .await?;

        let run_id = Uuid::new_v4().to_string();
        let skill_snapshots = capability_policy
            .as_ref()
            .map(|policy| policy.skill_snapshots(&runtime_task))
            .transpose()?
            .unwrap_or_default();
        let plugin_snapshots = capability_policy
            .as_ref()
            .map(|policy| policy.plugin_snapshots(&runtime_task))
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
            "retry_instruction": input.retry_instruction,
            "model_config_id": model_config_id,
            "plugin_config": runtime_task.plugin_config,
            "mcp_config": runtime_task.mcp_config,
            "skill_snapshots": skill_snapshots,
            "plugin_snapshots": plugin_snapshots,
            "effective_workspace_dir": effective_workspace_dir.as_str(),
            "execution_environment_mode": execution_environment_mode,
            "sandbox_enabled": sandbox_enabled,
            "retry_of_run_id": retry_of_run_id,
        });
        let now = now_rfc3339();
        let mut run = TaskRunRecord::queued(
            run_id.clone(),
            task.id.clone(),
            model_config_id.clone(),
            task.memory_thread_id.clone(),
            input_snapshot,
            plugin_snapshots,
            now,
        );
        run.execution_lane_key = task.execution_lane_key();
        let requested_dispatch_paused = task.task_tool_state.execution_paused;
        run.dispatch_paused = requested_dispatch_paused || retry_of_run_id.is_some();
        self.store.save_run(run.clone()).await?;
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
        // Publish the new run identity immediately. Retries reuse the same task
        // id, so downstream project execution links must replace the terminal
        // run id/status before batch pause or cancellation decisions are made.
        self.try_send_task_callback("task.run.started", task_id, Some(&run))
            .await;
        if let Err(err) = self.enqueue_run_dispatch_if_needed(&run).await {
            warn!(
                run_id = run.id.as_str(),
                task_id = task_id,
                error = err.as_str(),
                "failed to enqueue queued run for rabbitmq dispatch"
            );
        }

        Ok(run)
    }

    async fn execution_environment_mode_for_task(
        &self,
        task: &TaskRecord,
        configured_mode: &str,
    ) -> String {
        let fallback = normalize_execution_environment_mode(Some(configured_mode));
        let project_id = normalize_project_id(Some(task.project_id.clone()));
        if project_id == PUBLIC_PROJECT_ID
            || !project_management_api_client::project_service_enabled(&self.config)
        {
            return fallback;
        }

        match project_management_api_client::sync_get_project(&self.config, project_id.as_str())
            .await
        {
            Ok(Some(project)) => execution_environment_mode_for_project_source(
                project.source_type.as_deref(),
                fallback.as_str(),
            ),
            Ok(None) => fallback,
            Err(error) => {
                warn!(
                    project_id = project_id.as_str(),
                    error = error.as_str(),
                    "failed to resolve project execution environment mode; using configured fallback"
                );
                fallback
            }
        }
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
        RunTriggerSource::Scheduler | RunTriggerSource::Retry | RunTriggerSource::AutomaticRetry
    )
}

fn cancelled_task_trigger_is_allowed(trigger: RunTriggerSource) -> bool {
    matches!(trigger, RunTriggerSource::Retry)
}

fn execution_environment_mode_for_project_source(
    source_type: Option<&str>,
    configured_mode: &str,
) -> String {
    match source_type
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("cloud") => "cloud".to_string(),
        Some("local" | "local_connector") => "local".to_string(),
        _ => normalize_execution_environment_mode(Some(configured_mode)),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cancelled_task_trigger_is_allowed, contact_async_trigger_is_allowed,
        execution_environment_mode_for_project_source, RunTriggerSource,
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

    #[test]
    fn cloud_project_overrides_a_local_host_default_for_run_observability() {
        assert_eq!(
            execution_environment_mode_for_project_source(Some("cloud"), "local"),
            "cloud"
        );
    }

    #[test]
    fn local_connector_project_remains_local_even_with_a_cloud_default() {
        assert_eq!(
            execution_environment_mode_for_project_source(Some("local_connector"), "cloud"),
            "local"
        );
    }

    #[test]
    fn unknown_project_source_uses_the_configured_mode() {
        assert_eq!(
            execution_environment_mode_for_project_source(Some("legacy"), "cloud"),
            "cloud"
        );
    }
}
