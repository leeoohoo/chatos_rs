// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use chatos_agent::{TaskRunnerAgent, TaskRunnerRunSpecInput};
use chatos_ai_runtime::{
    AiRuntimeOptions, AiSingleStepOutcome, MemoryRecordScope, MemoryScope, RuntimeCallbacks,
    TaskExecutionReviewPolicy, TaskFinalizationLifecycleHook, TaskMemoryRuntimeConfig,
    TaskRunReport, TaskRunSpec, TaskRuntime, TaskRuntimeConfig, ToolResultModelBudgetLimits,
    DEFAULT_TASK_RUN_MAX_ITERATIONS,
};
use chatos_cloud_agent_runtime::cloud_agent_trigger_input_items;
use chatos_mcp_management_sdk::McpManagementRuntimeSessionHandle;
use chatos_mcp_runtime::McpExecutorBuilder;
use memory_engine_sdk::ComposeContextPolicy;
use serde_json::{json, Value};
use tracing::warn;

use super::prerequisite_context::{
    attach_prerequisite_context_to_run, build_task_prompt, PrerequisiteTaskContext,
};
use super::stream_events::{
    append_pending_stream_event, flush_pending_stream_event, PendingRunStreamEvent,
};
use super::task_process_log::{
    task_process_log_prefixed_input_items, task_process_logging_enabled,
};
use super::{summarized_report_content, RunService};
use crate::models::{
    now_rfc3339, ModelConfigRecord, StartTaskRunRequest, TaskRecord, TaskRunEventRecord,
    TaskRunRecord, TaskRunStatus, TaskStatus,
};
use callbacks::runtime_state::TaskRunnerLifecycleState;

pub(in crate::services) mod callbacks;
mod completion;
mod setup;
pub(super) mod supply_chain;

pub(in crate::services) struct PreparedModelExecution {
    agent: TaskRunnerAgent,
    run_spec: TaskRunSpec,
    runtime_config: TaskRuntimeConfig,
    mcp_builder: McpExecutorBuilder,
    mcp_management_runtime_session: McpManagementRuntimeSessionHandle,
    mcp_command_queue: String,
    tool_result_model_budget_limits: ToolResultModelBudgetLimits,
    effective_workspace_dir: String,
}

pub(in crate::services) struct PreparedSingleModelStep {
    pub(crate) agent: TaskRunnerAgent,
    pub(crate) run_spec: TaskRunSpec,
    pub(crate) runtime: TaskRuntime,
    pub(crate) runtime_options: AiRuntimeOptions,
    pub(crate) mcp_runtime_session_ref: String,
    pub(crate) mcp_command_queue: String,
    pub(crate) lifecycle_state: Arc<parking_lot::Mutex<TaskRunnerLifecycleState>>,
    pub(crate) progress: Arc<chatos_ai_runtime::TaskExecutionProgressState>,
    pub(crate) pending_stream_event: Arc<parking_lot::Mutex<PendingRunStreamEvent>>,
    pub(crate) supply_chain_evidence:
        Arc<parking_lot::Mutex<super::run_model_phase::supply_chain::SupplyChainEvidenceState>>,
}

impl PreparedSingleModelStep {
    pub(crate) fn continuation_input_items(&self) -> Vec<Value> {
        self.run_spec.current_input_items.clone()
    }

    pub(crate) fn prepare_for_trigger(
        mut self,
        cloud_run: &chatos_cloud_agent_protocol::CloudAgentRunRecord,
        trigger: &chatos_cloud_agent_runtime::CloudAgentModelTrigger,
    ) -> Result<Self, String> {
        self.restore_durable_state(&cloud_run.input)?;
        // Cloud orchestration follows the official stateless Responses
        // protocol. The durable accumulated input is authoritative.
        self.run_spec.model_config.previous_response_id = None;
        let initial_input_items = self.run_spec.current_input_items.clone();
        self.run_spec.current_input_items =
            cloud_agent_trigger_input_items(cloud_run, trigger, initial_input_items)?;
        if !matches!(
            trigger,
            chatos_cloud_agent_runtime::CloudAgentModelTrigger::RunStarted { .. }
        ) {
            self.run_spec.user_record = None;
        }
        Ok(self)
    }

    pub(crate) async fn persist_external_tool_results(
        &self,
        calls: &[Value],
        results: &[Value],
    ) -> Result<Vec<chatos_mcp_runtime::ToolResult>, String> {
        if calls.len() != results.len() {
            return Err("MCP aggregate result count does not match pending tool calls".to_string());
        }
        let tool_results = calls
            .iter()
            .zip(results)
            .enumerate()
            .map(|(index, (call, result))| {
                let tool_call_id = chatos_ai_runtime::tool_call::extract_tool_call_id(call)
                    .ok_or_else(|| format!("pending tool call {index} has no call id"))?;
                let name = chatos_ai_runtime::tool_call::extract_tool_call_name(call)
                    .ok_or_else(|| format!("pending tool call {index} has no name"))?;
                let success = result.get("status").and_then(Value::as_str) == Some("completed");
                if success {
                    Ok(
                        chatos_mcp_runtime::execution::external_tool_result_from_value(
                            tool_call_id.to_string(),
                            name.to_string(),
                            Some(self.run_spec.run_id.clone()),
                            result.get("result").unwrap_or(&Value::Null),
                            None,
                        ),
                    )
                } else {
                    let content = result
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("MCP tool call failed")
                        .to_string();
                    Ok(chatos_mcp_runtime::ToolResult {
                        tool_call_id: tool_call_id.to_string(),
                        name: name.to_string(),
                        success: false,
                        is_error: true,
                        is_stream: false,
                        conversation_turn_id: Some(self.run_spec.run_id.clone()),
                        content,
                        result: None,
                        fatal_error: false,
                        transient_model_input: None,
                    })
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        let batch_identity = calls
            .first()
            .and_then(chatos_ai_runtime::tool_call::extract_tool_call_id)
            .unwrap_or("empty");
        let mut options = self.runtime_options.clone();
        options.record_options =
            options
                .record_options
                .clone()
                .with_tool_message_id_prefix(format!(
                    "task-run:{}:mcp-batch:{batch_identity}",
                    self.run_spec.run_id
                ));
        self.runtime
            .runner()
            .persist_external_tool_results(&options, tool_results.as_slice())
            .await?;
        Ok(tool_results)
    }

    pub(crate) fn automatic_file_write_recovery_calls(
        &self,
        tool_results: &[chatos_mcp_runtime::ToolResult],
    ) -> Result<Vec<Value>, String> {
        let available_tools = self
            .runtime
            .mcp_executor()
            .map(|executor| executor.available_tools())
            .unwrap_or_default();
        chatos_ai_runtime::automatic_file_write_recovery_calls(
            tool_results,
            available_tools.as_slice(),
        )
    }

    fn restore_durable_state(&self, input: &Value) -> Result<(), String> {
        if let Some(value) = input.get("lifecycle") {
            *self.lifecycle_state.lock() = serde_json::from_value(value.clone())
                .map_err(|error| format!("decode Task Runner lifecycle state failed: {error}"))?;
        }
        if let Some(value) = input.get("supply_chain") {
            *self.supply_chain_evidence.lock() =
                serde_json::from_value(value.clone()).map_err(|error| {
                    format!("decode Task Runner supply-chain state failed: {error}")
                })?;
        }
        if let Some(value) = input.get("progress") {
            let snapshot = serde_json::from_value(value.clone())
                .map_err(|error| format!("decode Task Runner progress state failed: {error}"))?;
            self.progress.restore_snapshot(&snapshot);
        }
        Ok(())
    }

    pub(crate) async fn execute(
        self,
        iteration: usize,
        reason: String,
        model_attempt: usize,
    ) -> Result<AiSingleStepOutcome, String> {
        self.agent
            .execute_once_with_runtime_options(
                self.run_spec,
                &self.runtime,
                self.runtime_options,
                iteration,
                reason,
                model_attempt,
            )
            .await
    }
}

#[cfg(test)]
mod cloud_trigger_tests {
    #[test]
    fn cloud_event_driven_model_config_uses_durable_full_input() {
        let mut config = chatos_ai_runtime::ModelRuntimeConfig::openai_compatible(
            "https://api.openai.com/v1",
            "secret",
            "gpt-test",
            "openai",
        )
        .with_previous_response_id(Some("resp-2".to_string()));
        config.previous_response_id = None;
        assert_eq!(config.previous_response_id, None);
    }
}
