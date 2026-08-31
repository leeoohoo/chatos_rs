// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use super::*;
use crate::models::TaskMcpConfig;
use crate::services::stream_events::flush_pending_stream_event;
use chatos_ai_runtime::TaskRunReport;
use chatos_cloud_agent_protocol::CloudAgentRunStatus;
use chatos_cloud_agent_runtime::{
    cloud_agent_mcp_result_callback_payload, cloud_agent_trigger_execution_identity,
    CloudAgentModelTrigger, CloudAgentProfile, CloudAgentRunStore, CloudAgentSingleStepExecution,
    CloudAgentSingleStepOutput,
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
            .filter(|value| !value.is_null())
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
            .filter(|value| !value.is_null())
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| format!("decode terminal task execution outcome failed: {error}"))?
            .or(lifecycle.execution_outcome);
        let supply_chain_evidence = outcome
            .get("supply_chain")
            .or_else(|| cloud_run.input.get("supply_chain"))
            .filter(|value| !value.is_null())
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
                crate::services::run_model_phase::callbacks::execution::attach_supply_chain_outcome_receipt(
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
            if let Some(input_snapshot) = run.input_snapshot.as_object_mut() {
                match supply_chain_evidence
                    .passed_receipt(&supply_chain_policy, &supply_chain_report)
                {
                    Some(receipt) => {
                        input_snapshot.insert("supply_chain_receipt".to_string(), receipt);
                    }
                    None => {
                        input_snapshot.remove("supply_chain_receipt");
                    }
                }
            }
        }
        let effective_workspace_dir = run
            .input_snapshot
            .get("effective_workspace_dir")
            .and_then(Value::as_str)
            .unwrap_or(self.config.default_workspace_dir.as_str())
            .to_string();
        self.finalize_model_phase(&task, &mut run, report, effective_workspace_dir.as_str())
            .await;
        self.unregister_runtime_abort_token(run.id.as_str());
        // `finalize_model_phase` finalizes the entire MCP run. MCP Management
        // owns the run/session lifecycle and removes every session attached to
        // the run as part of that operation. Closing this individual session a
        // second time races with (and normally follows) run finalization, so it
        // deterministically turns a successful terminal delivery into a 404.
        // If run finalization fails, the durable post-process retries the same
        // run-level operation; Task Runner must not bypass it with a separate
        // session-level close.
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
    automatic_recovery_calls: Vec<Value>,
}

impl TaskRunnerSingleStepExecutable {
    async fn execute(
        self,
    ) -> Result<(chatos_ai_runtime::AiSingleStepOutcome, Value, Option<Value>), String> {
        let lifecycle_state = Arc::clone(&self.prepared.lifecycle_state);
        let progress = Arc::clone(&self.prepared.progress);
        let pending_stream_event = Arc::clone(&self.prepared.pending_stream_event);
        let supply_chain_evidence = Arc::clone(&self.prepared.supply_chain_evidence);
        let outcome = if self.automatic_recovery_calls.is_empty() {
            self.prepared
                .execute(self.iteration, self.reason, self.model_attempt)
                .await?
        } else {
            let response_output_items = self
                .automatic_recovery_calls
                .iter()
                .filter_map(|call| {
                    let call_id = chatos_ai_runtime::tool_call::extract_tool_call_id(call)?;
                    let name = chatos_ai_runtime::tool_call::extract_tool_call_name(call)?;
                    Some(chatos_ai_runtime::tool_call::build_function_call_item(
                        call_id,
                        name,
                        chatos_ai_runtime::tool_call::tool_call_arguments_text(call).as_str(),
                    ))
                })
                .collect::<Vec<_>>();
            if response_output_items.len() != self.automatic_recovery_calls.len() {
                return Err(
                    "automatic stale-write recovery produced an invalid tool call".to_string(),
                );
            }
            chatos_ai_runtime::AiSingleStepOutcome::ToolCommand {
                response: chatos_ai_runtime::AiRuntimeResult {
                    content: String::new(),
                    reasoning: None,
                    tool_calls: None,
                    finish_reason: Some("runtime_stale_write_recovery".to_string()),
                    usage: None,
                    response_id: None,
                    response_output_items,
                    request_input_items: self.prepared.continuation_input_items(),
                },
                tool_calls: Value::Array(self.automatic_recovery_calls),
            }
        };
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
        capability_policy.validate_task_plugin_selection_for_run(&task)?;
        capability_policy.apply_to_task(&mut task)?;
        ensure_queued_mcp_scope_unchanged(&task, &run)?;
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
                .await?
        {
            return Err("Task Run was cancelled before entering the model phase".to_string());
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
        let automatic_recovery_calls =
            if let CloudAgentModelTrigger::ToolResults { items, .. } = trigger {
                let tool_results = prepared
                    .persist_external_tool_results(
                        cloud_run.pending_tool_calls.as_slice(),
                        items.as_slice(),
                    )
                    .await?;
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
                prepared.automatic_file_write_recovery_calls(tool_results.as_slice())?
            } else {
                Vec::new()
            };
        let (reason, model_attempt) = cloud_agent_trigger_execution_identity(trigger);
        Ok(TaskRunnerSingleStepExecutable {
            service: self.service.clone(),
            run_id: run.id.clone(),
            effective_workspace_dir,
            prepared,
            iteration: usize::try_from(cloud_run.iteration.saturating_add(1)).unwrap_or(usize::MAX),
            reason,
            model_attempt,
            automatic_recovery_calls,
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

pub(crate) fn cloud_agent_profile(service: RunService) -> impl CloudAgentProfile + Clone + 'static {
    TaskRunnerSingleStepResolver { service }
}

fn ensure_queued_mcp_scope_unchanged(task: &TaskRecord, run: &TaskRunRecord) -> Result<(), String> {
    let Some(value) = run.input_snapshot.get("mcp_config") else {
        return Err("queued Task Run is missing its frozen MCP scope".to_string());
    };
    let queued = serde_json::from_value::<TaskMcpConfig>(value.clone())
        .map_err(|error| format!("queued MCP scope snapshot is invalid: {error}"))?;
    let queued_scope = crate::services::workspace_execution::effective_task_tool_snapshot(&queued)
        .requested_mcp_resource_ids;
    let current_scope =
        crate::services::workspace_execution::effective_task_tool_snapshot(&task.mcp_config)
            .requested_mcp_resource_ids;
    if queued_scope != current_scope {
        return Err(format!(
            "MCP capability scope changed after this run was queued; queued=[{}], current=[{}]",
            queued_scope.join(","),
            current_scope.join(",")
        ));
    }
    Ok(())
}

#[cfg(test)]
mod mcp_scope_freeze_tests {
    use super::*;

    fn frozen_scope(config: &TaskMcpConfig) -> Vec<String> {
        crate::services::workspace_execution::effective_task_tool_snapshot(config)
            .requested_mcp_resource_ids
    }

    #[test]
    fn frozen_scope_contains_only_selected_resources_and_required_dependencies() {
        let config = TaskMcpConfig {
            enabled_builtin_kinds: vec!["CodeMaintainerWrite".to_string()],
            external_mcp_config_ids: vec!["postgres-mcp".to_string()],
            ..TaskMcpConfig::default()
        };

        assert_eq!(
            frozen_scope(&config),
            vec![
                "builtin_code_maintainer_read".to_string(),
                "builtin_code_maintainer_write".to_string(),
                "postgres-mcp".to_string(),
                "system_mcp_task_process_log".to_string(),
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

        assert_ne!(frozen_scope(&selected), frozen_scope(&expanded));
    }
}
