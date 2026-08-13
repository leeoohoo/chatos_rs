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
use chatos_cloud_agent_runtime::cloud_agent_mcp_result_input_items;
use chatos_mcp_management_sdk::McpManagementRuntimeSessionHandle;
use chatos_mcp_runtime::{BuiltinMcpPromptLocale, McpExecutorBuilder};
use memory_engine_sdk::ComposeContextPolicy;
use serde_json::{json, Value};
use tracing::warn;

use super::harness_run_git::HarnessRunOutputReport;
use super::plugin_runtime_relay::PreparedPluginSession;
use super::prerequisite_context::{
    attach_prerequisite_context_to_run, build_task_prompt, PrerequisiteTaskContext,
};
use super::sandbox_runtime::SandboxOutputReport;
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

const HARNESS_MERGE_CONFLICT_MAX_RUNS: usize = 3;
const SANDBOX_INFRASTRUCTURE_MAX_RETRIES: usize = 3;

pub(in crate::services) struct PreparedModelExecution {
    agent: TaskRunnerAgent,
    run_spec: TaskRunSpec,
    runtime_config: TaskRuntimeConfig,
    mcp_builder: McpExecutorBuilder,
    mcp_management_runtime_session: McpManagementRuntimeSessionHandle,
    mcp_command_queue: String,
    tool_result_model_budget_limits: ToolResultModelBudgetLimits,
    effective_workspace_dir: String,
    plugin_sessions: Vec<PreparedPluginSession>,
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
    pub(crate) plugin_sessions: Vec<PreparedPluginSession>,
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
        self.run_spec.model_config.previous_response_id = cloud_run.previous_response_id.clone();
        match trigger {
            chatos_cloud_agent_runtime::CloudAgentModelTrigger::RunStarted { .. } => {}
            chatos_cloud_agent_runtime::CloudAgentModelTrigger::Continuation {
                payload, ..
            } => {
                self.run_spec.user_record = None;
                self.run_spec.current_input_items = payload
                    .get("input_items")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
            }
            chatos_cloud_agent_runtime::CloudAgentModelTrigger::ToolResults { items, .. } => {
                self.run_spec.user_record = None;
                self.run_spec.current_input_items = cloud_agent_mcp_result_input_items(
                    cloud_run.pending_tool_calls.as_slice(),
                    items.as_slice(),
                )?;
            }
            chatos_cloud_agent_runtime::CloudAgentModelTrigger::Retry { payload, .. } => {
                self.run_spec.user_record = None;
                if let Some(items) = payload.get("input_items").and_then(Value::as_array) {
                    self.run_spec.current_input_items = items.clone();
                }
            }
        }
        Ok(self)
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

impl RunService {
    pub(in crate::services) async fn retry_after_sandbox_infrastructure_failure(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
    ) {
        let failed_run_count = match self.store.list_runs(Some(task.id.as_str())).await {
            Ok(runs) => runs
                .iter()
                .filter(|run| {
                    run.error_message
                        .as_deref()
                        .is_some_and(is_sandbox_infrastructure_failure)
                })
                .count(),
            Err(error) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    error = error.as_str(),
                    "failed to count sandbox infrastructure retries"
                );
                return;
            }
        };
        if failed_run_count > SANDBOX_INFRASTRUCTURE_MAX_RETRIES {
            let _ = self
                .store
                .append_run_event(TaskRunEventRecord::new(
                    run.id.clone(),
                    "sandbox_infrastructure_retry_exhausted",
                    Some(format!(
                        "沙箱基础设施连续失败 {failed_run_count} 次，停止自动重新执行"
                    )),
                    Some(json!({
                        "failed_run_count": failed_run_count,
                        "max_retries": SANDBOX_INFRASTRUCTURE_MAX_RETRIES,
                    })),
                ))
                .await;
            return;
        }

        match self.retry_run_automatically(run.id.as_str()).await {
            Ok(Some(retry_run)) => {
                let _ = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "sandbox_infrastructure_retry_queued",
                        Some("检测到沙箱租约失效，已由程序申请新环境并重新执行原任务".to_string()),
                        Some(json!({
                            "retry_run_id": retry_run.id,
                            "failed_run_count": failed_run_count,
                            "max_retries": SANDBOX_INFRASTRUCTURE_MAX_RETRIES,
                        })),
                    ))
                    .await;
            }
            Ok(None) => {}
            Err(error) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    error = error.as_str(),
                    "failed to queue automatic sandbox infrastructure retry"
                );
                let _ = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "sandbox_infrastructure_retry_failed",
                        Some(format!("沙箱失效后自动重新执行原任务失败: {error}")),
                        None,
                    ))
                    .await;
            }
        }
    }

    pub(in crate::services) async fn retry_after_harness_merge_conflict(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
    ) {
        let conflict_run_count = match self.store.list_runs(Some(task.id.as_str())).await {
            Ok(runs) => runs
                .iter()
                .filter(|run| run_has_harness_merge_conflict(run))
                .count(),
            Err(error) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    error = error.as_str(),
                    "failed to count Harness merge-conflict retries"
                );
                return;
            }
        };
        if conflict_run_count >= HARNESS_MERGE_CONFLICT_MAX_RUNS {
            let _ = self
                .store
                .append_run_event(TaskRunEventRecord::new(
                    run.id.clone(),
                    "harness_merge_conflict_retry_exhausted",
                    Some(format!(
                        "Harness 并发合并连续冲突 {conflict_run_count} 次，停止自动重试"
                    )),
                    Some(json!({
                        "conflict_run_count": conflict_run_count,
                        "max_conflict_runs": HARNESS_MERGE_CONFLICT_MAX_RUNS,
                    })),
                ))
                .await;
            return;
        }

        match self.retry_run_automatically(run.id.as_str()).await {
            Ok(Some(retry_run)) => {
                let _ = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "harness_merge_conflict_retry_queued",
                        Some(
                            "检测到并发合并冲突，已由程序基于最新 Harness 基线重新执行原任务"
                                .to_string(),
                        ),
                        Some(json!({
                            "retry_run_id": retry_run.id,
                            "conflict_run_count": conflict_run_count,
                            "max_conflict_runs": HARNESS_MERGE_CONFLICT_MAX_RUNS,
                        })),
                    ))
                    .await;
            }
            Ok(None) => {}
            Err(error) => {
                warn!(
                    task_id = task.id.as_str(),
                    run_id = run.id.as_str(),
                    error = error.as_str(),
                    "failed to queue automatic Harness merge-conflict retry"
                );
                let _ = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "harness_merge_conflict_retry_failed",
                        Some(format!("自动重新执行原任务失败: {error}")),
                        None,
                    ))
                    .await;
            }
        }
    }
}

fn run_has_harness_merge_conflict(run: &TaskRunRecord) -> bool {
    run.report
        .as_ref()
        .and_then(|report| report.pointer("/output/harness/status"))
        .and_then(Value::as_str)
        .is_some_and(|status| status == "merge_conflict")
}

pub(in crate::services) fn plugin_hook_terminal_state(
    report: &TaskRunReport,
) -> (
    chatos_plugin_management_sdk::PluginHookEvent,
    chatos_plugin_management_sdk::PluginHookOutcome,
) {
    use chatos_ai_runtime::{AiTurnStatus, TaskExecutionOutcomeStatus};
    use chatos_plugin_management_sdk::{PluginHookEvent, PluginHookOutcome};

    match report.status {
        AiTurnStatus::Failed => (PluginHookEvent::RunFailed, PluginHookOutcome::Failed),
        AiTurnStatus::Aborted => (PluginHookEvent::RunFailed, PluginHookOutcome::Cancelled),
        AiTurnStatus::Completed => match report
            .execution_outcome
            .as_ref()
            .map(|outcome| outcome.status)
        {
            Some(TaskExecutionOutcomeStatus::Succeeded) => {
                (PluginHookEvent::RunCompleted, PluginHookOutcome::Succeeded)
            }
            Some(TaskExecutionOutcomeStatus::Cancelled) => {
                (PluginHookEvent::RunFailed, PluginHookOutcome::Cancelled)
            }
            Some(TaskExecutionOutcomeStatus::Blocked | TaskExecutionOutcomeStatus::Failed)
            | None => (PluginHookEvent::RunFailed, PluginHookOutcome::Failed),
        },
    }
}

pub(in crate::services) fn is_sandbox_infrastructure_failure(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    (normalized.contains("sandbox manager lease is not runnable")
        && (normalized.contains("destroyed") || normalized.contains("expired")))
        || normalized.contains("sandbox infrastructure unavailable; the run must reacquire")
}

#[cfg(test)]
mod harness_merge_retry_tests {
    use super::{
        is_sandbox_infrastructure_failure, plugin_hook_terminal_state,
        run_has_harness_merge_conflict,
    };
    use crate::models::TaskRunRecord;
    use chatos_ai_runtime::{
        AiTurnStatus, TaskExecutionOutcome, TaskExecutionOutcomeStatus, TaskRunReport,
    };
    use chatos_plugin_management_sdk::{PluginHookEvent, PluginHookOutcome};
    use serde_json::json;

    #[test]
    fn detects_only_structured_harness_merge_conflicts() {
        let now = crate::models::now_rfc3339();
        let mut run = TaskRunRecord::queued(
            "run-1".to_string(),
            "task-1".to_string(),
            "model-1".to_string(),
            "memory-1".to_string(),
            json!({}),
            Vec::new(),
            now,
        );
        run.report = Some(json!({
            "output": {
                "harness": {
                    "status": "merge_conflict"
                }
            }
        }));
        assert!(run_has_harness_merge_conflict(&run));

        run.report = Some(json!({
            "output": {
                "harness": {
                    "status": "failed",
                    "message": "merge conflict text is not a structured retry signal"
                }
            }
        }));
        assert!(!run_has_harness_merge_conflict(&run));
    }

    #[test]
    fn detects_destroyed_or_expired_sandbox_infrastructure() {
        assert!(is_sandbox_infrastructure_failure(
            "Sandbox Manager lease is not runnable: destroyed"
        ));
        assert!(is_sandbox_infrastructure_failure(
            "sandbox infrastructure unavailable; the run must reacquire its sandbox"
        ));
        assert!(!is_sandbox_infrastructure_failure(
            "No such file or directory"
        ));
    }

    #[test]
    fn plugin_terminal_hook_uses_business_outcome_instead_of_protocol_completion() {
        let mut report = completed_task_report();
        report.execution_outcome = Some(TaskExecutionOutcome::succeeded(
            "verified",
            vec!["tests passed".to_string()],
        ));
        assert_eq!(
            plugin_hook_terminal_state(&report),
            (PluginHookEvent::RunCompleted, PluginHookOutcome::Succeeded)
        );

        report.execution_outcome = Some(TaskExecutionOutcome {
            status: TaskExecutionOutcomeStatus::Blocked,
            summary: "blocked".to_string(),
            blocking_reason: Some("database unavailable".to_string()),
            unmet_acceptance_criteria: vec!["integration test passes".to_string()],
            verification_evidence: vec!["connection refused".to_string()],
            referenced_paths: Vec::new(),
            referenced_endpoints: Vec::new(),
        });
        assert_eq!(
            plugin_hook_terminal_state(&report),
            (PluginHookEvent::RunFailed, PluginHookOutcome::Failed)
        );

        report.execution_outcome = None;
        assert_eq!(
            plugin_hook_terminal_state(&report),
            (PluginHookEvent::RunFailed, PluginHookOutcome::Failed)
        );
    }

    fn completed_task_report() -> TaskRunReport {
        TaskRunReport {
            task_id: "task-1".to_string(),
            run_id: "run-1".to_string(),
            model_config_id: Some("model-1".to_string()),
            status: AiTurnStatus::Completed,
            execution_outcome: None,
            content: Some("done".to_string()),
            reasoning: None,
            error: None,
            tool_calls: None,
            finish_reason: Some("stop".to_string()),
            usage: None,
            response_id: None,
            completed_at: crate::models::now_rfc3339(),
        }
    }
}
