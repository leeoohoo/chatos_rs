// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use chatos_agent::{
    TaskRunnerAgent, TaskRunnerRunSpecInput, TASK_RUNNER_AGENT, TASK_RUNNER_PLAN_AGENT,
};
use chatos_ai_runtime::{
    AiRuntimeOptions, AiTurnReport, MemoryRecordScope, MemoryScope, RuntimeCallbacks,
    TaskExecutionReviewPolicy, TaskFinalizationLifecycleHook, TaskMemoryRuntimeConfig,
    TaskRunReport, TaskRunSpec, TaskRuntime, TaskRuntimeConfig, ToolResultModelBudgetLimits,
    DEFAULT_TASK_RUN_MAX_ITERATIONS,
};
use chatos_mcp_management_sdk::McpManagementRuntimeSessionHandle;
use chatos_mcp_runtime::{BuiltinMcpPromptLocale, McpExecutorBuilder};
use memory_engine_sdk::ComposeContextPolicy;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::models::{
    now_rfc3339, ModelConfigRecord, StartTaskRunRequest, TaskRecord, TaskRunEventRecord,
    TaskRunRecord, TaskRunStatus, TaskStatus,
};
use crate::services::TaskRunnerCapabilityPolicy;

use super::harness_run_git::{HarnessRunContext, HarnessRunOutputReport};
use super::plugin_runtime_relay::{
    cancel_prepared_plugin_sessions, dispatch_prepared_plugin_hooks, PreparedPluginSession,
};
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

mod callbacks;
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
    tool_result_model_budget_limits: ToolResultModelBudgetLimits,
    sandbox_context: Option<crate::services::sandbox_runtime::SandboxRuntimeContext>,
    harness_run_context: Option<HarnessRunContext>,
    effective_workspace_dir: String,
    plugin_sessions: Vec<PreparedPluginSession>,
}

impl RunService {
    pub(super) async fn execute_run_model_phase(
        &self,
        task: TaskRecord,
        model_config: ModelConfigRecord,
        mut run: TaskRunRecord,
        input: StartTaskRunRequest,
        effective_workspace_dir: String,
        prerequisite_context: Vec<PrerequisiteTaskContext>,
        capability_policy: Option<TaskRunnerCapabilityPolicy>,
    ) {
        let authoritative_policy = capability_policy.is_some();
        self.log_run_model_phase_start(
            &task,
            &model_config,
            &run,
            &input,
            effective_workspace_dir.as_str(),
        );
        if !self
            .initialize_model_phase(
                &task,
                &mut run,
                effective_workspace_dir.as_str(),
                &prerequisite_context,
                authoritative_policy,
            )
            .await
        {
            return;
        }

        let prepared_execution = match self
            .prepare_model_execution(
                &task,
                &model_config,
                &mut run,
                &input,
                effective_workspace_dir.as_str(),
                &prerequisite_context,
                capability_policy.as_ref(),
            )
            .await
        {
            Ok(execution) => execution,
            Err(err) => {
                self.finish_failed_before_execution(
                    &task,
                    &mut run,
                    effective_workspace_dir.as_str(),
                    err,
                )
                .await;
                return;
            }
        };

        let sandbox_context = prepared_execution.sandbox_context.clone();
        let harness_run_context = prepared_execution.harness_run_context.clone();
        let plugin_sessions = prepared_execution.plugin_sessions.clone();
        let finalized_workspace_dir = prepared_execution.effective_workspace_dir.clone();
        let mut report = self
            .execute_prepared_model_run(&task, &run, &model_config, prepared_execution)
            .await;
        let (hook_event, hook_terminal_outcome) = plugin_hook_terminal_state(&report);
        let hook_outcome = dispatch_prepared_plugin_hooks(
            plugin_sessions.as_slice(),
            hook_event,
            &chatos_plugin_management_sdk::PluginHookEventContext {
                agent_key: Some(
                    crate::models::task_runner_agent_key_for(
                        task.task_profile.as_str(),
                        task.mcp_config.requires_execution,
                    )
                    .as_str()
                    .to_string(),
                ),
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
                Some(json!({
                    "event": hook_event,
                    "blocking_failure": true,
                })),
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
        let sandbox_output = if let Some(context) = sandbox_context.as_ref() {
            self.release_sandbox(&run, context).await
        } else {
            None
        };
        if !self.run_claim_is_current(&run).await {
            warn!(
                run_id = run.id.as_str(),
                task_id = task.id.as_str(),
                "task runner stopped stale execution before committing output"
            );
            if let Some(context) = harness_run_context.as_ref() {
                self.cleanup_harness_run_workspace(context);
            }
            self.clear_local_run_abort(run.id.as_str());
            return;
        }
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
            finalized_workspace_dir.as_str(),
            sandbox_output,
            harness_output,
        )
        .await;
        if let Some(context) = harness_run_context.as_ref() {
            self.cleanup_harness_run_workspace(context);
        }
        if harness_merge_conflict {
            self.retry_after_harness_merge_conflict(&task, &run).await;
        } else if sandbox_infrastructure_failure {
            self.retry_after_sandbox_infrastructure_failure(&task, &run)
                .await;
        }
    }

    async fn retry_after_sandbox_infrastructure_failure(
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

    async fn retry_after_harness_merge_conflict(&self, task: &TaskRecord, run: &TaskRunRecord) {
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

fn plugin_hook_terminal_state(
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

fn is_sandbox_infrastructure_failure(error: &str) -> bool {
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
