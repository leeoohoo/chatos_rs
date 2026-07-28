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
use chatos_mcp_runtime::{
    builtin_servers_from_kinds, BuiltinMcpPromptLocale, BuiltinMcpServerOptions,
    McpExecutorBuilder, McpHttpServer, McpStdioServer,
};
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
    task_process_log_builtin_server, task_process_log_prefixed_input_items,
    task_process_logging_enabled, TaskProcessLogBuiltinProvider,
    TASK_PROCESS_LOG_INTERNAL_SERVER_NAME,
};
use super::workspace_mcp::{
    runtime_selected_builtin_kinds, runtime_selected_builtin_kinds_authoritative,
    task_uses_harness_code,
};
use super::{
    build_builtin_registry, summarized_report_content, unfinished_subtasks_error,
    DisabledBuiltinProvider, RunService, TaskService,
};

mod callbacks;
mod completion;
mod setup;

pub(in crate::services) struct PreparedModelExecution {
    agent: TaskRunnerAgent,
    run_spec: TaskRunSpec,
    runtime_config: TaskRuntimeConfig,
    mcp_builder: McpExecutorBuilder,
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
        let hook_event = if report.status == chatos_ai_runtime::AiTurnStatus::Completed {
            chatos_plugin_management_sdk::PluginHookEvent::RunCompleted
        } else {
            chatos_plugin_management_sdk::PluginHookEvent::RunFailed
        };
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
                outcome: Some(match report.status {
                    chatos_ai_runtime::AiTurnStatus::Completed => {
                        chatos_plugin_management_sdk::PluginHookOutcome::Succeeded
                    }
                    chatos_ai_runtime::AiTurnStatus::Failed => {
                        chatos_plugin_management_sdk::PluginHookOutcome::Failed
                    }
                    chatos_ai_runtime::AiTurnStatus::Aborted => {
                        chatos_plugin_management_sdk::PluginHookOutcome::Cancelled
                    }
                }),
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
    }
}
