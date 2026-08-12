// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::TaskMcpConfig;
use crate::services::TaskRunnerCapabilityPolicy;
use chatos_ai_runtime::TaskRunReport;
use chatos_cloud_agent_protocol::CloudAgentRunStatus;
use chatos_cloud_agent_runtime::{
    cloud_agent_trigger_execution_identity, CloudAgentModelTrigger, CloudAgentProfile,
    CloudAgentRunStore, CloudAgentSingleStepExecution, CloudAgentSingleStepOutput,
};

impl RunService {
    pub(crate) async fn finalize_cloud_agent_terminal(
        &self,
        agent_run_id: &str,
    ) -> Result<(), String> {
        let cloud_run = self
            .cloud_agent_store
            .load_run(agent_run_id)
            .await?
            .ok_or_else(|| format!("Cloud Agent run not found: {agent_run_id}"))?;
        if !cloud_run.status.is_terminal() {
            return Err("Cloud Agent lifecycle event arrived before terminal state".to_string());
        }
        let mut run = self
            .store
            .get_run(cloud_run.owner_entity_id.as_str())
            .await?
            .ok_or_else(|| format!("Task Run not found: {}", cloud_run.owner_entity_id))?;
        if matches!(
            run.status,
            TaskRunStatus::Succeeded
                | TaskRunStatus::Failed
                | TaskRunStatus::Cancelled
                | TaskRunStatus::Blocked
        ) {
            return Ok(());
        }
        let task = self
            .store
            .get_task(run.task_id.as_str())
            .await?
            .ok_or_else(|| format!("Task not found: {}", run.task_id))?;
        let outcome = cloud_run.terminal_outcome.unwrap_or(Value::Null);
        let ai_report = match cloud_run.status {
            CloudAgentRunStatus::Succeeded => chatos_ai_runtime::AiTurnReport {
                status: chatos_ai_runtime::AiTurnStatus::Completed,
                content: outcome
                    .get("content")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                reasoning: outcome
                    .get("reasoning")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                error: None,
                tool_calls: None,
                finish_reason: outcome
                    .get("finish_reason")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                usage: outcome
                    .get("usage")
                    .cloned()
                    .filter(|value| !value.is_null()),
                response_id: outcome
                    .get("response_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                completed_at: now_rfc3339(),
            },
            CloudAgentRunStatus::Cancelled => chatos_ai_runtime::AiTurnReport::aborted(),
            CloudAgentRunStatus::Failed | CloudAgentRunStatus::Blocked => {
                chatos_ai_runtime::AiTurnReport::failed(
                    outcome
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("Cloud Agent execution failed"),
                )
            }
            _ => return Err("Cloud Agent terminal lifecycle has non-terminal status".to_string()),
        };
        let report = TaskRunReport::from_ai_report(
            task.id.clone(),
            run.id.clone(),
            Some(run.model_config_id.clone()),
            ai_report,
        );
        let effective_workspace_dir = run
            .input_snapshot
            .get("effective_workspace_dir")
            .and_then(Value::as_str)
            .unwrap_or(self.config.default_workspace_dir.as_str())
            .to_string();
        self.finalize_model_phase(
            &task,
            &mut run,
            report,
            effective_workspace_dir.as_str(),
            None,
            None,
        )
        .await;
        if let Some(session_ref) = cloud_run.mcp_runtime_session_ref {
            let config =
                chatos_mcp_management_sdk::McpManagementClientConfig::from_env("task-runner")
                    .await
                    .map_err(|error| error.to_string())?;
            let client = chatos_mcp_management_sdk::McpManagementClient::new(config)
                .map_err(|error| error.to_string())?;
            client
                .close_runtime_session(session_ref.as_str())
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

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
        if let Err(err) = ensure_queued_mcp_scope_unchanged(&task, &run) {
            self.finish_failed_before_execution(&task, &mut run, ".", err)
                .await;
            return;
        }
        let current_plugin_snapshots = match capability_policy.plugin_snapshots(&task) {
            Ok(snapshots) => snapshots,
            Err(err) => {
                self.finish_failed_before_execution(&task, &mut run, ".", err)
                    .await;
                return;
            }
        };
        if current_plugin_snapshots != run.plugin_snapshots {
            self.finish_failed_before_execution(
                &task,
                &mut run,
                ".",
                "Plugin Release, installation, component, permission, auth, device, or workspace snapshot changed after this run was queued"
                    .to_string(),
            )
            .await;
            return;
        }
        let input = StartTaskRunRequest {
            model_config_id: Some(run.model_config_id.clone()),
            prompt_override: run
                .input_snapshot
                .get("prompt_override")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            retry_instruction: run
                .input_snapshot
                .get("retry_instruction")
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

    pub(in crate::services) async fn ensure_run_thread(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
    ) -> Result<(), String> {
        super::super::task_threads::ensure_run_thread_for_config(&self.config, task, run).await
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

#[derive(Clone)]
struct TaskRunnerSingleStepResolver {
    service: RunService,
}

struct TaskRunnerSingleStepExecutable {
    prepared: crate::services::run_model_phase::PreparedSingleModelStep,
    iteration: usize,
    reason: String,
    model_attempt: usize,
}

impl TaskRunnerSingleStepExecutable {
    async fn execute(self) -> Result<chatos_ai_runtime::AiSingleStepOutcome, String> {
        self.prepared
            .execute(self.iteration, self.reason, self.model_attempt)
            .await
    }
}

impl TaskRunnerSingleStepResolver {
    async fn prepare(
        &self,
        cloud_run: &chatos_cloud_agent_protocol::CloudAgentRunRecord,
        _agent_run_id: &str,
        trigger: &CloudAgentModelTrigger,
    ) -> Result<TaskRunnerSingleStepExecutable, String> {
        let mut run = self
            .service
            .store
            .get_run(cloud_run.owner_entity_id.as_str())
            .await?
            .ok_or_else(|| format!("Task Run not found: {}", cloud_run.owner_entity_id))?;
        let task = self
            .service
            .store
            .get_task(run.task_id.as_str())
            .await?
            .ok_or_else(|| format!("Task not found: {}", run.task_id))?;
        let mut task = save_task_if_tenant_aligned(&self.service.store, task).await?;
        let model_config = self
            .service
            .store
            .get_model_config(run.model_config_id.as_str())
            .await?
            .ok_or_else(|| format!("model config not found: {}", run.model_config_id))?;
        if !model_config.enabled {
            return Err(format!("model config is disabled: {}", model_config.id));
        }
        let capability_policy = self
            .service
            .resolve_task_runner_policy_for_task(&task)
            .await?
            .ok_or_else(|| {
                "Plugin Management capability configuration is required before Task Runner Agent execution"
                    .to_string()
            })?;
        capability_policy.apply_to_task(&mut task)?;
        ensure_queued_mcp_scope_unchanged(&task, &run)?;
        let current_plugin_snapshots = capability_policy.plugin_snapshots(&task)?;
        if current_plugin_snapshots != run.plugin_snapshots {
            return Err("Plugin snapshot changed after this run was queued".to_string());
        }
        let input = StartTaskRunRequest {
            model_config_id: Some(run.model_config_id.clone()),
            prompt_override: run
                .input_snapshot
                .get("prompt_override")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            retry_instruction: run
                .input_snapshot
                .get("retry_instruction")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        };
        let effective_workspace_dir = run
            .input_snapshot
            .get("effective_workspace_dir")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                ensure_effective_task_workspace_dir(&self.service.config, &task, &model_config).ok()
            })
            .unwrap_or_else(|| self.service.config.default_workspace_dir.clone());
        let Some(prerequisite_context) = self
            .service
            .prepare_prerequisite_context_event_driven(&task, &run, &input)
            .await?
        else {
            return Err(crate::services::CLOUD_AGENT_DEPENDENCY_WAITING.to_string());
        };
        if run.status == TaskRunStatus::Queued
            && !self
                .service
                .initialize_model_phase(
                    &task,
                    &mut run,
                    effective_workspace_dir.as_str(),
                    &prerequisite_context,
                    true,
                )
                .await
        {
            return Err("Task Run could not enter the model phase".to_string());
        }
        let prepared = self
            .service
            .prepare_model_execution(
                &task,
                &model_config,
                &mut run,
                &input,
                effective_workspace_dir.as_str(),
                &prerequisite_context,
                Some(&capability_policy),
                cloud_run.mcp_runtime_session_ref.as_deref(),
            )
            .await?;
        let prepared = self
            .service
            .prepare_single_model_step(&task, &run, &model_config, prepared)
            .await?
            .prepare_for_trigger(cloud_run, trigger)?;
        let (reason, model_attempt) = cloud_agent_trigger_execution_identity(trigger);
        Ok(TaskRunnerSingleStepExecutable {
            prepared,
            iteration: usize::try_from(cloud_run.iteration.saturating_add(1)).unwrap_or(usize::MAX),
            reason,
            model_attempt,
        })
    }
}

#[async_trait::async_trait]
impl CloudAgentProfile for TaskRunnerSingleStepResolver {
    async fn execute_single_step(
        &self,
        cloud_run: &chatos_cloud_agent_protocol::CloudAgentRunRecord,
        trigger: &CloudAgentModelTrigger,
    ) -> Result<CloudAgentSingleStepExecution, String> {
        let executable = match self
            .prepare(cloud_run, &cloud_run.ordering.agent_run_id, trigger)
            .await
        {
            Ok(executable) => executable,
            Err(error) if error == crate::services::CLOUD_AGENT_DEPENDENCY_WAITING => {
                return Ok(CloudAgentSingleStepExecution::AckWithoutTransition);
            }
            Err(error) => return Err(error),
        };
        let mcp_command_queue = executable.prepared.mcp_command_queue.clone();
        let mcp_runtime_session_ref = executable.prepared.mcp_runtime_session_ref.clone();
        let retry_input_items = executable.prepared.continuation_input_items();
        let outcome = executable.execute().await?;
        Ok(CloudAgentSingleStepExecution::Apply(
            CloudAgentSingleStepOutput::new(outcome)
                .with_mcp_runtime(mcp_runtime_session_ref, mcp_command_queue)
                .with_retry_input_items(retry_input_items),
        ))
    }

    async fn finalize_terminal(
        &self,
        cloud_run: &chatos_cloud_agent_protocol::CloudAgentRunRecord,
    ) -> Result<(), String> {
        self.service
            .finalize_cloud_agent_terminal(cloud_run.ordering.agent_run_id.as_str())
            .await
    }
}

pub(crate) fn cloud_agent_profile(
    service: RunService,
) -> impl CloudAgentProfile + Clone + Send + Sync + 'static {
    TaskRunnerSingleStepResolver { service }
}

fn ensure_queued_mcp_scope_unchanged(task: &TaskRecord, run: &TaskRunRecord) -> Result<(), String> {
    let Some(value) = run.input_snapshot.get("mcp_config") else {
        // Runs queued before MCP scope freezing was introduced remain executable.
        return Ok(());
    };
    let queued = serde_json::from_value::<TaskMcpConfig>(value.clone())
        .map_err(|error| format!("queued MCP scope snapshot is invalid: {error}"))?;
    let queued_scope = frozen_mcp_resource_scope(&queued);
    let current_scope = frozen_mcp_resource_scope(&task.mcp_config);
    if queued_scope != current_scope {
        return Err(format!(
            "MCP capability scope changed after this run was queued; queued=[{}], current=[{}]",
            queued_scope.join(","),
            current_scope.join(",")
        ));
    }
    Ok(())
}

fn frozen_mcp_resource_scope(config: &TaskMcpConfig) -> Vec<String> {
    let builtin_kinds = chatos_mcp_runtime::complete_builtin_kind_dependencies(
        config
            .enabled_builtin_kinds
            .iter()
            .filter_map(|kind| chatos_mcp_runtime::builtin_kind_by_any(kind)),
    );
    let mut resource_ids = builtin_kinds
        .iter()
        .filter_map(|kind| chatos_mcp::system_mcp_descriptor_by_any(kind.kind_name()))
        .map(|descriptor| descriptor.resource_id.to_string())
        .chain(
            config
                .external_mcp_config_ids
                .iter()
                .filter_map(|resource_id| {
                    let resource_id = resource_id.trim();
                    (!resource_id.is_empty()).then(|| resource_id.to_string())
                }),
        )
        .collect::<Vec<_>>();
    resource_ids.sort();
    resource_ids.dedup();
    resource_ids
}

#[cfg(test)]
mod mcp_scope_freeze_tests {
    use super::*;

    #[test]
    fn frozen_scope_contains_only_selected_resources_and_required_dependencies() {
        let config = TaskMcpConfig {
            enabled_builtin_kinds: vec!["CodeMaintainerWrite".to_string()],
            external_mcp_config_ids: vec!["postgres-mcp".to_string()],
            ..TaskMcpConfig::default()
        };

        assert_eq!(
            frozen_mcp_resource_scope(&config),
            vec![
                "builtin_code_maintainer_read".to_string(),
                "builtin_code_maintainer_write".to_string(),
                "postgres-mcp".to_string(),
            ]
        );
    }

    #[test]
    fn frozen_scope_detects_an_unselected_optional_mcp() {
        let selected = TaskMcpConfig {
            external_mcp_config_ids: vec!["postgres-mcp".to_string()],
            ..TaskMcpConfig::default()
        };
        let expanded = TaskMcpConfig {
            external_mcp_config_ids: vec!["browser-mcp".to_string(), "postgres-mcp".to_string()],
            ..TaskMcpConfig::default()
        };

        assert_ne!(
            frozen_mcp_resource_scope(&selected),
            frozen_mcp_resource_scope(&expanded)
        );
    }
}
