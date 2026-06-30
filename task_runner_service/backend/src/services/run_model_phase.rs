use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use chatos_ai_runtime::{
    AiRuntimeOptions, AiTurnReport, MemoryRecordScope, MemoryScope, RuntimeCallbacks,
    RuntimeRecordOptions, SaveRecordInput, TaskMemoryRuntimeConfig, TaskRunExecution,
    TaskRunReport, TaskRunSpec, TaskRuntime, TaskRuntimeConfig, ToolResultModelBudgetLimits,
};
use chatos_mcp_runtime::{
    builtin_servers_from_kinds, BuiltinMcpPromptLocale, BuiltinMcpServerOptions,
    McpExecutorBuilder, McpHttpServer, McpStdioServer,
};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::models::{
    now_rfc3339, ModelConfigRecord, StartTaskRunRequest, TaskRecord, TaskRunEventRecord,
    TaskRunRecord, TaskRunStatus, TaskStatus,
};

use super::prerequisite_context::{
    attach_prerequisite_context_to_run, build_task_prompt, PrerequisiteTaskContext,
};
use super::stream_events::{
    append_pending_stream_event, flush_pending_stream_event, PendingRunStreamEvent,
};
use super::task_process_log::{
    task_process_log_builtin_server, task_process_log_prefixed_input_items,
    task_process_logging_enabled, TaskProcessLogBuiltinProvider,
    TASK_PROCESS_LOG_INTERNAL_SERVER_NAME,
};
use super::workspace_mcp::runtime_selected_builtin_kinds;
use super::{
    build_builtin_registry_with_project_management_options, summarized_report_content,
    unfinished_subtasks_error, unfinished_subtasks_for_task, DisabledBuiltinProvider,
    ProjectManagementExecutionOptions, RunService, TaskService,
};

mod callbacks;
mod completion;
mod setup;

const PROJECT_MANAGEMENT_MCP_SERVER_NAME: &str = "project_management_service";

pub(in crate::services) struct PreparedModelExecution {
    run_spec: TaskRunSpec,
    runtime_config: TaskRuntimeConfig,
    mcp_builder: McpExecutorBuilder,
    tool_result_model_budget_limits: ToolResultModelBudgetLimits,
    sandbox_context: Option<crate::services::sandbox_runtime::SandboxRuntimeContext>,
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
    ) {
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
        let report = self
            .execute_prepared_model_run(&task, &run, &model_config, prepared_execution)
            .await;
        self.finalize_model_phase(&task, &mut run, report, effective_workspace_dir.as_str())
            .await;
        if let Some(context) = sandbox_context.as_ref() {
            self.release_cloud_sandbox(&run, context).await;
        }
    }
}
