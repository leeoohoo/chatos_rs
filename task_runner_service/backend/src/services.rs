use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chatos_ai_runtime::ToolResultModelBudgetLimits;
use chatos_mcp_runtime::BuiltinMcpPromptLocale;
use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, Mutex as AsyncMutex};
use tracing::info;
use uuid::Uuid;

use crate::ask_user_prompt_service::AskUserPromptService;
use crate::auth::CurrentUser;
use crate::config::AppConfig;
use crate::models::{
    normalize_project_id, now_rfc3339, BatchTaskDeleteRequest, BatchTaskOperationItem,
    BatchTaskOperationResponse, BatchTaskRunRequest, BatchTaskStatusUpdateRequest,
    CancelTaskRequest, CancelTaskResponse, ChatosProjectImportRequest,
    CreateExternalMcpConfigRequest, CreateTaskProjectRequest, CreateTaskRequest,
    ExternalMcpConfigRecord, HealthResponse, PaginatedResponse, RecordTaskProcessRequest,
    RunListFilters, RunSummaryRecord, RuntimeSettingsRecord, StartTaskRunRequest,
    SystemConfigResponse, TaskIndexResponse, TaskListFilters, TaskMcpConfig, TaskProjectRecord,
    TaskProjectStatus, TaskRecord, TaskRunEventRecord, TaskRunRecord, TaskRunStatus,
    TaskRunnerInternalPromptPreviewResponse, TaskSourceContext, TaskStatsResponse, TaskStatus,
    TaskSummaryRecord, TaskToolState, UpdateExternalMcpConfigRequest, UpdateRuntimeSettingsRequest,
    UpdateTaskMcpRequest, UpdateTaskProjectRequest, UpdateTaskRequest, PUBLIC_PROJECT_ID,
};
use crate::store::AppStore;

mod batch_ops;
mod builtin_providers;
mod chatos_callbacks;
mod chatos_message_tasks;
mod external_mcp_config_service;
mod filter_sanitize;
mod mcp_catalog_service;
mod memory_options;
mod model_catalog;
mod model_config_service;
mod prerequisite_context;
mod process_log_text;
mod project_management_api_client;
mod project_service;
mod remote_server_service;
mod remote_servers;
mod run_control;
mod run_execution_support;
mod run_model_phase;
mod run_prerequisites;
mod run_recovery;
mod run_service;
mod schedule_helpers;
mod status_display;
mod stream_events;
mod task_dependencies;
mod task_manager_bridge;
mod task_memory;
mod task_process_log;
mod task_service;
mod task_tenant_scope;
mod task_threads;
mod terminal_lifecycle;
mod tooling_state;
mod workspace_mcp;

use self::batch_ops::{
    normalize_batch_task_ids, normalize_prerequisite_task_ids, normalize_tags, sanitize_id_list,
    summarize_batch_results,
};
use self::builtin_providers::{build_builtin_registry, DisabledBuiltinProvider};
pub use self::chatos_message_tasks::{
    ChatosActiveMessageTaskSource, ChatosMessageModelConfigSummary, ChatosMessageRunDetail,
    ChatosMessageTaskDetail, ChatosMessageTaskGraph, ChatosMessageTaskGraphEdge,
    ChatosMessageTaskGraphNode, ChatosMessageTaskRun, ChatosMessageTaskRunEvent,
    ChatosMessageTaskRunSummary, ChatosMessageTaskSummary,
};
pub(crate) use self::filter_sanitize::sanitize_prompt_list_filters;
use self::filter_sanitize::{sanitize_run_list_filters, sanitize_task_list_filters};
use self::process_log_text::apply_task_process_log_update;
use self::remote_servers::build_remote_server_record;
use self::schedule_helpers::{advance_task_schedule_after_dispatch, sanitize_task_schedule_config};
use self::status_display::{TaskScheduleModeExt, TaskStatusExt};
use self::task_tenant_scope::{
    align_task_tenant_to_owner, resolve_task_tenant_id, save_task_if_tenant_aligned,
};
use self::workspace_mcp::{
    ensure_workspace_dir_available, normalize_builtin_kind_names, sanitize_task_mcp_config,
};

const RUN_CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(300);
const TASK_PROCESS_LOG_MAX_CHARS: usize = 200_000;
const SYSTEM_RUNTIME_SETTINGS_ID: &str = "system";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunTriggerSource {
    Manual,
    Scheduler,
}

#[derive(Clone)]
pub struct TaskService {
    config: AppConfig,
    store: AppStore,
}

#[derive(Clone)]
pub struct ModelConfigService {
    store: AppStore,
}

#[derive(Clone)]
pub struct RemoteServerService {
    store: AppStore,
}

#[derive(Clone)]
pub struct ExternalMcpConfigService {
    store: AppStore,
}

#[derive(Clone)]
pub struct TaskProjectService {
    config: Option<AppConfig>,
    store: AppStore,
}

#[derive(Clone)]
pub struct RunService {
    config: AppConfig,
    store: AppStore,
    ask_user_prompt_service: AskUserPromptService,
    start_locks: Arc<parking_lot::Mutex<HashMap<String, Arc<AsyncMutex<()>>>>>,
}

#[derive(Clone)]
pub struct McpCatalogService {
    task_service: TaskService,
    ask_user_prompt_service: AskUserPromptService,
}

#[derive(Clone)]
pub struct ToolingStateService {
    config: AppConfig,
}

pub fn health() -> HealthResponse {
    HealthResponse {
        status: "ok",
        service: "task_runner_service_backend",
        now: now_rfc3339(),
    }
}

pub fn system_config(
    config: &AppConfig,
    execution_timeout_ms: u64,
    task_execution_max_iterations: usize,
    tool_result_model_budget_limits: ToolResultModelBudgetLimits,
) -> SystemConfigResponse {
    SystemConfigResponse {
        host: config.host.to_string(),
        port: config.port,
        store_mode: config.store_mode_key().to_string(),
        database_url: config.database_url.clone(),
        memory_engine_base_url: config.memory_engine_base_url.clone(),
        memory_engine_source_id: config.memory_engine_source_id.clone(),
        memory_engine_configured: config.memory_engine_base_url.is_some(),
        default_tenant_id: config.default_tenant_id.clone(),
        default_subject_id: config.default_subject_id.clone(),
        default_workspace_dir: config.default_workspace_dir.clone(),
        memory_timeout_ms: config.memory_timeout.as_millis() as u64,
        default_execution_timeout_ms: config.execution_timeout.as_millis() as u64,
        execution_timeout_ms,
        scheduler_poll_interval_ms: config.scheduler_poll_interval.as_millis() as u64,
        auto_memory_summary: config.auto_memory_summary,
        default_task_execution_max_iterations: config.default_task_execution_max_iterations,
        task_execution_max_iterations,
        default_tool_result_model_max_chars: config.default_tool_result_model_max_chars,
        tool_result_model_max_chars: tool_result_model_budget_limits.per_result_max_chars,
        default_tool_results_model_total_max_chars: config
            .default_tool_results_model_total_max_chars,
        tool_results_model_total_max_chars: tool_result_model_budget_limits.total_max_chars,
    }
}

async fn unfinished_subtasks_for_task(
    store: &AppStore,
    task: &TaskRecord,
) -> Result<Vec<TaskRecord>, String> {
    let mut subtasks = store
        .list_tasks_filtered(&TaskListFilters {
            parent_task_id: Some(task.id.clone()),
            ..TaskListFilters::default()
        })
        .await?
        .into_iter()
        .filter(|subtask| subtask.status != TaskStatus::Succeeded)
        .collect::<Vec<_>>();
    subtasks.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(subtasks)
}

fn unfinished_subtasks_error(task: &TaskRecord, subtasks: &[TaskRecord]) -> String {
    let examples = subtasks
        .iter()
        .take(5)
        .map(|subtask| {
            format!(
                "{}({})",
                subtask.title.trim(),
                subtask.status.status_string()
            )
        })
        .collect::<Vec<_>>()
        .join("、");
    let suffix = if subtasks.len() > 5 {
        format!(" 等 {} 个", subtasks.len())
    } else {
        format!(" {} 个", subtasks.len())
    };
    format!(
        "父任务「{}」还有未完成子任务{suffix}：{examples}。请先完成所有子任务，再将父任务标记为成功。",
        task.title.trim()
    )
}

async fn ensure_task_has_no_unfinished_subtasks(
    store: &AppStore,
    task: &TaskRecord,
) -> Result<(), String> {
    let unfinished = unfinished_subtasks_for_task(store, task).await?;
    if unfinished.is_empty() {
        Ok(())
    } else {
        Err(unfinished_subtasks_error(task, &unfinished))
    }
}

async fn ensure_subtask_can_be_marked_unfinished(
    store: &AppStore,
    subtask: &TaskRecord,
    status: TaskStatus,
) -> Result<(), String> {
    if status == TaskStatus::Succeeded {
        return Ok(());
    }
    let Some(parent_task_id) = subtask
        .parent_task_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let Some(parent) = store.get_task(parent_task_id).await? else {
        return Ok(());
    };
    if parent.status != TaskStatus::Succeeded {
        return Ok(());
    }
    Err(format!(
        "父任务「{}」已经成功，不能再将子任务「{}」改为 {}。",
        parent.title.trim(),
        subtask.title.trim(),
        status.status_string()
    ))
}

pub fn task_runner_internal_prompt_preview(
    locale: BuiltinMcpPromptLocale,
) -> TaskRunnerInternalPromptPreviewResponse {
    let locale_key = if locale.is_english() {
        BuiltinMcpPromptLocale::ENGLISH_KEY
    } else {
        BuiltinMcpPromptLocale::DEFAULT_KEY
    };
    let notes = if locale.is_english() {
        vec![
            "The prerequisite-task section is injected only when the task declares prerequisite tasks.".to_string(),
            "Task description and input-data sections appear only when the current task has those values.".to_string(),
            "The main task prompt asks the runner to understand the real flow, reuse existing code or platform capabilities, and leave the smallest useful verification evidence.".to_string(),
            "The global execution prompt is appended to the current task prompt during execution and is shown separately here for clarity.".to_string(),
            "The process-log system message is injected only when MCP stays enabled for the task run.".to_string(),
            "Builtin MCP system prompt content is shown separately and follows the same prompt-language setting.".to_string(),
        ]
    } else {
        vec![
            "前置任务结果段只会在任务配置了前置任务时注入。".to_string(),
            "任务说明和输入数据两段只有当前任务存在对应值时才会出现。".to_string(),
            "任务主 prompt 会要求执行方先理解真实链路、优先复用已有代码或平台能力，并留下最小但有用的验证证据。".to_string(),
            "全局执行 prompt 会在运行时追加到当前任务 prompt 后面，这里单独展示以便核对。".to_string(),
            "过程日志系统提示只会在该次任务运行保持启用 MCP 时注入。".to_string(),
            "Builtin MCP system prompt 会单独展示，并跟随同一个 prompt 语言设置。".to_string(),
        ]
    };
    TaskRunnerInternalPromptPreviewResponse {
        locale: locale_key.to_string(),
        task_prompt_template: prerequisite_context::build_task_prompt_template(locale),
        global_execution_prompt: prerequisite_context::build_global_execution_prompt(locale),
        process_log_system_prompt: task_process_log::task_process_log_preview_text(locale),
        notes,
    }
}

fn is_terminal_run_status(status: TaskRunStatus) -> bool {
    matches!(
        status,
        TaskRunStatus::Succeeded
            | TaskRunStatus::Failed
            | TaskRunStatus::Cancelled
            | TaskRunStatus::Blocked
    )
}

fn summarized_report_content(content: &Option<String>) -> Option<String> {
    content
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_strings(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn normalized_optional_nested(value: Option<String>) -> Option<String> {
    normalized_optional(value)
}

fn validate_required(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{label} 不能为空"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::workspace_mcp::{ensure_workspace_dir_available, resolve_workspace_dir_with_base};
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{prefix}_{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn resolve_workspace_dir_with_base_joins_relative_path() {
        let base = make_temp_dir("task_runner_workspace_base");
        let resolved = resolve_workspace_dir_with_base(
            base.to_string_lossy().as_ref(),
            Some("nested/project"),
        );
        assert_eq!(PathBuf::from(resolved), base.join("nested/project"));
    }

    #[test]
    fn ensure_workspace_dir_available_creates_missing_relative_dir() {
        let base = make_temp_dir("task_runner_workspace_create");
        let expected = base.join("nested/project");

        let ensured =
            ensure_workspace_dir_available(base.to_string_lossy().as_ref(), Some("nested/project"))
                .expect("ensure workspace dir");

        assert!(expected.is_dir());
        assert_eq!(
            PathBuf::from(ensured),
            expected.canonicalize().unwrap_or(expected)
        );
    }

    #[test]
    fn ensure_workspace_dir_available_rejects_file_path() {
        let base = make_temp_dir("task_runner_workspace_file");
        let file_path = base.join("not_a_dir.txt");
        fs::write(&file_path, "hello").expect("write temp file");

        let err =
            ensure_workspace_dir_available(base.to_string_lossy().as_ref(), Some("not_a_dir.txt"))
                .expect_err("file path should be rejected");

        assert!(err.contains("工作目录不是目录"));
    }
}
