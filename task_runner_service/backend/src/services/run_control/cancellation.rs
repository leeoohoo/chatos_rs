// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::auth::CurrentUser;

impl RunService {
    pub async fn cancel_run(&self, run_id: &str) -> Result<Option<TaskRunRecord>, String> {
        let Some(current_run) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        match current_run.status {
            TaskRunStatus::Queued | TaskRunStatus::Running => {}
            TaskRunStatus::Succeeded => {
                return Err("cannot cancel a succeeded run".to_string());
            }
            TaskRunStatus::Failed => {
                return Err("cannot cancel a failed run".to_string());
            }
            TaskRunStatus::Cancelled => {
                return Err("cannot cancel an already cancelled run".to_string());
            }
            TaskRunStatus::Blocked => {
                return Err("cannot cancel a blocked run".to_string());
            }
        }
        if current_run.is_waiting_for_workspace_integration() {
            return self
                .cancel_workspace_integration_waiting_run(current_run)
                .await;
        }
        if current_run.cancel_requested {
            if let Err(err) = self.enqueue_run_cancel_event_if_needed(&current_run).await {
                warn!(
                    run_id = current_run.id.as_str(),
                    error = err.as_str(),
                    "failed to republish pending run cancellation event"
                );
            }
            if let Err(err) = self
                .ask_user_prompt_service
                .cancel_pending_prompts_for_run(run_id, "run cancellation requested")
                .await
            {
                warn!(
                    run_id,
                    error = err.as_str(),
                    "failed to cancel pending ask user prompts"
                );
            }
            return Ok(Some(current_run));
        }

        let Some(mut run) = self.store.mark_cancel_requested(run_id).await? else {
            return Ok(None);
        };
        if let Err(err) = self
            .ask_user_prompt_service
            .cancel_pending_prompts_for_run(run_id, "run cancellation requested")
            .await
        {
            warn!(
                run_id,
                error = err.as_str(),
                "failed to cancel pending ask user prompts"
            );
        }
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run_id.to_string(),
                "cancel_requested",
                Some("run cancellation requested".to_string()),
                None,
            ))
            .await?;
        if let Err(err) = self.enqueue_run_cancel_event_if_needed(&run).await {
            warn!(
                run_id = run.id.as_str(),
                worker_id = run.worker_id.as_deref().unwrap_or_default(),
                error = err.as_str(),
                "failed to publish run cancellation event; outbox reconciliation will retry"
            );
        }
        if matches!(run.status, TaskRunStatus::Queued) {
            run.status = TaskRunStatus::Cancelled;
            run.model_phase_status = crate::models::ModelPhaseStatus::Cancelled;
            run.cancel_requested = true;
            run.claim_token = None;
            run.claim_until = None;
            run.finished_at = Some(now_rfc3339());
            run.updated_at = now_rfc3339();
            self.store.save_run(run.clone()).await?;
            if let Some(task_record) = self.store.get_task(&run.task_id).await? {
                self.notify_mcp_management_run_finalized(&task_record, &run)
                    .await;
            }
            self.store
                .append_run_event(TaskRunEventRecord::new(
                    run_id.to_string(),
                    "cancelled",
                    Some("run cancelled before execution started".to_string()),
                    None,
                ))
                .await?;
            if let Some(mut task_record) = self.store.get_task(&run.task_id).await? {
                task_record.status = TaskStatus::Cancelled;
                task_record.last_run_id = Some(run.id.clone());
                task_record.updated_at = now_rfc3339();
                self.store.save_task(task_record).await?;
            }
            self.try_send_terminal_callback(run.task_id.as_str(), &run)
                .await;
        }
        Ok(Some(run))
    }

    async fn cancel_workspace_integration_waiting_run(
        &self,
        mut run: TaskRunRecord,
    ) -> Result<Option<TaskRunRecord>, String> {
        if let Err(err) = self
            .ask_user_prompt_service
            .cancel_pending_prompts_for_run(run.id.as_str(), "run cancellation requested")
            .await
        {
            warn!(
                run_id = run.id.as_str(),
                error = err.as_str(),
                "failed to cancel pending ask user prompts"
            );
        }
        let now = now_rfc3339();
        let message = "run cancelled before workspace integration".to_string();
        run.cancel_before_workspace_integration(message, now.as_str());

        let saved = self.store.save_run(run.clone()).await?;
        self.store.clear_cancel_requested(saved.id.as_str());
        self.store
            .append_run_event(TaskRunEventRecord::new(
                saved.id.clone(),
                "cancel_requested",
                Some("run cancellation requested".to_string()),
                None,
            ))
            .await?;
        self.store
            .append_run_event(TaskRunEventRecord::new(
                saved.id.clone(),
                "cancelled",
                Some("run cancelled before workspace integration".to_string()),
                None,
            ))
            .await?;
        if let Some(mut task_record) = self.store.get_task(&saved.task_id).await? {
            task_record.status = TaskStatus::Cancelled;
            task_record.last_run_id = Some(saved.id.clone());
            task_record.updated_at = now;
            self.store.save_task(task_record).await?;
        }
        self.try_send_terminal_callback(saved.task_id.as_str(), &saved)
            .await;

        Ok(Some(saved))
    }

    pub async fn retry_run(&self, run_id: &str) -> Result<Option<TaskRunRecord>, String> {
        self.retry_run_with_user(run_id, None, None, None).await
    }

    pub async fn retry_run_with_instruction(
        &self,
        run_id: &str,
        retry_instruction: Option<String>,
    ) -> Result<Option<TaskRunRecord>, String> {
        self.retry_run_with_user(run_id, None, retry_instruction, None)
            .await
    }

    pub async fn retry_run_with_instruction_and_execution_service(
        &self,
        run_id: &str,
        retry_instruction: Option<String>,
        execution_service_id: Option<String>,
    ) -> Result<Option<TaskRunRecord>, String> {
        self.retry_run_with_user(run_id, None, retry_instruction, execution_service_id)
            .await
    }

    pub async fn retry_run_for_user(
        &self,
        run_id: &str,
        current_user: &CurrentUser,
    ) -> Result<Option<TaskRunRecord>, String> {
        self.retry_run_with_user(run_id, Some(current_user), None, None)
            .await
    }

    pub async fn retry_run_for_user_with_instruction(
        &self,
        run_id: &str,
        current_user: &CurrentUser,
        retry_instruction: Option<String>,
    ) -> Result<Option<TaskRunRecord>, String> {
        self.retry_run_with_user(run_id, Some(current_user), retry_instruction, None)
            .await
    }

    async fn retry_run_with_user(
        &self,
        run_id: &str,
        current_user: Option<&CurrentUser>,
        retry_instruction: Option<String>,
        execution_service_id: Option<String>,
    ) -> Result<Option<TaskRunRecord>, String> {
        self.retry_run_from_source(
            run_id,
            current_user,
            retry_instruction,
            execution_service_id,
            false,
        )
        .await
    }

    async fn retry_run_from_source(
        &self,
        run_id: &str,
        current_user: Option<&CurrentUser>,
        retry_instruction: Option<String>,
        execution_service_id: Option<String>,
        automatic: bool,
    ) -> Result<Option<TaskRunRecord>, String> {
        let Some(run) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        if matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running) {
            return Err("run is still active and cannot be retried yet".to_string());
        }

        let Some(mut task) = self.store.get_task(run.task_id.as_str()).await? else {
            return Err("task not found for retry run".to_string());
        };
        if let Some(execution_service_id) = execution_service_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            if !task.mcp_config.requires_execution {
                return Err(
                    "execution_service_id can only be selected for an execution task".to_string(),
                );
            }
            task.mcp_config.execution_service_id = Some(execution_service_id);
            task.updated_at = now_rfc3339();
            self.store.save_task(task.clone()).await?;
        }
        if !automatic {
            if let Some(policy) = self.resolve_task_runner_policy_for_task(&task).await? {
                if policy.refresh_task_plugin_selection_for_manual_retry(&mut task)? {
                    task.updated_at = now_rfc3339();
                    self.store.save_task(task.clone()).await?;
                }
            }
        }
        self.ensure_project_execution_retry_configuration_changed(&run, &task)
            .await?;

        let prompt_override = run
            .input_snapshot
            .get("prompt_override")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let request = retry_request_with_current_task_config(prompt_override, retry_instruction);
        let restarted = if automatic {
            self.start_automatic_retry_run(&run.task_id, request, run.id.as_str())
                .await?
        } else {
            self.start_retry_run_with_user(&run.task_id, request, run.id.as_str(), current_user)
                .await?
        };
        Ok(Some(restarted))
    }

    async fn ensure_project_execution_retry_configuration_changed(
        &self,
        run: &TaskRunRecord,
        task: &TaskRecord,
    ) -> Result<(), String> {
        if !matches!(run.status, TaskRunStatus::Blocked | TaskRunStatus::Failed)
            || task
                .input_payload
                .as_ref()
                .and_then(|payload| payload.get("source"))
                .and_then(Value::as_str)
                != Some("chatos_project_requirement_execution")
        {
            return Ok(());
        }
        let mut runtime_task = task.clone();
        if let Some(policy) = self.resolve_task_runner_policy_for_task(task).await? {
            policy.validate_task_plugin_selection_for_run(&runtime_task)?;
            policy.apply_to_task(&mut runtime_task)?;
        }
        let effective_tools =
            crate::services::workspace_execution::effective_task_tool_snapshot_for_scope(
                &runtime_task.mcp_config,
                &runtime_task.execution_scope(),
            );
        let Err(contract_error) =
            crate::services::workspace_execution::validate_project_execution_task_runtime_contract(
                &runtime_task,
                &effective_tools,
            )
        else {
            return Ok(());
        };
        let current_fingerprint =
            crate::services::workspace_execution::task_runtime_capability_fingerprint(
                &runtime_task,
            );
        if retry_capability_configuration_unchanged(
            &run.input_snapshot,
            current_fingerprint.as_str(),
        ) {
            return Err(format!(
                "platform_configuration_unchanged: project execution task capability configuration is still invalid and unchanged for run {}: {contract_error}",
                run.id
            ));
        }
        Ok(())
    }
}

fn retry_capability_configuration_unchanged(
    previous_input_snapshot: &Value,
    current_fingerprint: &str,
) -> bool {
    previous_input_snapshot
        .get("task_runtime_capability_fingerprint")
        .and_then(Value::as_str)
        .map(|previous| previous == current_fingerprint)
        .unwrap_or(false)
}

fn retry_request_with_current_task_config(
    prompt_override: Option<String>,
    retry_instruction: Option<String>,
) -> StartTaskRunRequest {
    StartTaskRunRequest {
        // A retry is explicitly described in the UI as using the task's current
        // configuration. Leaving this unset lets start_run_with_trigger resolve
        // the latest task default instead of pinning the failed run's old model.
        model_config_id: None,
        prompt_override,
        retry_instruction: retry_instruction
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::{retry_capability_configuration_unchanged, retry_request_with_current_task_config};
    use serde_json::json;

    #[test]
    fn retry_uses_current_task_model_configuration() {
        let request = retry_request_with_current_task_config(
            Some("keep prompt".to_string()),
            Some("  use the repaired configuration  ".to_string()),
        );

        assert_eq!(request.model_config_id, None);
        assert_eq!(request.prompt_override.as_deref(), Some("keep prompt"));
        assert_eq!(
            request.retry_instruction.as_deref(),
            Some("use the repaired configuration")
        );
    }

    #[test]
    fn retry_requires_a_changed_runtime_capability_fingerprint() {
        assert!(retry_capability_configuration_unchanged(
            &json!({"task_runtime_capability_fingerprint": "fnv1a64:same"}),
            "fnv1a64:same",
        ));
        assert!(!retry_capability_configuration_unchanged(
            &json!({"task_runtime_capability_fingerprint": "fnv1a64:old"}),
            "fnv1a64:new",
        ));
        assert!(!retry_capability_configuration_unchanged(
            &json!({}),
            "fnv1a64:new",
        ));
    }
}
