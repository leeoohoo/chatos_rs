// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use chatos_ai_runtime::ToolResultModelBudgetLimits;
use chatos_cloud_agent_runtime::CloudAgentStateStore;
use chatos_mcp_runtime::BuiltinMcpPromptLocale;
use chatos_plugin_management_sdk::PluginManagementClient;
use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, Mutex as AsyncMutex, OwnedMutexGuard};
use tracing::info;
use uuid::Uuid;

use crate::ask_user_prompt_service::AskUserPromptService;
use crate::auth::CurrentUser;
use crate::config::AppConfig;
use crate::models::{
    normalize_project_id, now_rfc3339, BatchTaskDeleteRequest, BatchTaskOperationItem,
    BatchTaskOperationResponse, BatchTaskRunRequest, BatchTaskStatusUpdateRequest,
    CancelTaskRequest, CancelTaskResponse, ChatosProjectImportRequest, CreateTaskProjectRequest,
    CreateTaskRequest, HealthResponse, PaginatedResponse, RecordTaskProcessRequest,
    RunEventPruneResult, RunListFilters, RunSummaryRecord, RuntimeSettingsRecord,
    StartTaskRunRequest, SystemConfigResponse, TaskClosureState, TaskIndexResponse,
    TaskListFilters, TaskMcpConfig, TaskMcpResolutionResponse, TaskProjectRecord,
    TaskProjectStatus, TaskRecord, TaskRunEventRecord, TaskRunRecord, TaskRunStatus,
    TaskRunnerInternalPromptPreviewResponse, TaskScheduleMode, TaskSourceContext,
    TaskStatsResponse, TaskStatus, TaskSummaryRecord, TaskToolState, UpdateRuntimeSettingsRequest,
    UpdateTaskProjectRequest, UpdateTaskRequest, PUBLIC_PROJECT_ID,
};
use crate::platform_queue::TaskQueueTopology;
use crate::store::AppStore;

pub(crate) const MCP_RUN_FINALIZATION_ERROR_PREFIX: &str = "MCP runtime run finalization failed";
pub(crate) const CLOUD_AGENT_DEPENDENCY_WAITING: &str = "cloud_agent_dependency_waiting";
pub(crate) const WORKSPACE_INTEGRATION_RETRY_PREFIX: &str = "workspace integration retry";

mod batch_ops;
mod builtin_providers;
mod chatos_async_dispatch;
mod chatos_callbacks;
mod chatos_message_tasks;
mod filter_sanitize;
mod managed_config;
#[path = "services/tool_runtime/mcp_catalog_service.rs"]
mod mcp_catalog_service;
#[path = "services/tool_runtime/mcp_resolution.rs"]
mod mcp_resolution;
mod memory_options;
mod model_catalog;
mod model_config_service;
mod model_runtime_resolver;
pub(crate) mod path_redaction;
#[path = "services/tool_runtime/plugin_management_policy.rs"]
mod plugin_management_policy;
mod plugin_management_prompts;
mod prerequisite_context;
mod process_log_text;
pub(crate) mod project_management_api_client;
mod project_service;
mod run_control;
pub(crate) use run_control::cloud_agent_profile;
mod run_execution_support;
mod run_model_phase;
mod run_post_process;
mod run_prerequisites;
mod run_recovery;
mod run_service;
mod schedule_helpers;
mod status_display;
mod stream_events;
mod task_dependencies;
mod task_manager_lifecycle;
mod task_memory;
mod task_process_log;
mod task_service;
mod task_tenant_scope;
mod task_threads;
mod tooling_state;
mod verification_repair;
mod workspace_execution;
pub(crate) use workspace_execution::load_task_run_workspace_changes;
#[path = "services/tool_runtime/workspace_mcp.rs"]
mod workspace_mcp;

use self::batch_ops::{
    normalize_batch_task_ids, normalize_prerequisite_task_ids, normalize_tags, sanitize_id_list,
    summarize_batch_results,
};
pub use self::chatos_callbacks::{
    spawn_chatos_callback_queue_consumer, spawn_chatos_callback_reconciler,
};
pub use self::chatos_message_tasks::{
    ChatosActiveMessageTaskSource, ChatosMessageModelConfigSummary, ChatosMessageRunDetail,
    ChatosMessageTaskDetail, ChatosMessageTaskGraph, ChatosMessageTaskGraphEdge,
    ChatosMessageTaskGraphNode, ChatosMessageTaskRun, ChatosMessageTaskRunEvent,
    ChatosMessageTaskRunSummary, ChatosMessageTaskSummary,
};
pub(crate) use self::filter_sanitize::sanitize_prompt_list_filters;
use self::filter_sanitize::{sanitize_run_list_filters, sanitize_task_list_filters};
use self::managed_config::{
    load_managed_config_snapshot, require_managed_string, require_managed_string_map,
    require_managed_string_set, require_managed_u64, require_managed_usize,
    TASK_RUNNER_EXECUTION_TIMEOUT_CONFIG_KEY,
    TASK_RUNNER_SUPPLY_CHAIN_BASELINE_REVISION_CONFIG_KEY,
    TASK_RUNNER_SUPPLY_CHAIN_INSTALL_SCRIPT_ALLOWLIST_CONFIG_KEY,
    TASK_RUNNER_SUPPLY_CHAIN_NODE_AUDIT_LEVEL_CONFIG_KEY,
    TASK_RUNNER_SUPPLY_CHAIN_NODE_AUDIT_REGISTRY_CONFIG_KEY,
    TASK_RUNNER_SUPPLY_CHAIN_NODE_DEPENDENCY_REQUIREMENTS_CONFIG_KEY,
    TASK_RUNNER_SUPPLY_CHAIN_NODE_INSTALL_REGISTRY_CONFIG_KEY,
    TASK_RUNNER_TOOL_RESULTS_TOTAL_MAX_CHARS_CONFIG_KEY,
    TASK_RUNNER_TOOL_RESULT_MAX_CHARS_CONFIG_KEY,
};
pub(crate) use self::plugin_management_policy::TaskRunnerCapabilityPolicy;
use self::process_log_text::apply_task_process_log_update;
use self::schedule_helpers::{advance_task_schedule_after_dispatch, sanitize_task_schedule_config};
use self::status_display::{TaskScheduleModeExt, TaskStatusExt};
use self::task_tenant_scope::{
    align_task_tenant_to_owner, resolve_task_tenant_id, save_task_if_tenant_aligned,
};
use self::workspace_mcp::{
    ensure_workspace_dir_available, sanitize_task_mcp_config, task_mcp_resolution_response,
};
pub use crate::models::RunExecutionStats;

const TASK_PROCESS_LOG_MAX_CHARS: usize = 200_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunTriggerSource {
    Manual,
    Scheduler,
    Retry,
    AutomaticRetry,
    Dependency,
}

#[derive(Clone)]
pub struct TaskService {
    config: AppConfig,
    store: AppStore,
    plugin_management_client: Option<PluginManagementClient>,
}

#[derive(Debug, Clone)]
pub(crate) struct ClonedProjectExecutionTask {
    pub(crate) old_task_id: String,
    pub(crate) project_task_id: String,
    pub(crate) task: TaskRecord,
}

#[derive(Clone)]
pub struct ModelConfigService {
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
    task_queue_topology: TaskQueueTopology,
    store: AppStore,
    plugin_management_client: Option<PluginManagementClient>,
    ask_user_prompt_service: AskUserPromptService,
    runtime_stats: crate::state::TaskRunnerRuntimeStats,
    cloud_agent_store: CloudAgentStateStore,
    start_locks: Arc<KeyedAsyncLockRegistry>,
    callback_delivery_locks: Arc<KeyedAsyncLockRegistry>,
    runtime_abort_tokens:
        Arc<parking_lot::Mutex<HashMap<String, tokio_util::sync::CancellationToken>>>,
}

impl RunService {
    pub(crate) fn worker_concurrency(&self) -> usize {
        self.config.worker_concurrency.max(1)
    }
}

#[derive(Default)]
struct KeyedAsyncLockRegistry {
    locks: parking_lot::Mutex<HashMap<String, Weak<AsyncMutex<()>>>>,
}

pub(crate) struct KeyedAsyncLockHandle {
    key: String,
    lock: Arc<AsyncMutex<()>>,
    registry: Arc<KeyedAsyncLockRegistry>,
}

pub(crate) struct KeyedAsyncLockGuard {
    _guard: OwnedMutexGuard<()>,
    _handle: KeyedAsyncLockHandle,
}

impl KeyedAsyncLockRegistry {
    fn handle(self: &Arc<Self>, key: &str) -> KeyedAsyncLockHandle {
        let mut locks = self.locks.lock();
        locks.retain(|_, lock| lock.strong_count() > 0);
        let lock = locks.get(key).and_then(Weak::upgrade).unwrap_or_else(|| {
            let lock = Arc::new(AsyncMutex::new(()));
            locks.insert(key.to_string(), Arc::downgrade(&lock));
            lock
        });
        KeyedAsyncLockHandle {
            key: key.to_string(),
            lock,
            registry: Arc::clone(self),
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.locks.lock().len()
    }
}

impl KeyedAsyncLockHandle {
    pub(crate) async fn lock_owned(self) -> KeyedAsyncLockGuard {
        let guard = Arc::clone(&self.lock).lock_owned().await;
        KeyedAsyncLockGuard {
            _guard: guard,
            _handle: self,
        }
    }
}

impl Drop for KeyedAsyncLockHandle {
    fn drop(&mut self) {
        let mut locks = self.registry.locks.lock();
        let remove = locks.get(self.key.as_str()).is_some_and(|current| {
            Weak::ptr_eq(current, &Arc::downgrade(&self.lock)) && Arc::strong_count(&self.lock) == 1
        });
        if remove {
            locks.remove(self.key.as_str());
        }
    }
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
    task_queue_topology: &TaskQueueTopology,
    execution_timeout_ms: u64,
    task_runner_runtime_settings: chatos_agent::TaskRunnerRuntimeSettings,
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
        worker_claim_ttl_ms: config.worker_claim_ttl.as_millis() as u64,
        worker_concurrency: config.worker_concurrency,
        auto_memory_summary: config.auto_memory_summary,
        default_task_execution_max_iterations: config.default_task_execution_max_iterations,
        task_execution_max_iterations: task_runner_runtime_settings.max_iterations,
        task_runner_review_read_only_iterations: task_runner_runtime_settings
            .review_read_only_iterations,
        task_runner_review_missing_read_failures: task_runner_runtime_settings
            .review_missing_read_failures,
        task_runner_review_repeat_interval_iterations: task_runner_runtime_settings
            .review_repeat_interval_iterations,
        default_tool_result_model_max_chars: config.default_tool_result_model_max_chars,
        tool_result_model_max_chars: tool_result_model_budget_limits.per_result_max_chars,
        default_tool_results_model_total_max_chars: config
            .default_tool_results_model_total_max_chars,
        tool_results_model_total_max_chars: tool_result_model_budget_limits.total_max_chars,
        task_queue_rabbitmq_enabled: task_queue_topology.uses_rabbitmq(),
        task_queue_callback_delivery_mode: task_queue_topology
            .callback_delivery_mode
            .as_str()
            .to_string(),
        task_queue_run_events_publish_mode: task_queue_topology
            .run_events_publish_mode
            .as_str()
            .to_string(),
        task_queue_rabbitmq_exchange: task_queue_topology.rabbitmq_exchange.clone(),
        task_queue_event_outbox_reconcile_ms: task_queue_topology
            .event_outbox_reconcile_interval
            .as_millis() as u64,
        task_queue_event_outbox_batch_size: task_queue_topology.event_outbox_batch_size,
        task_queue_worker_control_queue_prefix: task_queue_topology
            .worker_control_queue_prefix
            .clone(),
        task_queue_callback_delivery_queue: task_queue_topology.callback_delivery_queue.clone(),
        task_queue_run_events_routing_key: task_queue_topology.run_events_routing_key.clone(),
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
        .filter(|subtask| {
            subtask
                .task_tool_state
                .required_for_parent_completion
                .unwrap_or(false)
                && subtask.task_tool_state.closure_state == Some(TaskClosureState::Open)
        })
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
            "The process-log system message is injected only when MCP stays enabled and the Task Process Log MCP is enabled by Plugin Management for the task run.".to_string(),
            "Builtin MCP system prompt content is shown separately and follows the same prompt-language setting.".to_string(),
        ]
    } else {
        vec![
            "前置任务结果段只会在任务配置了前置任务时注入。".to_string(),
            "任务说明和输入数据两段只有当前任务存在对应值时才会出现。".to_string(),
            "任务主 prompt 会要求执行方先理解真实链路、优先复用已有代码或平台能力，并留下最小但有用的验证证据。".to_string(),
            "全局执行 prompt 会在运行时追加到当前任务 prompt 后面，这里单独展示以便核对。".to_string(),
            "过程日志系统提示只会在该次任务运行启用 MCP，且配置中心为该 Task Runner Agent 启用 Task Process Log MCP 时注入。".to_string(),
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
    use super::{
        load_managed_config_snapshot, require_managed_string_map, KeyedAsyncLockRegistry,
        TASK_RUNNER_SUPPLY_CHAIN_NODE_DEPENDENCY_REQUIREMENTS_CONFIG_KEY,
    };
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use uuid::Uuid;

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{prefix}_{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[tokio::test]
    async fn dependency_baseline_requires_a_non_empty_managed_string_map() {
        let mut snapshot = load_managed_config_snapshot().await.expect("snapshot");
        let requirements = require_managed_string_map(
            &snapshot,
            TASK_RUNNER_SUPPLY_CHAIN_NODE_DEPENDENCY_REQUIREMENTS_CONFIG_KEY,
        )
        .expect("managed requirements");
        assert_eq!(requirements["react"], "^19.2.7");

        snapshot.values.insert(
            TASK_RUNNER_SUPPLY_CHAIN_NODE_DEPENDENCY_REQUIREMENTS_CONFIG_KEY.to_string(),
            json!({}),
        );
        assert!(require_managed_string_map(
            &snapshot,
            TASK_RUNNER_SUPPLY_CHAIN_NODE_DEPENDENCY_REQUIREMENTS_CONFIG_KEY,
        )
        .expect_err("empty baseline must fail")
        .contains("must not be empty"));

        snapshot.values.insert(
            TASK_RUNNER_SUPPLY_CHAIN_NODE_DEPENDENCY_REQUIREMENTS_CONFIG_KEY.to_string(),
            json!({"react": 19}),
        );
        assert!(require_managed_string_map(
            &snapshot,
            TASK_RUNNER_SUPPLY_CHAIN_NODE_DEPENDENCY_REQUIREMENTS_CONFIG_KEY,
        )
        .expect_err("non-string requirement must fail")
        .contains("non-empty version requirements"));
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

    #[tokio::test]
    async fn keyed_async_lock_serializes_same_key_and_removes_idle_entry() {
        let registry = Arc::new(KeyedAsyncLockRegistry::default());
        let first_guard = registry.handle("task-1").lock_owned().await;
        let second_handle = registry.handle("task-1");
        assert_eq!(registry.len(), 1);

        let second = tokio::spawn(async move {
            let _guard = second_handle.lock_owned().await;
        });
        tokio::task::yield_now().await;
        assert!(!second.is_finished());

        drop(first_guard);
        second.await.expect("second keyed lock task");
        assert_eq!(registry.len(), 0);
    }
}
