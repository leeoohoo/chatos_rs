// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use super::*;
use crate::models::TaskMcpConfig;
use crate::services::plugin_runtime_relay::{
    cancel_prepared_plugin_sessions, dispatch_prepared_plugin_hooks,
};
use crate::services::run_model_phase::{
    is_sandbox_infrastructure_failure, plugin_hook_terminal_state,
};
use crate::services::stream_events::flush_pending_stream_event;
use chatos_ai_runtime::TaskRunReport;
use chatos_cloud_agent_protocol::CloudAgentRunStatus;
use chatos_cloud_agent_runtime::{
    cloud_agent_mcp_result_callback_payload, cloud_agent_trigger_execution_identity,
    CloudAgentModelTrigger, CloudAgentProfile, CloudAgentRunStore, CloudAgentSingleStepExecution,
    CloudAgentSingleStepOutput,
};
use sha2::{Digest, Sha256};

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
        let lifecycle = cloud_run
            .input
            .get("lifecycle")
            .cloned()
            .map(serde_json::from_value::<
                crate::services::run_model_phase::callbacks::runtime_state::TaskRunnerLifecycleState,
            >)
            .transpose()
            .map_err(|error| format!("decode terminal Task Runner lifecycle failed: {error}"))?
            .unwrap_or_default();
        let visible_response = outcome
            .get("visible_response")
            .cloned()
            .map(serde_json::from_value::<chatos_ai_runtime::AiResponse>)
            .transpose()
            .map_err(|error| format!("decode terminal visible response failed: {error}"))?
            .or(lifecycle.visible_response);
        let ai_report = match cloud_run.status {
            CloudAgentRunStatus::Succeeded => chatos_ai_runtime::AiTurnReport {
                status: chatos_ai_runtime::AiTurnStatus::Completed,
                content: visible_response
                    .as_ref()
                    .map(|response| response.content.clone())
                    .or_else(|| {
                        outcome
                            .get("content")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    }),
                reasoning: visible_response
                    .as_ref()
                    .and_then(|response| response.reasoning.clone())
                    .or_else(|| {
                        outcome
                            .get("reasoning")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    }),
                error: None,
                tool_calls: None,
                finish_reason: visible_response
                    .as_ref()
                    .and_then(|response| response.finish_reason.clone())
                    .or_else(|| {
                        outcome
                            .get("finish_reason")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    }),
                usage: visible_response
                    .as_ref()
                    .and_then(|response| response.usage.clone())
                    .or_else(|| {
                        outcome
                            .get("usage")
                            .cloned()
                            .filter(|value| !value.is_null())
                    }),
                response_id: visible_response
                    .as_ref()
                    .and_then(|response| response.response_id.clone())
                    .or_else(|| {
                        outcome
                            .get("response_id")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    }),
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
        let mut report = TaskRunReport::from_ai_report(
            task.id.clone(),
            run.id.clone(),
            Some(run.model_config_id.clone()),
            ai_report,
        );
        report.execution_outcome = outcome
            .get("task_execution_outcome")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| format!("decode terminal task execution outcome failed: {error}"))?
            .or(lifecycle.execution_outcome);
        let supply_chain_evidence = outcome
            .get("supply_chain")
            .or_else(|| cloud_run.input.get("supply_chain"))
            .cloned()
            .map(
                serde_json::from_value::<
                    crate::services::run_model_phase::supply_chain::SupplyChainEvidenceState,
                >,
            )
            .transpose()
            .map_err(|error| format!("decode terminal supply-chain state failed: {error}"))?
            .unwrap_or_default();
        if task.mcp_config.requires_execution {
            let supply_chain_policy = self.effective_node_supply_chain_policy().await?;
            let supply_chain_report = supply_chain_evidence.evaluate(&supply_chain_policy);
            if supply_chain_report.applicable {
                crate::services::run_model_phase::callbacks::execution::apply_supply_chain_outcome_gate(
                    &mut report,
                    &supply_chain_report,
                );
                self.store.append_run_event_sync(TaskRunEventRecord::new(
                    run.id.clone(),
                    "supply_chain_audit",
                    Some(match supply_chain_report.status {
                        "passed" => "Node.js 供应链审计通过".to_string(),
                        _ => "Node.js 供应链审计未通过".to_string(),
                    }),
                    Some(supply_chain_report.event_payload()),
                ));
            }
        }
        let effective_workspace_dir = run
            .input_snapshot
            .get("effective_workspace_dir")
            .and_then(Value::as_str)
            .unwrap_or(self.config.default_workspace_dir.as_str())
            .to_string();
        let plugin_sessions = cloud_run
            .input
            .get("plugin_sessions")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| format!("decode terminal Plugin sessions failed: {error}"))?
            .map(|snapshots| self.restore_plugin_runtime(&task, &run, snapshots))
            .transpose()?
            .map(|runtime| runtime.sessions)
            .unwrap_or_default();
        let (hook_event, hook_terminal_outcome) = plugin_hook_terminal_state(&report);
        let hook_outcome = dispatch_prepared_plugin_hooks(
            plugin_sessions.as_slice(),
            hook_event,
            &chatos_plugin_management_sdk::PluginHookEventContext {
                agent_key: run
                    .input_snapshot
                    .get("agent_key")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                outcome: Some(hook_terminal_outcome),
                summary_sha256: Some(hex::encode(Sha256::digest(
                    report
                        .error
                        .as_deref()
                        .or(report.content.as_deref())
                        .unwrap_or_default()
                        .as_bytes(),
                ))),
                ..chatos_plugin_management_sdk::PluginHookEventContext::default()
            },
        )
        .await;
        if hook_outcome.blocking_failure {
            let message = if hook_outcome.errors.is_empty() {
                format!(
                    "Plugin Hook {} failed with fail_run policy",
                    hook_event.as_str()
                )
            } else {
                format!(
                    "Plugin Hook {} dispatch failed: {}",
                    hook_event.as_str(),
                    hook_outcome.errors.join("; ")
                )
            };
            self.store.append_run_event_sync(TaskRunEventRecord::new(
                run.id.clone(),
                "plugin_hook_blocked",
                Some(message.clone()),
                Some(json!({"event": hook_event, "blocking_failure": true})),
            ));
            report.status = chatos_ai_runtime::AiTurnStatus::Failed;
            report.error = Some(match report.error.take() {
                Some(error) => format!("{error}; {message}"),
                None => message,
            });
        }
        cancel_prepared_plugin_sessions(plugin_sessions.as_slice()).await;
        let sandbox_infrastructure_failure = report
            .error
            .as_deref()
            .is_some_and(is_sandbox_infrastructure_failure);
        let sandbox_context =
            crate::services::sandbox_runtime::SandboxRuntimeContext::from_run(&run)?;
        let sandbox_output = if let Some(context) = sandbox_context.as_ref() {
            self.release_sandbox(&run, context).await
        } else {
            None
        };
        let harness_run_context =
            crate::services::harness_run_git::HarnessRunContext::from_run(&run)?;
        let harness_output = if let Some(context) = harness_run_context.as_ref() {
            Some(
                self.commit_harness_run_output(
                    &run,
                    context,
                    sandbox_output
                        .as_ref()
                        .and_then(|output| output.output_workspace.as_deref()),
                )
                .await,
            )
        } else {
            None
        };
        let harness_merge_conflict = harness_output
            .as_ref()
            .is_some_and(|output| output.status == "merge_conflict");
        self.finalize_model_phase(
            &task,
            &mut run,
            report,
            effective_workspace_dir.as_str(),
            sandbox_output,
            harness_output,
        )
        .await;
        if let Some(context) = harness_run_context.as_ref() {
            self.cleanup_harness_run_workspace(context);
        }
        self.unregister_runtime_abort_token(run.id.as_str());
        if harness_merge_conflict {
            self.retry_after_harness_merge_conflict(&task, &run).await;
        } else if sandbox_infrastructure_failure {
            self.retry_after_sandbox_infrastructure_failure(&task, &run)
                .await;
        }
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
}

#[derive(Clone)]
struct TaskRunnerSingleStepResolver {
    service: RunService,
}

struct TaskRunnerSingleStepExecutable {
    service: RunService,
    run_id: String,
    effective_workspace_dir: String,
    prepared: crate::services::run_model_phase::PreparedSingleModelStep,
    iteration: usize,
    reason: String,
    model_attempt: usize,
}

impl TaskRunnerSingleStepExecutable {
    async fn execute(
        self,
    ) -> Result<(chatos_ai_runtime::AiSingleStepOutcome, Value, Option<Value>), String> {
        let lifecycle_state = Arc::clone(&self.prepared.lifecycle_state);
        let progress = Arc::clone(&self.prepared.progress);
        let pending_stream_event = Arc::clone(&self.prepared.pending_stream_event);
        let plugin_sessions = self
            .prepared
            .plugin_sessions
            .iter()
            .map(|session| session.snapshot())
            .collect::<Vec<_>>();
        let supply_chain_evidence = Arc::clone(&self.prepared.supply_chain_evidence);
        let outcome = self
            .prepared
            .execute(self.iteration, self.reason, self.model_attempt)
            .await?;
        let lifecycle = lifecycle_state.lock().clone();
        let path_redactor = crate::services::path_redaction::WorkspacePathRedactor::for_workspace(
            self.service.config.default_workspace_dir.as_str(),
            self.effective_workspace_dir.as_str(),
        );
        flush_pending_stream_event(
            &self.service.store,
            self.run_id.as_str(),
            &pending_stream_event,
            Some(&path_redactor),
        );
        let progress = progress.snapshot();
        let supply_chain = supply_chain_evidence.lock().clone();
        let terminal_overlay = lifecycle
            .execution_outcome
            .as_ref()
            .map(|execution_outcome| {
                json!({
                    "task_execution_outcome": execution_outcome,
                    "visible_response": lifecycle.visible_response,
                    "supply_chain": supply_chain,
                })
            });
        Ok((
            outcome,
            json!({
                "lifecycle": lifecycle,
                "supply_chain": supply_chain,
                "progress": progress,
                "plugin_sessions": plugin_sessions,
            }),
            terminal_overlay,
        ))
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
                cloud_run
                    .input
                    .get("plugin_sessions")
                    .map(|value| {
                        serde_json::from_value(value.clone()).map_err(|error| {
                            format!("decode persisted Plugin sessions failed: {error}")
                        })
                    })
                    .transpose()?,
            )
            .await?;
        let prepared = self
            .service
            .prepare_single_model_step(&task, &run, &model_config, prepared)
            .await?
            .prepare_for_trigger(cloud_run, trigger)?;
        if let CloudAgentModelTrigger::ToolResults { items, .. } = trigger {
            let callbacks = &prepared.runtime_options.callbacks;
            if let Some(on_start) = callbacks.on_tools_start.as_ref() {
                on_start(Value::Array(cloud_run.pending_tool_calls.clone()));
            }
            let payload = cloud_agent_mcp_result_callback_payload(
                cloud_run.pending_tool_calls.as_slice(),
                items.as_slice(),
            )?;
            if let Some(on_stream) = callbacks.on_tools_stream.as_ref() {
                if let Some(results) = payload.get("tool_results").and_then(Value::as_array) {
                    for result in results {
                        on_stream(result.clone());
                    }
                }
            }
            if let Some(on_end) = callbacks.on_tools_end.as_ref() {
                on_end(payload);
            }
        }
        let (reason, model_attempt) = cloud_agent_trigger_execution_identity(trigger);
        Ok(TaskRunnerSingleStepExecutable {
            service: self.service.clone(),
            run_id: run.id.clone(),
            effective_workspace_dir,
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
        let (outcome, next_input, terminal_outcome_overlay) = executable.execute().await?;
        Ok(CloudAgentSingleStepExecution::Apply(
            CloudAgentSingleStepOutput::new(outcome)
                .with_mcp_runtime(mcp_runtime_session_ref, mcp_command_queue)
                .with_retry_input_items(retry_input_items)
                .with_next_input(next_input)
                .with_terminal_outcome_overlay(terminal_outcome_overlay),
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
