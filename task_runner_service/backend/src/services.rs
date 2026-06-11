use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use async_trait::async_trait;
use chatos_ai_runtime::model_config::{
    default_base_url_for_provider, normalize_provider, normalize_thinking_level,
};
use chatos_ai_runtime::{
    AiRequestHandler, AiRuntimeOptions, AiTurnReport, MemoryContextComposer, MemoryRecordScope,
    MemoryScope, RuntimeCallbacks, RuntimeRecordOptions, SaveRecordInput, SimplePromptOptions,
    TaskBuiltinMcpPromptMode, TaskMemoryRuntimeConfig, TaskRunExecution, TaskRunReport,
    TaskRunSpec, TaskRuntimeConfig, build_responses_text_input, run_compatible_prompt_with,
    select_preferred_response_text,
};
use chatos_builtin_tools::{
    NotepadBuiltinService, NotepadOptions, NotepadStore, NotepadStoreRef,
    REVIEW_TIMEOUT_MS_DEFAULT, RemoteConnectionControllerOptions,
    RemoteConnectionControllerService, RemoteConnectionControllerStoreRef,
    SharedBuiltinToolService, TASK_NOT_FOUND_ERR, TaskDraft as SharedTaskDraft, TaskManagerOptions,
    TaskManagerService, TaskManagerStore, TaskManagerStoreRef,
    TaskOutcomeItem as SharedTaskOutcomeItem, TaskStreamChunkCallback,
    TaskUpdatePatch as SharedTaskUpdatePatch, TerminalControllerContext, TerminalControllerOptions,
    TerminalControllerService, TerminalControllerStore, TerminalControllerStoreRef,
    UI_PROMPT_TIMEOUT_MS_DEFAULT, UiPrompterOptions, UiPrompterService, UiPrompterStoreRef,
    build_shared_builtin_tool_service,
};
use chatos_mcp_runtime::{
    BuiltinMcpPromptLocale, BuiltinMcpServerOptions, BuiltinToolProvider, BuiltinToolRegistry,
    McpBuiltinServer, McpExecutorBuilder, ToolCallContext, ToolStreamChunkCallback,
    builtin_kind_by_any, builtin_servers_from_kinds, configurable_builtin_kinds,
    default_runtime_builtin_kinds,
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use memory_engine_sdk::{
    ComposeContextPolicy, SdkComposeContextRequest, SdkCountThreadRecordsRequest,
    SdkListThreadRecordsRequest, SdkUpsertThreadRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Mutex as AsyncMutex, broadcast};
use tracing::{info, warn};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::config::AppConfig;
use crate::models::{
    BatchTaskDeleteRequest, BatchTaskOperationItem, BatchTaskOperationResponse,
    BatchTaskRunRequest, BatchTaskStatusUpdateRequest, CreateModelConfigRequest,
    CreateRemoteServerRequest, CreateTaskRequest, HealthResponse, McpCatalogEntry,
    McpPromptPreviewRequest, McpPromptPreviewResponse, McpUnavailableTool, ModelCatalogResponse,
    ModelConfigRecord, ModelConfigTestResponse, ModelConfigUsageRecord, PaginatedResponse,
    PreviewModelCatalogRequest, PromptListFilters, ProviderModelRecord, RecordTaskProcessRequest,
    RemoteServerRecord, RemoteServerTestResponse, RunListFilters, RunSummaryRecord,
    RuntimeSettingsRecord, StartTaskRunRequest, SystemConfigResponse, TaskDependencyGraph,
    TaskIndexResponse, TaskListFilters, TaskMcpConfig, TaskMemoryContextOptions,
    TaskMemoryContextResponse, TaskMemoryRecordsOptions, TaskMemoryRecordsResponse,
    TaskMemorySummaryResponse, TaskProcessLogOperation, TaskRecord, TaskRunEventRecord,
    TaskRunRecord, TaskRunStatus, TaskScheduleConfig, TaskScheduleMode, TaskSourceContext,
    TaskStatsResponse, TaskStatus, TaskSummaryRecord, TaskToolOutcomeItem, TaskToolState,
    TestModelConfigRequest, TestRemoteServerRequest, UpdateModelConfigRequest,
    UpdateRemoteServerRequest, UpdateRuntimeSettingsRequest, UpdateTaskMcpRequest,
    UpdateTaskRequest, mcp_builtin_kind_guide, now_rfc3339,
};
use crate::notepad_store::TaskRunnerNotepadStore;
use crate::remote_server_runtime::{
    TaskRunnerRemoteConnectionStore, test_remote_server_connectivity,
};
use crate::store::AppStore;
use crate::terminal_store::TaskRunnerTerminalControllerStore;
use crate::ui_prompt_service::UiPromptService;

const RUN_CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(300);
const TASK_PROCESS_LOG_MAX_CHARS: usize = 200_000;
const TASK_PROCESS_LOG_INTERNAL_SERVER_NAME: &str = "task_run_process";
const TASK_PROCESS_LOG_INTERNAL_TOOL_NAME: &str = "record_process";
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
pub struct RunService {
    config: AppConfig,
    store: AppStore,
    ui_prompt_service: UiPromptService,
    start_locks: Arc<parking_lot::Mutex<HashMap<String, Arc<AsyncMutex<()>>>>>,
}

#[derive(Clone)]
pub struct McpCatalogService {
    task_service: TaskService,
    ui_prompt_service: UiPromptService,
}

#[derive(Clone)]
pub struct ToolingStateService {
    config: AppConfig,
}

#[derive(Debug, Clone)]
struct PrerequisiteTaskContext {
    task_id: String,
    title: String,
    objective: String,
    status: TaskStatus,
    run_id: Option<String>,
    result_summary: Option<String>,
    run_result_summary: Option<String>,
    process_log: Option<String>,
    report_content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ChatosTaskCallbackPayload {
    event: String,
    task_id: String,
    run_id: Option<String>,
    status: String,
    task_title: String,
    result_summary: Option<String>,
    error_message: Option<String>,
    report_content: Option<String>,
    process_log: Option<String>,
    source_session_id: Option<String>,
    source_turn_id: Option<String>,
    source_user_message_id: Option<String>,
    parent_task_id: Option<String>,
    source_run_id: Option<String>,
    prerequisite_task_ids: Vec<String>,
    schedule_mode: String,
    callback_at: String,
}

impl TaskService {
    pub(crate) fn new(config: AppConfig, store: AppStore) -> Self {
        Self { config, store }
    }

    pub async fn get_runtime_settings(&self) -> Result<Option<RuntimeSettingsRecord>, String> {
        self.store.get_runtime_settings().await
    }

    pub async fn update_runtime_settings(
        &self,
        input: UpdateRuntimeSettingsRequest,
    ) -> Result<RuntimeSettingsRecord, String> {
        if input.task_execution_max_iterations == Some(0) {
            return Err("task_execution_max_iterations 必须大于 0".to_string());
        }

        let now = now_rfc3339();
        let mut settings = self
            .get_runtime_settings()
            .await?
            .unwrap_or(RuntimeSettingsRecord {
                id: SYSTEM_RUNTIME_SETTINGS_ID.to_string(),
                task_execution_max_iterations: self.config.default_task_execution_max_iterations,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        if let Some(task_execution_max_iterations) = input.task_execution_max_iterations {
            settings.task_execution_max_iterations = task_execution_max_iterations;
        }
        settings.updated_at = now;
        self.store.save_runtime_settings(settings).await
    }

    pub async fn effective_task_execution_max_iterations(&self) -> Result<usize, String> {
        Ok(self
            .get_runtime_settings()
            .await?
            .map(|settings| settings.task_execution_max_iterations.max(1))
            .unwrap_or(self.config.default_task_execution_max_iterations.max(1)))
    }

    pub async fn list_tasks(&self) -> Result<Vec<TaskRecord>, String> {
        self.hydrate_tasks_prerequisites(self.store.list_tasks().await?)
            .await
    }

    pub async fn list_tasks_filtered(
        &self,
        filters: TaskListFilters,
    ) -> Result<Vec<TaskRecord>, String> {
        let filters = sanitize_task_list_filters(filters);
        self.hydrate_tasks_prerequisites(self.store.list_tasks_filtered(&filters).await?)
            .await
    }

    pub async fn list_tasks_for_source_user_message(
        &self,
        source_user_message_id: &str,
        creator: Option<&CurrentUser>,
    ) -> Result<Vec<TaskRecord>, String> {
        let Some(source_user_message_id) =
            normalized_optional(Some(source_user_message_id.to_string()))
        else {
            return Ok(Vec::new());
        };
        let filters = sanitize_task_list_filters(TaskListFilters {
            creator_user_id: creator.map(|user| user.id.clone()),
            ..TaskListFilters::default()
        });
        let tasks = self.store.list_tasks_filtered(&filters).await?;
        let tasks = tasks
            .into_iter()
            .filter(|task| {
                task.source_user_message_id.as_deref() == Some(source_user_message_id.as_str())
            })
            .collect::<Vec<_>>();
        self.hydrate_tasks_prerequisites(tasks).await
    }

    pub async fn list_tasks_page(
        &self,
        filters: TaskListFilters,
    ) -> Result<PaginatedResponse<TaskRecord>, String> {
        let mut filters = sanitize_task_list_filters(filters);
        filters.limit = Some(filters.limit.unwrap_or(20));
        filters.offset = Some(filters.offset.unwrap_or(0));
        let mut page = self.store.list_tasks_page(&filters).await?;
        page.items = self.hydrate_tasks_prerequisites(page.items).await?;
        Ok(page)
    }

    pub async fn get_task(&self, id: &str) -> Result<Option<TaskRecord>, String> {
        match self.store.get_task(id).await? {
            Some(task) => self.hydrate_task_prerequisites(task).await.map(Some),
            None => Ok(None),
        }
    }

    pub async fn list_task_prerequisites(
        &self,
        id: &str,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        if self.store.get_task(id).await?.is_none() {
            return Err(format!("任务不存在: {id}"));
        }
        let ids = self.direct_prerequisite_ids(id).await?;
        self.store.get_task_summaries_by_ids(&ids).await
    }

    pub async fn set_task_prerequisites(
        &self,
        id: &str,
        prerequisite_task_ids: Vec<String>,
        current_user: Option<&CurrentUser>,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        let prerequisite_task_ids = normalize_prerequisite_task_ids(prerequisite_task_ids);
        self.validate_task_prerequisites(id, &prerequisite_task_ids, current_user)
            .await?;
        self.store
            .set_task_prerequisites(id, prerequisite_task_ids.clone())
            .await?;
        task.prerequisite_task_ids = prerequisite_task_ids;
        task.updated_at = now_rfc3339();
        let saved = self.store.save_task(task).await?;
        self.hydrate_task_prerequisites(saved).await.map(Some)
    }

    pub async fn get_task_dependency_graph(
        &self,
        id: &str,
    ) -> Result<Option<TaskDependencyGraph>, String> {
        if self.store.get_task(id).await?.is_none() {
            return Ok(None);
        }
        let direct_ids = self.direct_prerequisite_ids(id).await?;
        let transitive_ids = self.resolve_prerequisite_order(id).await?;
        let direct = self.store.get_task_summaries_by_ids(&direct_ids).await?;
        let transitive = self
            .store
            .get_task_summaries_by_ids(&transitive_ids)
            .await?;
        let blocked_by = transitive
            .iter()
            .filter(|task| task.status != TaskStatus::Succeeded)
            .cloned()
            .collect::<Vec<_>>();
        Ok(Some(TaskDependencyGraph {
            task_id: id.to_string(),
            prerequisites: direct,
            transitive_prerequisites: transitive,
            ready: blocked_by.is_empty(),
            blocked_by,
        }))
    }

    async fn hydrate_task_prerequisites(&self, mut task: TaskRecord) -> Result<TaskRecord, String> {
        task.prerequisite_task_ids = self.direct_prerequisite_ids(&task.id).await?;
        Ok(task)
    }

    async fn hydrate_tasks_prerequisites(
        &self,
        tasks: Vec<TaskRecord>,
    ) -> Result<Vec<TaskRecord>, String> {
        let mut out = Vec::with_capacity(tasks.len());
        for task in tasks {
            out.push(self.hydrate_task_prerequisites(task).await?);
        }
        Ok(out)
    }

    async fn direct_prerequisite_ids(&self, task_id: &str) -> Result<Vec<String>, String> {
        Ok(self
            .store
            .list_task_prerequisites(task_id)
            .await?
            .into_iter()
            .map(|item| item.prerequisite_task_id)
            .collect())
    }

    async fn validate_task_prerequisites(
        &self,
        task_id: &str,
        prerequisite_task_ids: &[String],
        current_user: Option<&CurrentUser>,
    ) -> Result<(), String> {
        if prerequisite_task_ids.len() > 50 {
            return Err("前置任务数量不能超过 50 个".to_string());
        }
        for prerequisite_task_id in prerequisite_task_ids {
            if prerequisite_task_id == task_id {
                return Err("任务不能依赖自身".to_string());
            }
            let prerequisite = self
                .store
                .get_task(prerequisite_task_id)
                .await?
                .ok_or_else(|| format!("前置任务不存在: {prerequisite_task_id}"))?;
            if let Some(user) = current_user {
                if !user.is_admin()
                    && prerequisite.creator_user_id.as_deref() != Some(user.id.as_str())
                {
                    return Err(format!("无权引用前置任务: {prerequisite_task_id}"));
                }
            }
        }

        let mut stack = prerequisite_task_ids.to_vec();
        let mut visited = HashSet::new();
        let mut visited_count = 0usize;
        while let Some(current) = stack.pop() {
            if current == task_id {
                return Err(format!(
                    "前置任务不能形成循环依赖，任务 {task_id} 会依赖自身"
                ));
            }
            if !visited.insert(current.clone()) {
                continue;
            }
            visited_count += 1;
            if visited_count > 200 {
                return Err("前置任务依赖链过深或过大，请拆分后再保存".to_string());
            }
            for edge in self.store.list_task_prerequisites(&current).await? {
                stack.push(edge.prerequisite_task_id);
            }
        }
        Ok(())
    }

    async fn resolve_prerequisite_order(&self, task_id: &str) -> Result<Vec<String>, String> {
        let mut stack = vec![(task_id.to_string(), false)];
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        let mut order = Vec::new();

        while let Some((current, expanded)) = stack.pop() {
            if expanded {
                visiting.remove(&current);
                if visited.insert(current.clone()) && current != task_id {
                    order.push(current);
                }
                continue;
            }
            if visited.contains(&current) {
                continue;
            }
            if !visiting.insert(current.clone()) {
                return Err(format!("前置任务不能形成循环依赖: {current}"));
            }
            if visiting.len() > 200 {
                return Err("前置任务依赖链过深或过大，请拆分后再执行".to_string());
            }
            stack.push((current.clone(), true));
            let mut prerequisites = self.direct_prerequisite_ids(&current).await?;
            prerequisites.reverse();
            for prerequisite_task_id in prerequisites {
                if prerequisite_task_id == task_id {
                    return Err(format!(
                        "前置任务不能形成循环依赖，任务 {task_id} 会依赖自身"
                    ));
                }
                stack.push((prerequisite_task_id, false));
            }
        }
        Ok(order)
    }

    pub async fn task_stats(&self) -> Result<TaskStatsResponse, String> {
        self.store.task_stats().await
    }

    pub async fn task_index(&self) -> Result<TaskIndexResponse, String> {
        Ok(TaskIndexResponse {
            tasks: self.store.list_task_summaries().await?,
            tags: self.store.list_task_tags().await?,
        })
    }

    pub async fn list_task_summaries_filtered(
        &self,
        filters: TaskListFilters,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        let filters = sanitize_task_list_filters(filters);
        self.store.list_task_summaries_filtered(&filters).await
    }

    pub async fn get_task_summaries_by_ids(
        &self,
        ids: Vec<String>,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        let ids = sanitize_id_list(ids);
        self.store.get_task_summaries_by_ids(&ids).await
    }

    pub async fn create_task(
        &self,
        input: CreateTaskRequest,
        creator: Option<&CurrentUser>,
        source_context: Option<TaskSourceContext>,
    ) -> Result<TaskRecord, String> {
        validate_required("title", &input.title)?;
        validate_required("objective", &input.objective)?;
        if let Some(model_config_id) = input.default_model_config_id.as_deref() {
            self.ensure_model_config_exists(model_config_id).await?;
        }
        if matches!(input.status, Some(TaskStatus::Running)) {
            return Err("任务运行状态由系统维护，请通过执行任务进入 running".to_string());
        }
        let prerequisite_task_ids = normalize_prerequisite_task_ids(
            input.prerequisite_task_ids.clone().unwrap_or_default(),
        );

        let id = Uuid::new_v4().to_string();
        self.validate_task_prerequisites(&id, &prerequisite_task_ids, creator)
            .await?;
        let now = now_rfc3339();
        let source_context = source_context.unwrap_or_default();
        let schedule = sanitize_task_schedule_config(input.schedule.unwrap_or_default(), None)?;
        let mut mcp_config = sanitize_task_mcp_config(input.mcp_config.unwrap_or_default());
        if let Some(workspace_dir) = normalized_optional(source_context.workspace_dir.clone()) {
            mcp_config.workspace_dir = Some(workspace_dir);
        }
        if mcp_config.workspace_dir.is_some() {
            let _ = ensure_workspace_dir_available(
                self.config.default_workspace_dir.as_str(),
                mcp_config.workspace_dir.as_deref(),
            )?;
        }
        let passthrough_remote_server =
            if let Some(remote_server_config) = source_context.remote_server_config.clone() {
                Some(build_remote_server_record(
                    remote_server_config,
                    creator,
                    Some(id.clone()),
                    now.clone(),
                )?)
            } else {
                None
            };
        if let Some(remote_server) = passthrough_remote_server.as_ref() {
            mcp_config.default_remote_server_id = Some(remote_server.id.clone());
        }
        if passthrough_remote_server.is_none() {
            self.validate_task_mcp_config(&mcp_config).await?;
        }
        let task = TaskRecord {
            id: id.clone(),
            title: input.title.trim().to_string(),
            description: normalized_optional(input.description),
            objective: input.objective.trim().to_string(),
            input_payload: input.input_payload,
            status: input.status.unwrap_or(TaskStatus::Draft),
            priority: input.priority.unwrap_or(0),
            tags: normalize_tags(input.tags),
            default_model_config_id: normalized_optional(input.default_model_config_id),
            memory_thread_id: format!("task-{id}"),
            tenant_id: input
                .tenant_id
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| self.config.default_tenant_id.clone()),
            subject_id: input
                .subject_id
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| self.config.default_subject_id.clone()),
            creator_user_id: creator.map(|user| user.id.clone()),
            creator_username: creator.map(|user| user.username.clone()),
            creator_display_name: creator.map(|user| user.display_name.clone()),
            result_summary: None,
            process_log: None,
            last_run_id: None,
            schedule,
            parent_task_id: None,
            source_run_id: None,
            source_session_id: normalized_optional(source_context.source_session_id),
            source_turn_id: normalized_optional(source_context.source_turn_id),
            source_user_message_id: normalized_optional(source_context.source_user_message_id),
            prerequisite_task_ids: prerequisite_task_ids.clone(),
            task_tool_state: TaskToolState::default(),
            mcp_config,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        self.ensure_task_thread(&task).await?;
        if let Some(remote_server) = passthrough_remote_server {
            self.store.save_remote_server(remote_server).await?;
        }
        let saved = self.store.save_task(task).await?;
        self.store
            .set_task_prerequisites(&id, prerequisite_task_ids)
            .await?;
        let hydrated = self.hydrate_task_prerequisites(saved).await?;
        Ok(hydrated)
    }

    pub async fn update_task(
        &self,
        id: &str,
        patch: UpdateTaskRequest,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };

        if let Some(title) = patch.title {
            validate_required("title", &title)?;
            task.title = title.trim().to_string();
        }
        if let Some(description) = patch.description {
            task.description = normalized_optional(Some(description));
        }
        if let Some(objective) = patch.objective {
            validate_required("objective", &objective)?;
            task.objective = objective.trim().to_string();
        }
        if let Some(input_payload) = patch.input_payload {
            task.input_payload = Some(input_payload);
        }
        if let Some(status) = patch.status {
            if status == TaskStatus::Running {
                return Err("任务运行状态由系统维护，请通过执行任务进入 running".to_string());
            }
            if self.store.has_active_run_for_task(id).await? {
                return Err("任务仍有运行中的执行记录，请先取消或等待完成".to_string());
            }
            task.status = status;
        }
        if let Some(priority) = patch.priority {
            task.priority = priority;
        }
        if let Some(tags) = patch.tags {
            task.tags = normalize_tags(Some(tags));
        }
        if let Some(model_config_id) = patch.default_model_config_id {
            let model_config_id = model_config_id.trim().to_string();
            if !model_config_id.is_empty() {
                self.ensure_model_config_exists(&model_config_id).await?;
                task.default_model_config_id = Some(model_config_id);
            } else {
                task.default_model_config_id = None;
            }
        }
        if let Some(schedule) = patch.schedule {
            task.schedule = sanitize_task_schedule_config(schedule, Some(&task.schedule))?;
        }
        if let Some(mcp_config) = patch.mcp_config {
            task.mcp_config = sanitize_task_mcp_config(mcp_config);
            self.validate_task_mcp_config(&task.mcp_config).await?;
        }
        let prerequisite_task_ids = patch
            .prerequisite_task_ids
            .map(normalize_prerequisite_task_ids);
        if let Some(prerequisite_task_ids) = prerequisite_task_ids.as_ref() {
            self.validate_task_prerequisites(id, prerequisite_task_ids, None)
                .await?;
            task.prerequisite_task_ids = prerequisite_task_ids.clone();
        }
        task.updated_at = now_rfc3339();
        self.ensure_task_thread(&task).await?;
        let saved = self.store.save_task(task).await?;
        if let Some(prerequisite_task_ids) = prerequisite_task_ids {
            self.store
                .set_task_prerequisites(id, prerequisite_task_ids)
                .await?;
        }
        self.hydrate_task_prerequisites(saved).await.map(Some)
    }

    pub async fn record_task_process(
        &self,
        id: &str,
        input: RecordTaskProcessRequest,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        let now = now_rfc3339();
        task.process_log = apply_task_process_log_update(task.process_log, input, now.as_str())?;
        task.updated_at = now;
        let saved = self.store.save_task(task).await?;
        self.hydrate_task_prerequisites(saved).await.map(Some)
    }

    pub async fn update_task_mcp(
        &self,
        id: &str,
        patch: UpdateTaskMcpRequest,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        if let Some(enabled) = patch.enabled {
            task.mcp_config.enabled = enabled;
        }
        if let Some(init_mode) = patch.init_mode {
            task.mcp_config.init_mode = init_mode;
        }
        if let Some(prompt_mode) = patch.builtin_prompt_mode {
            task.mcp_config.builtin_prompt_mode = prompt_mode;
        }
        if let Some(prompt_locale) = patch.builtin_prompt_locale {
            let normalized = prompt_locale.trim();
            if !normalized.is_empty() {
                task.mcp_config.builtin_prompt_locale = normalized.to_string();
            }
        }
        if let Some(kinds) = patch.enabled_builtin_kinds {
            task.mcp_config.enabled_builtin_kinds = normalize_builtin_kind_names(kinds);
        }
        if let Some(workspace_dir) = patch.workspace_dir {
            task.mcp_config.workspace_dir = normalized_optional(Some(workspace_dir));
        }
        if let Some(default_remote_server_id) = patch.default_remote_server_id {
            task.mcp_config.default_remote_server_id =
                normalized_optional(Some(default_remote_server_id));
        }
        task.mcp_config = sanitize_task_mcp_config(task.mcp_config);
        self.validate_task_mcp_config(&task.mcp_config).await?;
        task.updated_at = now_rfc3339();
        Ok(Some(self.store.save_task(task).await?))
    }

    pub async fn delete_task(&self, id: &str) -> Result<bool, String> {
        if self.store.has_active_run_for_task(id).await? {
            return Err("任务仍有运行中的执行记录，暂时不能删除".to_string());
        }
        self.store.delete_task(id).await
    }

    pub async fn batch_update_status(
        &self,
        request: BatchTaskStatusUpdateRequest,
    ) -> Result<BatchTaskOperationResponse, String> {
        let task_ids = normalize_batch_task_ids(request.task_ids)?;
        let mut results = Vec::with_capacity(task_ids.len());

        for task_id in task_ids {
            match self
                .update_task(
                    &task_id,
                    UpdateTaskRequest {
                        status: Some(request.status),
                        ..UpdateTaskRequest::default()
                    },
                )
                .await
            {
                Ok(Some(_)) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: true,
                    message: None,
                    run_id: None,
                }),
                Ok(None) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: false,
                    message: Some("任务不存在".to_string()),
                    run_id: None,
                }),
                Err(err) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: false,
                    message: Some(err),
                    run_id: None,
                }),
            }
        }

        Ok(summarize_batch_results(results))
    }

    pub async fn batch_delete_tasks(
        &self,
        request: BatchTaskDeleteRequest,
    ) -> Result<BatchTaskOperationResponse, String> {
        let task_ids = normalize_batch_task_ids(request.task_ids)?;
        let mut results = Vec::with_capacity(task_ids.len());

        for task_id in task_ids {
            match self.delete_task(&task_id).await {
                Ok(true) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: true,
                    message: None,
                    run_id: None,
                }),
                Ok(false) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: false,
                    message: Some("任务不存在".to_string()),
                    run_id: None,
                }),
                Err(err) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: false,
                    message: Some(err),
                    run_id: None,
                }),
            }
        }

        Ok(summarize_batch_results(results))
    }

    pub async fn list_due_scheduled_tasks(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<TaskRecord>, String> {
        self.store.list_due_scheduled_tasks(now).await
    }

    pub async fn mark_scheduled_run_started(
        &self,
        id: &str,
        started_at: DateTime<Utc>,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        task.schedule = advance_task_schedule_after_dispatch(&task.schedule, started_at)?;
        task.updated_at = now_rfc3339();
        Ok(Some(self.store.save_task(task).await?))
    }

    pub async fn mark_scheduled_run_failed(
        &self,
        id: &str,
        error: &str,
    ) -> Result<Option<TaskRecord>, String> {
        let Some(mut task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        task.result_summary = normalized_optional(Some(format!("scheduler error: {error}")));
        task.updated_at = now_rfc3339();
        Ok(Some(self.store.save_task(task).await?))
    }

    pub async fn get_task_memory_context(
        &self,
        id: &str,
        options: TaskMemoryContextOptions,
    ) -> Result<Option<TaskMemoryContextResponse>, String> {
        let Some(task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        let client = self.require_memory_client()?;
        let thread = client
            .get_thread(&task.memory_thread_id, Some(&task.tenant_id))
            .await?;

        let total_record_count = if thread.is_some() {
            client
                .count_thread_records(
                    &task.memory_thread_id,
                    &SdkCountThreadRecordsRequest {
                        tenant_id: task.tenant_id.clone(),
                        role: None,
                        record_type: None,
                        summary_status: None,
                    },
                )
                .await?
        } else {
            0
        };

        let context = if thread.is_some() {
            Some(
                client
                    .compose_context(&SdkComposeContextRequest {
                        tenant_id: task.tenant_id.clone(),
                        subject_id: Some(task.subject_id.clone()),
                        related_subject_ids: None,
                        thread_id: task.memory_thread_id.clone(),
                        policy: Some(sanitize_task_memory_context_policy(options)),
                    })
                    .await?,
            )
        } else {
            None
        };

        Ok(Some(TaskMemoryContextResponse {
            task_id: task.id,
            memory_thread_id: task.memory_thread_id,
            tenant_id: task.tenant_id,
            subject_id: task.subject_id,
            thread,
            context,
            total_record_count,
        }))
    }

    pub async fn get_task_memory_records(
        &self,
        id: &str,
        options: TaskMemoryRecordsOptions,
    ) -> Result<Option<TaskMemoryRecordsResponse>, String> {
        let Some(task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        let client = self.require_memory_client()?;
        let thread = client
            .get_thread(&task.memory_thread_id, Some(&task.tenant_id))
            .await?;
        let options = sanitize_task_memory_records_options(options);
        let limit = options.limit.unwrap_or(50);
        let offset = options.offset.unwrap_or(0);
        let order = options.order.clone().unwrap_or_else(|| "desc".to_string());

        let Some(thread) = thread else {
            return Ok(Some(TaskMemoryRecordsResponse {
                task_id: task.id,
                memory_thread_id: task.memory_thread_id,
                tenant_id: task.tenant_id,
                subject_id: task.subject_id,
                thread: None,
                total: 0,
                limit,
                offset,
                order,
                role: options.role,
                record_type: options.record_type,
                summary_status: options.summary_status,
                has_more: false,
                items: Vec::new(),
            }));
        };

        let page = client
            .list_thread_records_page(
                &task.memory_thread_id,
                &SdkListThreadRecordsRequest {
                    tenant_id: task.tenant_id.clone(),
                    role: options.role.clone(),
                    record_type: options.record_type.clone(),
                    summary_status: options.summary_status.clone(),
                    limit: Some(limit),
                    offset: Some(offset),
                    order: Some(order.clone()),
                },
            )
            .await?;

        Ok(Some(TaskMemoryRecordsResponse {
            task_id: task.id,
            memory_thread_id: task.memory_thread_id,
            tenant_id: task.tenant_id,
            subject_id: task.subject_id,
            thread: Some(thread),
            total: page.total,
            limit,
            offset,
            order,
            role: options.role,
            record_type: options.record_type,
            summary_status: options.summary_status,
            has_more: page.total > offset + page.items.len() as i64,
            items: page.items,
        }))
    }

    pub async fn summarize_task_memory(
        &self,
        id: &str,
    ) -> Result<Option<TaskMemorySummaryResponse>, String> {
        let Some(task) = self.store.get_task(id).await? else {
            return Ok(None);
        };
        let client = self.require_memory_client()?;
        let result = client
            .run_thread_repair_summary(&task.memory_thread_id, &task.tenant_id)
            .await?;
        Ok(Some(TaskMemorySummaryResponse {
            task_id: task.id,
            memory_thread_id: task.memory_thread_id,
            tenant_id: task.tenant_id,
            requested_at: now_rfc3339(),
            result,
        }))
    }

    async fn ensure_model_config_exists(&self, id: &str) -> Result<(), String> {
        match self.store.get_model_config(id).await? {
            Some(model) if model.enabled => Ok(()),
            Some(_) => Err(format!("模型配置未启用: {id}")),
            None => Err(format!("模型配置不存在: {id}")),
        }
    }

    async fn ensure_remote_server_exists(&self, id: &str) -> Result<(), String> {
        match self.store.get_remote_server(id).await? {
            Some(server) if server.enabled => Ok(()),
            Some(_) => Err(format!("远程服务器未启用: {id}")),
            None => Err(format!("远程服务器不存在: {id}")),
        }
    }

    async fn validate_task_mcp_config(&self, config: &TaskMcpConfig) -> Result<(), String> {
        if let Some(remote_server_id) = config.default_remote_server_id.as_deref() {
            self.ensure_remote_server_exists(remote_server_id).await?;
        }
        if config.workspace_dir.is_some() {
            let _ = ensure_workspace_dir_available(
                self.config.default_workspace_dir.as_str(),
                config.workspace_dir.as_deref(),
            )?;
        }
        Ok(())
    }

    fn require_memory_client(&self) -> Result<memory_engine_sdk::MemoryEngineClient, String> {
        self.config
            .memory_client()?
            .ok_or_else(|| "Memory Engine 未配置，无法读取任务上下文".to_string())
    }

    async fn ensure_task_thread(&self, task: &TaskRecord) -> Result<(), String> {
        let Some(client) = self.config.memory_client()? else {
            return Ok(());
        };
        client
            .upsert_thread(
                &task.memory_thread_id,
                &SdkUpsertThreadRequest {
                    tenant_id: task.tenant_id.clone(),
                    subject_id: task.subject_id.clone(),
                    thread_type: "task".to_string(),
                    external_thread_id: Some(task.id.clone()),
                    title: Some(task.title.clone()),
                    labels: Some(vec![
                        "task_runner".to_string(),
                        format!("task_status:{}", task.status.status_string()),
                    ]),
                    metadata: Some(json!({
                        "task_id": task.id,
                        "service": "task_runner_service",
                    })),
                    status: Some("active".to_string()),
                    created_at: None,
                    updated_at: None,
                    archived_at: None,
                },
            )
            .await
            .map(|_| ())
    }

    async fn create_followup_task_for_tool(
        &self,
        root_task_id: &str,
        run_id: &str,
        draft: SharedTaskDraft,
    ) -> Result<TaskRecord, String> {
        validate_required("title", &draft.title)?;
        let parent = self
            .store
            .get_task(root_task_id)
            .await?
            .ok_or_else(|| TASK_NOT_FOUND_ERR.to_string())?;
        let id = Uuid::new_v4().to_string();
        let now = now_rfc3339();
        let title = draft.title.trim().to_string();
        let description = normalized_optional(Some(draft.details));
        let objective = description.clone().unwrap_or_else(|| title.clone());
        let result_summary = normalized_optional(Some(draft.outcome_summary));
        let status = task_status_from_manager_status(draft.status.as_str());
        let mut task_tool_state = TaskToolState {
            due_at: normalized_optional_nested(draft.due_at),
            outcome_items: shared_outcome_items_into_tool_state(draft.outcome_items),
            resume_hint: normalized_optional(Some(draft.resume_hint)),
            blocker_reason: normalized_optional(Some(draft.blocker_reason)),
            blocker_needs: normalize_strings(draft.blocker_needs),
            blocker_kind: normalized_optional(Some(draft.blocker_kind)),
            completed_at: None,
            last_outcome_at: None,
        };
        if result_summary.is_some() || !task_tool_state.outcome_items.is_empty() {
            task_tool_state.last_outcome_at = Some(now.clone());
        }
        if task_manager_status_from_task_status(status) == "done" {
            task_tool_state.completed_at = Some(now.clone());
        }

        let task = TaskRecord {
            id: id.clone(),
            title,
            description,
            objective,
            input_payload: None,
            status,
            priority: task_priority_from_manager_label(draft.priority.as_str()),
            tags: normalize_strings(draft.tags),
            default_model_config_id: parent.default_model_config_id.clone(),
            memory_thread_id: format!("task-{id}"),
            tenant_id: parent.tenant_id.clone(),
            subject_id: parent.subject_id.clone(),
            creator_user_id: parent.creator_user_id.clone(),
            creator_username: parent.creator_username.clone(),
            creator_display_name: parent.creator_display_name.clone(),
            result_summary,
            process_log: None,
            last_run_id: None,
            schedule: TaskScheduleConfig::default(),
            parent_task_id: Some(parent.id.clone()),
            source_run_id: Some(run_id.to_string()),
            source_session_id: parent.source_session_id.clone(),
            source_turn_id: parent.source_turn_id.clone(),
            source_user_message_id: parent.source_user_message_id.clone(),
            prerequisite_task_ids: Vec::new(),
            task_tool_state,
            mcp_config: parent.mcp_config.clone(),
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
        };
        self.ensure_task_thread(&task).await?;
        let saved = self.store.save_task(task).await?;
        info!(
            root_task_id,
            source_run_id = run_id,
            created_task_id = saved.id.as_str(),
            created_task_title = saved.title.as_str(),
            created_task_status = saved.status.status_string(),
            "task manager auto-created follow-up task during task run"
        );
        Ok(saved)
    }

    async fn list_tool_tasks(
        &self,
        root_task_id: &str,
        source_run_id: Option<&str>,
        include_done: bool,
        limit: usize,
    ) -> Result<Vec<TaskRecord>, String> {
        if self.store.get_task(root_task_id).await?.is_none() {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        }
        let mut tasks = self
            .store
            .list_tasks()
            .await?
            .into_iter()
            .filter(|task| task_belongs_to_context(task, root_task_id))
            .collect::<Vec<_>>();
        if let Some(run_id) = source_run_id {
            tasks.retain(|task| task.source_run_id.as_deref() == Some(run_id));
        }
        if !include_done {
            tasks.retain(|task| task_manager_status_from_task_status(task.status) != "done");
        }
        tasks.sort_by(|left, right| {
            if left.id == root_task_id && right.id != root_task_id {
                std::cmp::Ordering::Less
            } else if right.id == root_task_id && left.id != root_task_id {
                std::cmp::Ordering::Greater
            } else {
                right.updated_at.cmp(&left.updated_at)
            }
        });
        tasks.truncate(limit);
        Ok(tasks)
    }

    async fn update_task_from_tool(
        &self,
        root_task_id: &str,
        task_id: &str,
        patch: SharedTaskUpdatePatch,
    ) -> Result<TaskRecord, String> {
        let Some(mut task) = self.store.get_task(task_id).await? else {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        };
        if !task_belongs_to_context(&task, root_task_id) {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        }

        let now = now_rfc3339();
        apply_manager_patch(&mut task, patch, false, now.as_str())?;
        task.updated_at = now;
        self.ensure_task_thread(&task).await?;
        self.store.save_task(task).await
    }

    async fn complete_task_from_tool(
        &self,
        root_task_id: &str,
        task_id: &str,
        patch: Option<SharedTaskUpdatePatch>,
    ) -> Result<TaskRecord, String> {
        let Some(mut task) = self.store.get_task(task_id).await? else {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        };
        if !task_belongs_to_context(&task, root_task_id) {
            return Err(TASK_NOT_FOUND_ERR.to_string());
        }

        let now = now_rfc3339();
        if let Some(patch) = patch {
            apply_manager_patch(&mut task, patch, true, now.as_str())?;
        } else {
            task.status = TaskStatus::Succeeded;
            task.task_tool_state.completed_at = Some(now.clone());
            task.task_tool_state.last_outcome_at = Some(now.clone());
        }
        task.status = TaskStatus::Succeeded;
        if task.task_tool_state.completed_at.is_none() {
            task.task_tool_state.completed_at = Some(now.clone());
        }
        if task.task_tool_state.last_outcome_at.is_none() {
            task.task_tool_state.last_outcome_at = Some(now.clone());
        }
        task.updated_at = now;
        self.ensure_task_thread(&task).await?;
        self.store.save_task(task).await
    }

    async fn delete_task_from_tool(
        &self,
        root_task_id: &str,
        task_id: &str,
    ) -> Result<bool, String> {
        if task_id == root_task_id {
            return Err("不能删除当前正在执行的根任务".to_string());
        }
        let Some(task) = self.store.get_task(task_id).await? else {
            return Ok(false);
        };
        if !task_belongs_to_context(&task, root_task_id) {
            return Ok(false);
        }
        if self.store.has_active_run_for_task(task_id).await? {
            return Err("任务仍有运行中的执行记录，暂时不能删除".to_string());
        }
        self.store.delete_task(task_id).await
    }
}

#[derive(Clone)]
struct TaskRunnerTaskManagerStore {
    task_service: TaskService,
}

impl TaskRunnerTaskManagerStore {
    fn new(task_service: TaskService) -> Self {
        Self { task_service }
    }
}

#[async_trait]
impl TaskManagerStore for TaskRunnerTaskManagerStore {
    async fn create_tasks_for_turn(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
        draft_tasks: Vec<SharedTaskDraft>,
    ) -> Result<Vec<Value>, String> {
        let draft_count = draft_tasks.len();
        let draft_titles = draft_tasks
            .iter()
            .map(|draft| draft.title.trim().to_string())
            .filter(|title| !title.is_empty())
            .collect::<Vec<_>>();
        info!(
            task_id = conversation_id,
            run_id = conversation_turn_id,
            draft_count,
            draft_titles = draft_titles.join(" | "),
            "task manager received create_tasks_for_turn request"
        );
        let mut created = Vec::with_capacity(draft_count);
        for draft in draft_tasks {
            let task = self
                .task_service
                .create_followup_task_for_tool(conversation_id, conversation_turn_id, draft)
                .await?;
            created.push(task_to_manager_value(&task));
        }
        info!(
            task_id = conversation_id,
            run_id = conversation_turn_id,
            created_count = created.len(),
            "task manager finished create_tasks_for_turn request"
        );
        Ok(created)
    }

    async fn review_and_create_tasks(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
        draft_tasks: Vec<SharedTaskDraft>,
        _timeout_ms: u64,
        _on_stream_chunk: Option<TaskStreamChunkCallback>,
    ) -> Result<Value, String> {
        let tasks = self
            .create_tasks_for_turn(conversation_id, conversation_turn_id, draft_tasks)
            .await?;
        Ok(json!({
            "confirmed": true,
            "cancelled": false,
            "auto_created": true,
            "created_count": tasks.len(),
            "tasks": tasks,
            "conversation_id": conversation_id,
            "conversation_turn_id": conversation_turn_id,
        }))
    }

    async fn list_tasks_for_context(
        &self,
        conversation_id: &str,
        conversation_turn_id: Option<&str>,
        include_done: bool,
        limit: usize,
    ) -> Result<Vec<Value>, String> {
        let tasks = self
            .task_service
            .list_tool_tasks(conversation_id, conversation_turn_id, include_done, limit)
            .await?;
        Ok(tasks.iter().map(task_to_manager_value).collect::<Vec<_>>())
    }

    async fn update_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
        patch: SharedTaskUpdatePatch,
    ) -> Result<Value, String> {
        let task = self
            .task_service
            .update_task_from_tool(conversation_id, task_id, patch)
            .await?;
        Ok(task_to_manager_value(&task))
    }

    async fn complete_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
        patch: Option<SharedTaskUpdatePatch>,
    ) -> Result<Value, String> {
        let task = self
            .task_service
            .complete_task_from_tool(conversation_id, task_id, patch)
            .await?;
        Ok(task_to_manager_value(&task))
    }

    async fn delete_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
    ) -> Result<bool, String> {
        self.task_service
            .delete_task_from_tool(conversation_id, task_id)
            .await
    }

    async fn task_board_updated_event(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
    ) -> Option<Value> {
        Some(json!({
            "event": "task_runner_task_board_updated",
            "data": {
                "task_id": conversation_id,
                "run_id": conversation_turn_id,
            }
        }))
    }
}

impl ModelConfigService {
    pub(crate) fn new(store: AppStore) -> Self {
        Self { store }
    }

    async fn first_task_using_model_config(
        &self,
        model_config_id: &str,
    ) -> Result<Option<String>, String> {
        Ok(self
            .store
            .list_tasks()
            .await?
            .into_iter()
            .find(|task| task.default_model_config_id.as_deref() == Some(model_config_id))
            .map(|task| task.id))
    }

    pub async fn list_model_configs(&self) -> Result<Vec<ModelConfigRecord>, String> {
        let records = self.store.list_model_configs().await?;
        records
            .into_iter()
            .map(normalize_model_config_record)
            .collect::<Result<Vec<_>, _>>()
    }

    pub async fn get_model_config(&self, id: &str) -> Result<Option<ModelConfigRecord>, String> {
        self.store
            .get_model_config(id)
            .await?
            .map(normalize_model_config_record)
            .transpose()
    }

    pub async fn create_model_config(
        &self,
        input: CreateModelConfigRequest,
    ) -> Result<ModelConfigRecord, String> {
        validate_required("name", &input.name)?;
        validate_required("model", &input.model)?;
        let provider = normalize_model_provider_input(&input.provider)?;
        let thinking_level =
            normalize_model_thinking_level_input(provider.as_str(), input.thinking_level.clone())?;
        let now = now_rfc3339();
        let record = ModelConfigRecord {
            id: Uuid::new_v4().to_string(),
            name: input.name.trim().to_string(),
            provider: provider.clone(),
            base_url: normalize_model_base_url_input(provider.as_str(), Some(input.base_url)),
            api_key: input.api_key.trim().to_string(),
            model: input.model.trim().to_string(),
            usage_scenario: normalized_optional(input.usage_scenario),
            temperature: input.temperature,
            max_output_tokens: input.max_output_tokens,
            thinking_level,
            supports_responses: input
                .supports_responses
                .unwrap_or_else(|| provider == "openai"),
            instructions: normalized_optional(input.instructions),
            request_cwd: normalized_optional(input.request_cwd),
            include_prompt_cache_retention: input.include_prompt_cache_retention.unwrap_or(false),
            request_body_limit_bytes: input.request_body_limit_bytes,
            enabled: input.enabled.unwrap_or(true),
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.save_model_config(record).await
    }

    pub async fn test_model_config(
        &self,
        id: &str,
        input: TestModelConfigRequest,
    ) -> Result<Option<ModelConfigTestResponse>, String> {
        let Some(model_config) = self
            .store
            .get_model_config(id)
            .await?
            .map(normalize_model_config_record)
            .transpose()?
        else {
            return Ok(None);
        };

        let prompt = input
            .prompt
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("请简短回复：task runner model config test ok。");
        let runtime_config = model_config.to_runtime_config(None);
        let handler = AiRequestHandler::new();
        let tested_at = now_rfc3339();
        info!(
            model_config_id = model_config.id.as_str(),
            provider = model_config.provider.as_str(),
            model = model_config.model.as_str(),
            base_url = model_config.base_url.as_str(),
            supports_responses = model_config.supports_responses,
            prompt = prompt,
            "task runner test_model_config started"
        );

        let result = run_compatible_prompt_with(
            &handler,
            &runtime_config,
            prompt,
            SimplePromptOptions {
                temperature: model_config.temperature,
                max_output_tokens: model_config.max_output_tokens.or(Some(128)),
                ..SimplePromptOptions::default()
            },
            build_responses_text_input,
        )
        .await;

        let response = match result {
            Ok(ai_response) => {
                info!(
                    model_config_id = model_config.id.as_str(),
                    provider = model_config.provider.as_str(),
                    model = model_config.model.as_str(),
                    response_id = ai_response.response_id.as_deref().unwrap_or(""),
                    finish_content_chars = ai_response.content.chars().count(),
                    usage = ai_response
                        .usage
                        .as_ref()
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    "task runner test_model_config succeeded"
                );
                ModelConfigTestResponse {
                    ok: true,
                    model_config_id: model_config.id.clone(),
                    provider: model_config.provider.clone(),
                    model: model_config.model.clone(),
                    content: select_preferred_response_text(
                        ai_response.content.as_str(),
                        ai_response.reasoning.as_deref(),
                    )
                    .map(ToOwned::to_owned),
                    reasoning: ai_response.reasoning,
                    usage: ai_response.usage,
                    response_id: ai_response.response_id,
                    error: None,
                    tested_at,
                }
            }
            Err(err) => {
                warn!(
                    model_config_id = model_config.id.as_str(),
                    provider = model_config.provider.as_str(),
                    model = model_config.model.as_str(),
                    error = err.as_str(),
                    "task runner test_model_config failed"
                );
                ModelConfigTestResponse {
                    ok: false,
                    model_config_id: model_config.id.clone(),
                    provider: model_config.provider.clone(),
                    model: model_config.model.clone(),
                    content: None,
                    reasoning: None,
                    usage: None,
                    response_id: None,
                    error: Some(err),
                    tested_at,
                }
            }
        };

        Ok(Some(response))
    }

    pub async fn update_model_config(
        &self,
        id: &str,
        patch: UpdateModelConfigRequest,
    ) -> Result<Option<ModelConfigRecord>, String> {
        let Some(mut model) = self.store.get_model_config(id).await? else {
            return Ok(None);
        };
        model = normalize_model_config_record(model)?;
        let original_provider = model.provider.clone();
        let original_base_url = model.base_url.clone();
        if let Some(name) = patch.name {
            validate_required("name", &name)?;
            model.name = name.trim().to_string();
        }
        if let Some(provider) = patch.provider {
            model.provider = normalize_model_provider_input(&provider)?;
        }
        if let Some(base_url) = patch.base_url {
            model.base_url =
                normalize_model_base_url_input(model.provider.as_str(), Some(base_url));
        } else if model.provider != original_provider
            && model.base_url
                == normalize_model_base_url_input(
                    original_provider.as_str(),
                    Some(original_base_url),
                )
        {
            model.base_url = normalize_model_base_url_input(model.provider.as_str(), None);
        }
        if let Some(api_key) = patch.api_key {
            model.api_key = api_key.trim().to_string();
        }
        if let Some(runtime_model) = patch.model {
            validate_required("model", &runtime_model)?;
            model.model = runtime_model.trim().to_string();
        }
        if let Some(usage_scenario) = patch.usage_scenario {
            model.usage_scenario = normalized_optional(Some(usage_scenario));
        }
        if let Some(temperature) = patch.temperature {
            model.temperature = Some(temperature);
        }
        if let Some(max_output_tokens) = patch.max_output_tokens {
            model.max_output_tokens = Some(max_output_tokens);
        }
        if let Some(thinking_level) = patch.thinking_level {
            model.thinking_level = normalize_model_thinking_level_input(
                model.provider.as_str(),
                Some(thinking_level),
            )?;
        }
        if let Some(supports_responses) = patch.supports_responses {
            model.supports_responses = supports_responses;
        }
        if let Some(instructions) = patch.instructions {
            model.instructions = normalized_optional(Some(instructions));
        }
        if let Some(request_cwd) = patch.request_cwd {
            model.request_cwd = normalized_optional(Some(request_cwd));
        }
        if let Some(include_prompt_cache_retention) = patch.include_prompt_cache_retention {
            model.include_prompt_cache_retention = include_prompt_cache_retention;
        }
        if let Some(request_body_limit_bytes) = patch.request_body_limit_bytes {
            model.request_body_limit_bytes = Some(request_body_limit_bytes);
        }
        if let Some(enabled) = patch.enabled {
            if !enabled {
                if let Some(task_id) = self.first_task_using_model_config(id).await? {
                    return Err(format!("模型配置仍被任务引用，暂时不能停用: {task_id}"));
                }
            }
            model.enabled = enabled;
        }
        model.thinking_level = normalize_model_thinking_level_input(
            model.provider.as_str(),
            model.thinking_level.clone(),
        )?;
        model.updated_at = now_rfc3339();
        Ok(Some(self.store.save_model_config(model).await?))
    }

    pub async fn list_model_catalog(
        &self,
        id: &str,
    ) -> Result<Option<ModelCatalogResponse>, String> {
        let Some(model) = self
            .store
            .get_model_config(id)
            .await?
            .map(normalize_model_config_record)
            .transpose()?
        else {
            return Ok(None);
        };
        info!(
            model_config_id = model.id.as_str(),
            provider = model.provider.as_str(),
            model = model.model.as_str(),
            base_url = model.base_url.as_str(),
            "task runner list_model_catalog started"
        );
        Ok(Some(
            fetch_model_catalog_for_record(Some(model.id.clone()), &model).await,
        ))
    }

    pub async fn preview_model_catalog(
        &self,
        input: PreviewModelCatalogRequest,
    ) -> Result<ModelCatalogResponse, String> {
        validate_required("provider", &input.provider)?;
        let provider = normalize_model_provider_input(&input.provider)?;
        let model = normalized_optional(input.model);
        let record = ModelConfigRecord {
            id: "preview".to_string(),
            name: "preview".to_string(),
            provider: provider.clone(),
            base_url: normalize_model_base_url_input(provider.as_str(), input.base_url),
            api_key: input
                .api_key
                .map(|value| value.trim().to_string())
                .unwrap_or_default(),
            model: model.unwrap_or_default(),
            usage_scenario: None,
            temperature: None,
            max_output_tokens: None,
            thinking_level: None,
            supports_responses: input
                .supports_responses
                .unwrap_or_else(|| provider == "openai"),
            instructions: None,
            request_cwd: None,
            include_prompt_cache_retention: false,
            request_body_limit_bytes: None,
            enabled: true,
            created_at: now_rfc3339(),
            updated_at: now_rfc3339(),
        };
        info!(
            provider = record.provider.as_str(),
            model = record.model.as_str(),
            base_url = record.base_url.as_str(),
            supports_responses = record.supports_responses,
            "task runner preview_model_catalog started"
        );
        Ok(fetch_model_catalog_for_record(None, &record).await)
    }

    pub async fn delete_model_config(&self, id: &str) -> Result<bool, String> {
        self.store.delete_model_config(id).await
    }

    pub async fn usage_stats(&self) -> Result<Vec<ModelConfigUsageRecord>, String> {
        self.store.list_model_config_usage().await
    }
}

fn normalize_model_provider_input(provider: &str) -> Result<String, String> {
    let raw = provider.trim();
    if raw.is_empty() {
        return Err("provider 为必填项".to_string());
    }
    let normalized = normalize_provider(raw);
    let provider = match normalized.as_str() {
        "gpt" | "openai_compatible" => "openai",
        "deepseek" => "deepseek",
        "kimi" => "kimik2",
        "custom_gateway" => "openai",
        "kiminik2" => "kimik2",
        other => other,
    };
    match provider {
        "openai" | "deepseek" | "kimik2" => Ok(provider.to_string()),
        _ => Err("provider 仅支持 openai / deepseek / kimik2".to_string()),
    }
}

fn normalize_model_thinking_level_input(
    provider: &str,
    level: Option<String>,
) -> Result<Option<String>, String> {
    let level = level
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(level) = level else {
        return Ok(None);
    };
    normalize_thinking_level(provider, Some(level.as_str()))
        .map_err(|_| "思考等级仅支持 none/auto/minimal/low/medium/high/xhigh/max".to_string())
}

fn normalize_model_base_url_input(provider: &str, base_url: Option<String>) -> String {
    base_url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default_base_url_for_provider(provider, "https://api.openai.com/v1"))
        .trim_end_matches('/')
        .to_string()
}

fn normalize_model_config_record(
    mut record: ModelConfigRecord,
) -> Result<ModelConfigRecord, String> {
    let provider = normalize_model_provider_input(&record.provider)?;
    record.thinking_level =
        normalize_model_thinking_level_input(provider.as_str(), record.thinking_level.clone())?;
    record.base_url = normalize_model_base_url_input(provider.as_str(), Some(record.base_url));
    record.provider = provider;
    record.usage_scenario = normalized_optional(record.usage_scenario);
    record.instructions = normalized_optional(record.instructions);
    record.request_cwd = normalized_optional(record.request_cwd);
    Ok(record)
}

fn model_list_urls(provider: &str, base_url: &str) -> Vec<String> {
    let mut urls = vec![format!("{}/models", base_url.trim_end_matches('/'))];
    if provider == "deepseek" && base_url.ends_with("/v1") {
        let fallback = base_url.trim_end_matches("/v1");
        urls.push(format!("{fallback}/models"));
    }
    urls
}

fn read_provider_model_bool_field(item: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|key| item.get(*key).and_then(|value| value.as_bool()))
        .unwrap_or(false)
}

fn read_provider_model_i64_field(item: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| item.get(*key).and_then(|value| value.as_i64()))
}

fn normalize_provider_model_item(provider: &str, item: &Value) -> Option<ProviderModelRecord> {
    let id = item
        .get("id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let supports_images = read_provider_model_bool_field(
        item,
        &["supports_images", "supports_image_in", "vision", "image"],
    );
    let supports_video =
        read_provider_model_bool_field(item, &["supports_video", "supports_video_in"]);
    let supports_reasoning =
        read_provider_model_bool_field(item, &["supports_reasoning", "reasoning"]);
    let supports_responses =
        read_provider_model_bool_field(item, &["supports_responses"]) || provider == "openai";
    Some(ProviderModelRecord {
        id,
        owned_by: item
            .get("owned_by")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        context_length: read_provider_model_i64_field(
            item,
            &["context_length", "max_context_length", "max_tokens"],
        ),
        supports_images,
        supports_video,
        supports_reasoning,
        supports_responses,
        raw: Some(item.clone()),
    })
}

fn normalize_provider_models(provider: &str, raw: &Value) -> Vec<ProviderModelRecord> {
    let items = raw
        .get("data")
        .and_then(|value| value.as_array())
        .or_else(|| raw.as_array())
        .cloned()
        .unwrap_or_default();
    items
        .iter()
        .filter_map(|item| normalize_provider_model_item(provider, item))
        .collect()
}

async fn fetch_provider_models(
    profile: &ModelConfigRecord,
) -> Result<Vec<ProviderModelRecord>, String> {
    let api_key = profile.api_key.trim();
    if api_key.is_empty() {
        return Err("当前供应商配置未提供 API Key".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|err| err.to_string())?;
    let mut last_error = None;
    for url in model_list_urls(profile.provider.as_str(), profile.base_url.as_str()) {
        info!(
            provider = profile.provider.as_str(),
            model_config_id = profile.id.as_str(),
            model = profile.model.as_str(),
            url = url.as_str(),
            "task runner requesting provider model catalog"
        );
        match client.get(url.as_str()).bearer_auth(api_key).send().await {
            Ok(response) => {
                let status = response.status();
                let raw_text = response.text().await.map_err(|err| err.to_string())?;
                if !status.is_success() {
                    warn!(
                        provider = profile.provider.as_str(),
                        model_config_id = profile.id.as_str(),
                        model = profile.model.as_str(),
                        url = url.as_str(),
                        status = status.as_u16(),
                        response_body = raw_text.as_str(),
                        "task runner provider model catalog request failed"
                    );
                    last_error = Some(format!("{status}: {raw_text}"));
                    continue;
                }
                let raw: Value = serde_json::from_str(raw_text.as_str())
                    .map_err(|err| format!("解析模型列表失败: {err}"))?;
                let models = normalize_provider_models(profile.provider.as_str(), &raw);
                info!(
                    provider = profile.provider.as_str(),
                    model_config_id = profile.id.as_str(),
                    model = profile.model.as_str(),
                    url = url.as_str(),
                    model_count = models.len(),
                    "task runner received provider model catalog"
                );
                return Ok(models);
            }
            Err(err) => {
                let err_text = err.to_string();
                warn!(
                    provider = profile.provider.as_str(),
                    model_config_id = profile.id.as_str(),
                    model = profile.model.as_str(),
                    url = url.as_str(),
                    error = err_text.as_str(),
                    "task runner provider model catalog request errored"
                );
                last_error = Some(err_text);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "获取模型列表失败".to_string()))
}

fn fallback_model_list(profile: &ModelConfigRecord) -> Vec<ProviderModelRecord> {
    let model = profile.model.trim();
    if model.is_empty() {
        return Vec::new();
    }
    vec![ProviderModelRecord {
        id: model.to_string(),
        owned_by: Some(profile.provider.clone()),
        context_length: None,
        supports_images: false,
        supports_video: false,
        supports_reasoning: false,
        supports_responses: profile.supports_responses,
        raw: None,
    }]
}

async fn fetch_model_catalog_for_record(
    provider_config_id: Option<String>,
    profile: &ModelConfigRecord,
) -> ModelCatalogResponse {
    match fetch_provider_models(profile).await {
        Ok(models) => ModelCatalogResponse {
            provider_config_id,
            provider: profile.provider.clone(),
            base_url: profile.base_url.clone(),
            source: "live".to_string(),
            fetched_at: Some(now_rfc3339()),
            models,
            error: None,
        },
        Err(error) => ModelCatalogResponse {
            provider_config_id,
            provider: profile.provider.clone(),
            base_url: profile.base_url.clone(),
            source: "fallback".to_string(),
            fetched_at: None,
            models: fallback_model_list(profile),
            error: Some(error),
        },
    }
}

impl RemoteServerService {
    pub(crate) fn new(store: AppStore) -> Self {
        Self { store }
    }

    async fn first_task_referencing_server(
        &self,
        server_id: &str,
    ) -> Result<Option<String>, String> {
        Ok(self
            .store
            .list_tasks()
            .await?
            .into_iter()
            .find(|task| task.mcp_config.default_remote_server_id.as_deref() == Some(server_id))
            .map(|task| task.id))
    }

    pub async fn list_remote_servers(&self) -> Result<Vec<RemoteServerRecord>, String> {
        self.store.list_remote_servers().await
    }

    pub async fn get_remote_server(&self, id: &str) -> Result<Option<RemoteServerRecord>, String> {
        self.store.get_remote_server(id).await
    }

    pub async fn create_remote_server(
        &self,
        input: CreateRemoteServerRequest,
        creator: Option<&CurrentUser>,
    ) -> Result<RemoteServerRecord, String> {
        let now = now_rfc3339();
        let record = build_remote_server_record(input, creator, None, now)?;
        self.store.save_remote_server(record).await
    }

    pub async fn update_remote_server(
        &self,
        id: &str,
        patch: UpdateRemoteServerRequest,
    ) -> Result<Option<RemoteServerRecord>, String> {
        let Some(mut record) = self.store.get_remote_server(id).await? else {
            return Ok(None);
        };

        if let Some(name) = patch.name {
            validate_required("name", &name)?;
            record.name = name.trim().to_string();
        }
        if let Some(host) = patch.host {
            validate_required("host", &host)?;
            record.host = host.trim().to_string();
        }
        if let Some(port) = patch.port {
            record.port = normalize_remote_server_port(Some(port))?;
        }
        if let Some(username) = patch.username {
            validate_required("username", &username)?;
            record.username = username.trim().to_string();
        }
        if let Some(auth_type) = patch.auth_type {
            validate_required("auth_type", &auth_type)?;
            record.auth_type = normalize_remote_server_auth_type(&auth_type)?;
        }
        if let Some(password) = patch.password {
            record.password = normalized_optional(Some(password));
        }
        if let Some(private_key_path) = patch.private_key_path {
            record.private_key_path = normalized_optional(Some(private_key_path));
        }
        if let Some(certificate_path) = patch.certificate_path {
            record.certificate_path = normalized_optional(Some(certificate_path));
        }
        if let Some(default_remote_path) = patch.default_remote_path {
            record.default_remote_path = normalized_optional(Some(default_remote_path));
        }
        if let Some(host_key_policy) = patch.host_key_policy {
            record.host_key_policy =
                normalize_remote_server_host_key_policy(Some(host_key_policy.as_str()))?;
        }
        if let Some(enabled) = patch.enabled {
            if !enabled {
                if let Some(task_id) = self.first_task_referencing_server(id).await? {
                    return Err(format!("远程服务器仍被任务引用，暂时不能停用: {task_id}"));
                }
            }
            record.enabled = enabled;
        }
        validate_remote_server_auth_fields(&record)?;
        record.updated_at = now_rfc3339();
        Ok(Some(self.store.save_remote_server(record).await?))
    }

    pub async fn delete_remote_server(&self, id: &str) -> Result<bool, String> {
        if let Some(task_id) = self.first_task_referencing_server(id).await? {
            return Err(format!("远程服务器仍被任务引用，暂时不能删除: {task_id}"));
        }
        self.store.delete_remote_server(id).await
    }

    pub async fn test_remote_server_draft(
        &self,
        input: TestRemoteServerRequest,
    ) -> Result<RemoteServerTestResponse, String> {
        let name = input
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("draft");
        let host = input
            .host
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "host is required".to_string())?;
        let username = input
            .username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "username is required".to_string())?;
        let auth_type = input
            .auth_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "auth_type is required".to_string())?;
        let now = now_rfc3339();
        let draft = RemoteServerRecord {
            id: "draft".to_string(),
            name: name.to_string(),
            host: host.to_string(),
            port: normalize_remote_server_port(input.port)?,
            username: username.to_string(),
            auth_type: normalize_remote_server_auth_type(auth_type)?,
            password: normalized_optional(input.password),
            private_key_path: normalized_optional(input.private_key_path),
            certificate_path: normalized_optional(input.certificate_path),
            default_remote_path: normalized_optional(input.default_remote_path),
            host_key_policy: normalize_remote_server_host_key_policy(
                input.host_key_policy.as_deref(),
            )?,
            enabled: true,
            last_tested_at: None,
            last_test_status: None,
            last_test_message: None,
            last_active_at: None,
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            task_id: None,
            created_at: now.clone(),
            updated_at: now,
        };
        validate_remote_server_auth_fields(&draft)?;

        Ok(match test_remote_server_connectivity(&draft, None).await {
            Ok(response) => response,
            Err(err) => RemoteServerTestResponse {
                ok: false,
                server_id: None,
                name: draft.name,
                host: draft.host,
                port: draft.port,
                username: draft.username,
                auth_type: draft.auth_type,
                remote_host: None,
                error: Some(err),
                tested_at: now_rfc3339(),
            },
        })
    }

    pub async fn test_remote_server_saved(
        &self,
        id: &str,
    ) -> Result<Option<RemoteServerTestResponse>, String> {
        let Some(mut record) = self.store.get_remote_server(id).await? else {
            return Ok(None);
        };

        let response = match test_remote_server_connectivity(&record, Some(record.id.clone())).await
        {
            Ok(response) => {
                record.last_tested_at = Some(response.tested_at.clone());
                record.last_test_status = Some("success".to_string());
                record.last_test_message = response.remote_host.clone();
                record.updated_at = now_rfc3339();
                self.store.save_remote_server(record).await?;
                response
            }
            Err(err) => {
                let tested_at = now_rfc3339();
                record.last_tested_at = Some(tested_at.clone());
                record.last_test_status = Some("failed".to_string());
                record.last_test_message = Some(err.clone());
                record.updated_at = now_rfc3339();
                self.store.save_remote_server(record.clone()).await?;
                RemoteServerTestResponse {
                    ok: false,
                    server_id: Some(record.id),
                    name: record.name,
                    host: record.host,
                    port: record.port,
                    username: record.username,
                    auth_type: record.auth_type,
                    remote_host: None,
                    error: Some(err),
                    tested_at,
                }
            }
        };

        Ok(Some(response))
    }
}

impl RunService {
    pub(crate) fn new(
        config: AppConfig,
        store: AppStore,
        ui_prompt_service: UiPromptService,
    ) -> Self {
        Self {
            config,
            store,
            ui_prompt_service,
            start_locks: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        }
    }

    async fn effective_task_execution_max_iterations(&self) -> Result<usize, String> {
        Ok(self
            .store
            .get_runtime_settings()
            .await?
            .map(|settings| settings.task_execution_max_iterations.max(1))
            .unwrap_or(self.config.default_task_execution_max_iterations.max(1)))
    }

    fn start_lock_for_task(&self, task_id: &str) -> Arc<AsyncMutex<()>> {
        let mut locks = self.start_locks.lock();
        locks
            .entry(task_id.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    pub async fn list_runs(&self, task_id: Option<&str>) -> Result<Vec<TaskRunRecord>, String> {
        self.store.list_runs(task_id).await
    }

    pub async fn list_runs_filtered(
        &self,
        filters: RunListFilters,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let filters = sanitize_run_list_filters(filters);
        self.store.list_runs_filtered(&filters).await
    }

    pub async fn list_runs_page(
        &self,
        filters: RunListFilters,
    ) -> Result<PaginatedResponse<TaskRunRecord>, String> {
        let mut filters = sanitize_run_list_filters(filters);
        filters.limit = Some(filters.limit.unwrap_or(20));
        filters.offset = Some(filters.offset.unwrap_or(0));
        self.store.list_runs_page(&filters).await
    }

    pub async fn run_index(
        &self,
        filters: RunListFilters,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        let filters = sanitize_run_list_filters(filters);
        self.store.list_run_summaries_filtered(&filters).await
    }

    pub async fn get_run_summaries_by_ids(
        &self,
        ids: Vec<String>,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        let ids = sanitize_id_list(ids);
        self.store.get_run_summaries_by_ids(&ids).await
    }

    pub async fn get_run(&self, id: &str) -> Result<Option<TaskRunRecord>, String> {
        self.store.get_run(id).await
    }

    pub async fn has_active_run_for_task(&self, task_id: &str) -> Result<bool, String> {
        self.store.has_active_run_for_task(task_id).await
    }

    pub async fn batch_start_runs(
        &self,
        request: BatchTaskRunRequest,
    ) -> Result<BatchTaskOperationResponse, String> {
        let task_ids = normalize_batch_task_ids(request.task_ids)?;
        let mut results = Vec::with_capacity(task_ids.len());

        for task_id in task_ids {
            match self
                .start_run(
                    &task_id,
                    StartTaskRunRequest {
                        model_config_id: request.model_config_id.clone(),
                        prompt_override: request.prompt_override.clone(),
                    },
                )
                .await
            {
                Ok(run) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: true,
                    message: None,
                    run_id: Some(run.id),
                }),
                Err(err) => results.push(BatchTaskOperationItem {
                    task_id,
                    ok: false,
                    message: Some(err),
                    run_id: None,
                }),
            }
        }

        Ok(summarize_batch_results(results))
    }

    pub fn subscribe_run_events(&self) -> broadcast::Receiver<TaskRunEventRecord> {
        self.store.subscribe_run_events()
    }

    pub async fn list_run_events(&self, run_id: &str) -> Result<Vec<TaskRunEventRecord>, String> {
        self.store.list_run_events(run_id).await
    }

    pub async fn recover_incomplete_runs(&self) -> Result<usize, String> {
        let mut active_runs = self
            .store
            .list_runs_filtered(&RunListFilters {
                status: Some(TaskRunStatus::Queued),
                ..RunListFilters::default()
            })
            .await?;
        active_runs.extend(
            self.store
                .list_runs_filtered(&RunListFilters {
                    status: Some(TaskRunStatus::Running),
                    ..RunListFilters::default()
                })
                .await?,
        );
        self.repair_stale_cancel_requested_runs().await?;

        if active_runs.is_empty() {
            self.store.refresh_runtime_guards().await?;
            return Ok(0);
        }

        let mut recovered_count = 0usize;
        for mut run in active_runs {
            let now = now_rfc3339();
            let previous_status = match run.status {
                TaskRunStatus::Queued => "queued",
                TaskRunStatus::Running => "running",
                TaskRunStatus::Succeeded => "succeeded",
                TaskRunStatus::Failed => "failed",
                TaskRunStatus::Cancelled => "cancelled",
                TaskRunStatus::Blocked => "blocked",
            };
            let was_cancel_requested =
                run.cancel_requested || self.store.fetch_cancel_requested(&run.id).await?;

            let (next_status, event_type, message, error_message, task_status) =
                if was_cancel_requested {
                    (
                        TaskRunStatus::Cancelled,
                        "recovered_cancelled_after_restart",
                        "任务在服务重启后按取消状态收尾".to_string(),
                        Some("run was cancelled while the service was restarting".to_string()),
                        TaskStatus::Cancelled,
                    )
                } else {
                    (
                        TaskRunStatus::Failed,
                        "recovered_failed_after_restart",
                        "任务运行因服务重启中断，已标记为失败".to_string(),
                        Some("run was interrupted by a task runner service restart".to_string()),
                        TaskStatus::Failed,
                    )
                };

            run.status = next_status;
            run.cancel_requested = false;
            run.finished_at = Some(now.clone());
            run.updated_at = now.clone();
            run.result_summary = Some(message.clone());
            run.error_message = error_message;

            if let Err(err) = self.store.save_run(run.clone()).await {
                warn!(
                    "failed to recover incomplete run {} during startup: {}",
                    run.id, err
                );
                continue;
            }

            if let Err(err) = self
                .store
                .append_run_event(TaskRunEventRecord::new(
                    run.id.clone(),
                    event_type.to_string(),
                    Some(message.clone()),
                    Some(json!({
                        "reason": "service_restart_recovery",
                        "previous_status": previous_status,
                        "recovered_status": match next_status {
                            TaskRunStatus::Queued => "queued",
                            TaskRunStatus::Running => "running",
                            TaskRunStatus::Succeeded => "succeeded",
                            TaskRunStatus::Failed => "failed",
                            TaskRunStatus::Cancelled => "cancelled",
                            TaskRunStatus::Blocked => "blocked",
                        },
                    })),
                ))
                .await
            {
                warn!(
                    "failed to append recovery event for run {}: {}",
                    run.id, err
                );
            }

            if let Ok(Some(mut task_record)) = self.store.get_task(&run.task_id).await {
                task_record.status = task_status;
                task_record.result_summary = Some(message.clone());
                task_record.last_run_id = Some(run.id.clone());
                task_record.updated_at = now.clone();
                if let Err(err) = self.store.save_task(task_record).await {
                    warn!(
                        "failed to persist recovered task {} for run {}: {}",
                        run.task_id, run.id, err
                    );
                }
            }

            self.store.clear_cancel_requested(&run.id);
            recovered_count += 1;
        }

        self.store.refresh_runtime_guards().await?;
        Ok(recovered_count)
    }

    pub async fn start_run(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
    ) -> Result<TaskRunRecord, String> {
        self.start_run_with_trigger(task_id, input, RunTriggerSource::Manual)
            .await
    }

    pub async fn start_scheduled_run(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
    ) -> Result<TaskRunRecord, String> {
        self.start_run_with_trigger(task_id, input, RunTriggerSource::Scheduler)
            .await
    }

    async fn start_run_with_trigger(
        &self,
        task_id: &str,
        input: StartTaskRunRequest,
        trigger: RunTriggerSource,
    ) -> Result<TaskRunRecord, String> {
        let start_lock = self.start_lock_for_task(task_id);
        let _guard = start_lock.lock().await;
        let task = self
            .store
            .get_task(task_id)
            .await?
            .ok_or_else(|| format!("任务不存在: {task_id}"))?;
        info!(
            task_id = task.id.as_str(),
            task_title = task.title.as_str(),
            task_status = task.status.status_string(),
            schedule_mode = task.schedule.mode.mode_key(),
            parent_task_id = task.parent_task_id.as_deref().unwrap_or(""),
            source_run_id = task.source_run_id.as_deref().unwrap_or(""),
            requested_model_config_id = input.model_config_id.as_deref().unwrap_or(""),
            has_prompt_override = input
                .prompt_override
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            "task runner received start_run request"
        );
        if matches!(task.schedule.mode, TaskScheduleMode::ContactAsync)
            && !matches!(trigger, RunTriggerSource::Scheduler)
        {
            return Err("联系人异步任务只能由后台调度器触发执行".to_string());
        }
        if self.store.has_active_run_for_task(task_id).await? {
            info!(
                task_id = task.id.as_str(),
                task_title = task.title.as_str(),
                "task runner rejected start_run because an active run already exists"
            );
            return Err("当前任务已有正在执行的运行".to_string());
        }
        self.ensure_task_thread(&task).await?;

        let model_config_id = normalized_optional(input.model_config_id.clone())
            .or(task.default_model_config_id.clone())
            .ok_or_else(|| "任务未绑定模型配置，且本次执行也没有指定模型配置".to_string())?;
        let model_config = self
            .store
            .get_model_config(&model_config_id)
            .await?
            .ok_or_else(|| format!("模型配置不存在: {model_config_id}"))?;
        if !model_config.enabled {
            return Err(format!("模型配置已禁用: {model_config_id}"));
        }
        let effective_workspace_dir =
            ensure_effective_task_workspace_dir(&self.config, &task, &model_config)?;

        let run_id = Uuid::new_v4().to_string();
        let input_snapshot = json!({
            "task_id": task.id,
            "task_title": task.title,
            "objective": task.objective,
            "description": task.description,
            "input_payload": task.input_payload,
            "prompt_override": input.prompt_override,
            "model_config_id": model_config_id,
            "mcp_config": task.mcp_config,
        });
        let now = now_rfc3339();
        let run = TaskRunRecord {
            id: run_id.clone(),
            task_id: task.id.clone(),
            model_config_id: model_config_id.clone(),
            memory_thread_id: task.memory_thread_id.clone(),
            status: TaskRunStatus::Queued,
            started_at: None,
            finished_at: None,
            input_snapshot,
            context_snapshot: None,
            result_summary: None,
            error_message: None,
            usage: None,
            report: None,
            cancel_requested: false,
            summary_job_run_id: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.save_run(run.clone()).await?;
        info!(
            run_id = run.id.as_str(),
            task_id = task.id.as_str(),
            task_title = task.title.as_str(),
            model_config_id = model_config_id.as_str(),
            workspace_dir = effective_workspace_dir.as_str(),
            schedule_mode = task.schedule.mode.mode_key(),
            parent_task_id = task.parent_task_id.as_deref().unwrap_or(""),
            source_run_id = task.source_run_id.as_deref().unwrap_or(""),
            "task runner queued run"
        );
        if let Ok(Some(mut task_record)) = self.store.get_task(task_id).await {
            task_record.status = TaskStatus::Running;
            task_record.last_run_id = Some(run.id.clone());
            task_record.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_task(task_record).await {
                warn!(
                    "failed to persist queued task state for task {} and run {}: {}",
                    task_id, run.id, err
                );
            }
        }
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "queued",
                Some("任务已进入队列".to_string()),
                None,
            ))
            .await?;

        let service = self.clone();
        let run_for_spawn = run.clone();
        let input_for_spawn = input.clone();
        let workspace_dir_for_spawn = effective_workspace_dir.clone();
        tokio::spawn(async move {
            service
                .execute_run(
                    task,
                    model_config,
                    run_for_spawn,
                    input_for_spawn,
                    workspace_dir_for_spawn,
                )
                .await;
        });

        Ok(run)
    }

    pub async fn cancel_run(&self, run_id: &str) -> Result<Option<TaskRunRecord>, String> {
        let Some(current_run) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        match current_run.status {
            TaskRunStatus::Queued | TaskRunStatus::Running => {}
            TaskRunStatus::Succeeded => {
                return Err("当前运行状态不允许取消: succeeded".to_string());
            }
            TaskRunStatus::Failed => {
                return Err("当前运行状态不允许取消: failed".to_string());
            }
            TaskRunStatus::Cancelled => {
                return Err("当前运行状态不允许取消: cancelled".to_string());
            }
            TaskRunStatus::Blocked => {
                return Err("当前运行状态不允许取消: blocked".to_string());
            }
        }
        if current_run.cancel_requested {
            return Ok(Some(current_run));
        }

        let Some(mut run) = self.store.mark_cancel_requested(run_id).await? else {
            return Ok(None);
        };
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run_id.to_string(),
                "cancel_requested",
                Some("已请求取消任务运行".to_string()),
                None,
            ))
            .await?;
        if matches!(run.status, TaskRunStatus::Queued) {
            run.status = TaskRunStatus::Cancelled;
            run.cancel_requested = false;
            run.finished_at = Some(now_rfc3339());
            run.updated_at = now_rfc3339();
            self.store.save_run(run.clone()).await?;
            self.store
                .append_run_event(TaskRunEventRecord::new(
                    run_id.to_string(),
                    "cancelled",
                    Some("任务在启动前已取消".to_string()),
                    None,
                ))
                .await?;
            if let Some(mut task_record) = self.store.get_task(&run.task_id).await? {
                task_record.status = TaskStatus::Cancelled;
                task_record.last_run_id = Some(run.id.clone());
                task_record.updated_at = now_rfc3339();
                self.store.save_task(task_record).await?;
            }
            self.try_send_terminal_callback(run.task_id.as_str(), &run)
                .await;
        }
        Ok(Some(run))
    }

    pub async fn retry_run(&self, run_id: &str) -> Result<Option<TaskRunRecord>, String> {
        let Some(run) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        if matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running) {
            return Err("运行仍在进行中，暂时不能重试".to_string());
        }

        let prompt_override = run
            .input_snapshot
            .get("prompt_override")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let request = StartTaskRunRequest {
            model_config_id: Some(run.model_config_id.clone()),
            prompt_override,
        };
        self.start_run(&run.task_id, request).await.map(Some)
    }

    async fn prepare_prerequisite_context(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        input: &StartTaskRunRequest,
    ) -> Result<Vec<PrerequisiteTaskContext>, String> {
        let prerequisite_ids = self.resolve_prerequisite_order(task.id.as_str()).await?;
        if prerequisite_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "dependency_graph_resolved",
                Some(format!("解析到 {} 个前置任务", prerequisite_ids.len())),
                Some(json!({ "prerequisite_task_ids": prerequisite_ids.clone() })),
            ))
            .await?;

        let mut contexts = Vec::new();
        for prerequisite_task_id in prerequisite_ids {
            let prerequisite_task = self
                .store
                .get_task(&prerequisite_task_id)
                .await?
                .ok_or_else(|| format!("前置任务不存在: {prerequisite_task_id}"))?;
            let prerequisite_run = self
                .ensure_prerequisite_succeeded(&prerequisite_task, run, input)
                .await?;
            let prerequisite_task = self
                .store
                .get_task(&prerequisite_task_id)
                .await?
                .unwrap_or(prerequisite_task);
            contexts.push(build_prerequisite_context(
                &prerequisite_task,
                prerequisite_run.as_ref(),
            ));
        }
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "dependency_context_attached",
                Some("前置任务结果已加入本次任务 prompt".to_string()),
                Some(prerequisite_context_json(&contexts)),
            ))
            .await?;
        Ok(contexts)
    }

    async fn ensure_prerequisite_succeeded(
        &self,
        task: &TaskRecord,
        parent_run: &TaskRunRecord,
        input: &StartTaskRunRequest,
    ) -> Result<Option<TaskRunRecord>, String> {
        if matches!(task.status, TaskStatus::Archived) {
            return Err(format!("前置任务已归档，不能执行: {}", task.id));
        }
        if matches!(task.status, TaskStatus::Succeeded) {
            return Ok(self.latest_successful_run(task.id.as_str()).await?);
        }

        let active_run = self.active_run_for_task(task.id.as_str()).await?;
        let run = if let Some(active_run) = active_run {
            self.store
                .append_run_event(TaskRunEventRecord::new(
                    parent_run.id.clone(),
                    "dependency_waiting_active_run",
                    Some(format!("等待前置任务正在运行的 run: {}", task.title)),
                    Some(json!({
                        "task_id": task.id,
                        "run_id": active_run.id,
                    })),
                ))
                .await?;
            active_run
        } else {
            self.store
                .append_run_event(TaskRunEventRecord::new(
                    parent_run.id.clone(),
                    "dependency_run_started",
                    Some(format!("开始执行前置任务: {}", task.title)),
                    Some(json!({ "task_id": task.id })),
                ))
                .await?;
            self.queue_dependency_run(
                task.clone(),
                StartTaskRunRequest {
                    model_config_id: input.model_config_id.clone(),
                    prompt_override: None,
                },
            )
            .await?
        };

        let completed = self
            .wait_for_run_terminal(run.id.as_str(), parent_run.id.as_str())
            .await?;
        self.store
            .append_run_event(TaskRunEventRecord::new(
                parent_run.id.clone(),
                "dependency_run_finished",
                Some(format!(
                    "前置任务执行结束: {} / {}",
                    task.title,
                    completed.status.status_string()
                )),
                Some(json!({
                    "task_id": task.id,
                    "run_id": completed.id,
                    "status": completed.status.status_string(),
                    "result_summary": completed.result_summary,
                    "error_message": completed.error_message,
                })),
            ))
            .await?;
        if completed.status == TaskRunStatus::Succeeded {
            Ok(Some(completed))
        } else {
            Err(format!(
                "前置任务未成功完成: {} ({})",
                task.title,
                completed.status.status_string()
            ))
        }
    }

    async fn wait_for_run_terminal(
        &self,
        run_id: &str,
        parent_run_id: &str,
    ) -> Result<TaskRunRecord, String> {
        let timeout = self.config.execution_timeout + Duration::from_secs(30);
        let started = tokio::time::Instant::now();
        loop {
            let run = self
                .store
                .get_run(run_id)
                .await?
                .ok_or_else(|| format!("运行不存在: {run_id}"))?;
            if is_terminal_run_status(run.status) {
                return Ok(run);
            }
            if self.store.is_cancel_requested(parent_run_id) {
                return Err("当前任务已请求取消，停止等待前置任务".to_string());
            }
            if started.elapsed() > timeout {
                return Err(format!("等待前置任务运行超时: {run_id}"));
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    async fn active_run_for_task(&self, task_id: &str) -> Result<Option<TaskRunRecord>, String> {
        Ok(self
            .store
            .list_runs(Some(task_id))
            .await?
            .into_iter()
            .find(|run| matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running)))
    }

    async fn latest_successful_run(&self, task_id: &str) -> Result<Option<TaskRunRecord>, String> {
        Ok(self
            .store
            .list_runs(Some(task_id))
            .await?
            .into_iter()
            .find(|run| run.status == TaskRunStatus::Succeeded))
    }

    async fn collect_succeeded_prerequisite_context(
        &self,
        task_id: &str,
    ) -> Result<Vec<PrerequisiteTaskContext>, String> {
        let prerequisite_ids = self.resolve_prerequisite_order(task_id).await?;
        let mut contexts = Vec::new();
        for prerequisite_task_id in prerequisite_ids {
            let task = self
                .store
                .get_task(&prerequisite_task_id)
                .await?
                .ok_or_else(|| format!("前置任务不存在: {prerequisite_task_id}"))?;
            if task.status != TaskStatus::Succeeded {
                return Err(format!("前置任务尚未成功完成: {}", task.title));
            }
            let run = self.latest_successful_run(task.id.as_str()).await?;
            contexts.push(build_prerequisite_context(&task, run.as_ref()));
        }
        Ok(contexts)
    }

    async fn queue_dependency_run(
        &self,
        task: TaskRecord,
        input: StartTaskRunRequest,
    ) -> Result<TaskRunRecord, String> {
        if self.store.has_active_run_for_task(task.id.as_str()).await? {
            return self
                .active_run_for_task(task.id.as_str())
                .await?
                .ok_or_else(|| "前置任务已有运行中记录，但读取失败".to_string());
        }
        self.ensure_task_thread(&task).await?;

        let model_config_id = normalized_optional(input.model_config_id.clone())
            .or(task.default_model_config_id.clone())
            .ok_or_else(|| "前置任务未绑定模型配置，且本次执行也没有指定模型配置".to_string())?;
        let model_config = self
            .store
            .get_model_config(&model_config_id)
            .await?
            .ok_or_else(|| format!("模型配置不存在: {model_config_id}"))?;
        if !model_config.enabled {
            return Err(format!("模型配置已禁用: {model_config_id}"));
        }
        let effective_workspace_dir =
            ensure_effective_task_workspace_dir(&self.config, &task, &model_config)?;

        let run_id = Uuid::new_v4().to_string();
        let input_snapshot = json!({
            "task_id": task.id,
            "task_title": task.title,
            "objective": task.objective,
            "description": task.description,
            "input_payload": task.input_payload,
            "prompt_override": input.prompt_override,
            "model_config_id": model_config_id,
            "mcp_config": task.mcp_config,
            "started_as_prerequisite": true,
        });
        let now = now_rfc3339();
        let run = TaskRunRecord {
            id: run_id.clone(),
            task_id: task.id.clone(),
            model_config_id: model_config_id.clone(),
            memory_thread_id: task.memory_thread_id.clone(),
            status: TaskRunStatus::Queued,
            started_at: None,
            finished_at: None,
            input_snapshot,
            context_snapshot: None,
            result_summary: None,
            error_message: None,
            usage: None,
            report: None,
            cancel_requested: false,
            summary_job_run_id: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.save_run(run.clone()).await?;
        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_record.status = TaskStatus::Running;
            task_record.last_run_id = Some(run.id.clone());
            task_record.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_task(task_record).await {
                warn!(
                    "failed to persist queued prerequisite task state for task {} and run {}: {}",
                    task.id, run.id, err
                );
            }
        }
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "queued",
                Some("前置任务已进入队列".to_string()),
                None,
            ))
            .await?;

        let prerequisite_context = self
            .collect_succeeded_prerequisite_context(task.id.as_str())
            .await?;
        let service = self.clone();
        let run_for_spawn = run.clone();
        let input_for_spawn = input.clone();
        tokio::spawn(async move {
            service
                .execute_run_model_phase(
                    task,
                    model_config,
                    run_for_spawn,
                    input_for_spawn,
                    effective_workspace_dir,
                    prerequisite_context,
                )
                .await;
        });

        Ok(run)
    }

    async fn resolve_prerequisite_order(&self, task_id: &str) -> Result<Vec<String>, String> {
        TaskService::new(self.config.clone(), self.store.clone())
            .resolve_prerequisite_order(task_id)
            .await
    }

    async fn finish_blocked_by_prerequisite(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
        message: String,
    ) {
        run.status = TaskRunStatus::Blocked;
        run.finished_at = Some(now_rfc3339());
        run.updated_at = now_rfc3339();
        run.error_message = Some(message.clone());
        run.result_summary = Some(message.clone());
        run.cancel_requested = false;
        if let Err(err) = self.store.save_run(run.clone()).await {
            warn!("failed to persist blocked task run {}: {}", run.id, err);
        }
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "dependency_failed",
                Some(message.clone()),
                None,
            ))
            .await
        {
            warn!(
                "failed to append dependency_failed event for run {}: {}",
                run.id, err
            );
        }
        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_record.status = TaskStatus::Blocked;
            task_record.result_summary = Some(message);
            task_record.last_run_id = Some(run.id.clone());
            task_record.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_task(task_record).await {
                warn!("failed to persist blocked task {}: {}", task.id, err);
            }
        }
        self.try_send_terminal_callback(task.id.as_str(), run).await;
    }

    async fn finish_failed_before_execution(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
        message: String,
    ) {
        run.status = TaskRunStatus::Failed;
        run.finished_at = Some(now_rfc3339());
        run.updated_at = now_rfc3339();
        run.error_message = Some(message.clone());
        run.result_summary = Some(message.clone());
        run.cancel_requested = false;
        if let Err(err) = self.store.save_run(run.clone()).await {
            warn!("failed to persist failed task run {}: {}", run.id, err);
        }
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "failed",
                Some(message.clone()),
                None,
            ))
            .await
        {
            warn!("failed to append failed event for run {}: {}", run.id, err);
        }
        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_record.status = TaskStatus::Failed;
            task_record.result_summary = Some(message);
            task_record.last_run_id = Some(run.id.clone());
            task_record.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_task(task_record).await {
                warn!("failed to persist failed task {}: {}", task.id, err);
            }
        }
        self.try_send_terminal_callback(task.id.as_str(), run).await;
    }

    async fn execute_run(
        &self,
        task: TaskRecord,
        model_config: ModelConfigRecord,
        mut run: TaskRunRecord,
        input: StartTaskRunRequest,
        effective_workspace_dir: String,
    ) {
        let prerequisite_context =
            match self.prepare_prerequisite_context(&task, &run, &input).await {
                Ok(context) => context,
                Err(err) => {
                    self.finish_blocked_by_prerequisite(&task, &mut run, err)
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
        )
        .await;
    }

    async fn execute_run_model_phase(
        &self,
        task: TaskRecord,
        model_config: ModelConfigRecord,
        mut run: TaskRunRecord,
        input: StartTaskRunRequest,
        effective_workspace_dir: String,
        prerequisite_context: Vec<PrerequisiteTaskContext>,
    ) {
        info!(
            run_id = run.id.as_str(),
            task_id = task.id.as_str(),
            task_title = task.title.as_str(),
            model_config_id = model_config.id.as_str(),
            model = model_config.model.as_str(),
            provider = model_config.provider.as_str(),
            workspace_dir = effective_workspace_dir.as_str(),
            prompt_override = input.prompt_override.as_deref().unwrap_or(""),
            "task runner begin execute_run"
        );
        if self.store.is_cancel_requested(&run.id) {
            self.finish_cancelled_before_start(&task, &mut run).await;
            return;
        }

        run.status = TaskRunStatus::Running;
        run.started_at = Some(now_rfc3339());
        run.updated_at = now_rfc3339();
        if let Err(err) = self.store.save_run(run.clone()).await {
            warn!("failed to persist running task run {}: {}", run.id, err);
        }
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "running",
                Some("任务开始执行".to_string()),
                None,
            ))
            .await
        {
            warn!("failed to append running event for run {}: {}", run.id, err);
        }

        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_record.status = TaskStatus::Running;
            task_record.updated_at = now_rfc3339();
            task_record.last_run_id = Some(run.id.clone());
            if let Err(err) = self.store.save_task(task_record).await {
                warn!("failed to persist running task {}: {}", task.id, err);
            }
        }
        if !prerequisite_context.is_empty() {
            attach_prerequisite_context_to_run(&mut run, &prerequisite_context);
            run.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_run(run.clone()).await {
                warn!(
                    "failed to persist prerequisite context for run {}: {}",
                    run.id, err
                );
            }
        }

        let prompt = build_task_prompt(
            &task,
            input.prompt_override.as_deref(),
            &prerequisite_context,
        );
        let mut effective_model_config = model_config.clone();
        effective_model_config.request_cwd = Some(effective_workspace_dir.clone());
        let model_runtime_config =
            effective_model_config.to_runtime_config(Some(effective_workspace_dir.clone()));
        let metadata = json!({
            "task_id": task.id,
            "run_id": run.id,
            "model_config_id": model_config.id,
            "service": "task_runner_service",
        });
        let task_process_logging_enabled = task_process_logging_enabled(&task.mcp_config);

        let mut run_spec = TaskRunSpec::new(
            task.id.clone(),
            run.id.clone(),
            model_runtime_config.clone(),
            prompt.clone(),
        )
        .with_model_config_id(model_config.id.clone())
        .with_metadata(Some(metadata.clone()))
        .with_record_options(
            RuntimeRecordOptions::persist_all()
                .with_assistant_message_mode("task_run")
                .with_assistant_message_source("task_runner")
                .with_tool_message_mode("task_tool")
                .with_tool_message_source("task_runner")
                .with_assistant_metadata(metadata.clone())
                .with_tool_metadata(metadata.clone()),
        )
        .with_user_record(Some(
            SaveRecordInput::user_message(run.id.clone(), prompt.clone())
                .with_conversation_turn_id(run.id.clone())
                .with_message_mode("task_run")
                .with_message_source("task_runner")
                .with_metadata(metadata.clone()),
        ));
        if task_process_logging_enabled {
            run_spec = run_spec.with_prefixed_input_items(task_process_log_prefixed_input_items(
                task.mcp_config.locale(),
            ));
        }

        let memory_scope = MemoryScope::thread(
            task.tenant_id.clone(),
            self.config.memory_engine_source_id.clone(),
            task.memory_thread_id.clone(),
        )
        .with_subject_id(task.subject_id.clone());
        run_spec = run_spec.with_memory_scope(Some(memory_scope));

        let max_iterations = match self.effective_task_execution_max_iterations().await {
            Ok(value) => value,
            Err(err) => {
                self.finish_failed_before_execution(
                    &task,
                    &mut run,
                    format!("加载运行时配置失败: {err}"),
                )
                .await;
                return;
            }
        };

        let mut runtime_config = TaskRuntimeConfig::new().with_max_iterations(Some(max_iterations));
        if let Some(memory_engine_base_url) = self.config.memory_engine_base_url.clone() {
            runtime_config = runtime_config.with_memory_engine(Some(
                TaskMemoryRuntimeConfig::new(
                    memory_engine_base_url,
                    self.config.memory_engine_source_id.clone(),
                )
                .with_timeout_ms(self.config.memory_timeout.as_millis() as u64)
                .with_record_scope(Some(MemoryRecordScope::message_thread(
                    task.tenant_id.clone(),
                    task.memory_thread_id.clone(),
                ))),
            ));
        }

        let runtime_config = self.apply_task_mcp_config(runtime_config, &task.mcp_config);
        if let Some(snapshot) = self
            .compose_context_snapshot(run_spec.memory_scope.as_ref())
            .await
        {
            run.context_snapshot = Some(snapshot);
            run.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_run(run.clone()).await {
                warn!(
                    "failed to persist context snapshot for run {}: {}",
                    run.id, err
                );
            }
        }
        let selected_builtin_kinds = selected_builtin_kinds(&task.mcp_config);
        let mut server_options = BuiltinMcpServerOptions::new(effective_workspace_dir)
            .with_user_id(task.subject_id.clone())
            .with_project_id(task.id.clone())
            .with_auto_create_task(true);
        if let Some(remote_server_id) = task.mcp_config.default_remote_server_id.clone() {
            server_options = server_options.with_remote_connection_id(remote_server_id);
        }
        let mut builtin_servers =
            builtin_servers_from_kinds(selected_builtin_kinds.clone(), &server_options);
        if task_process_logging_enabled {
            builtin_servers.push(task_process_log_builtin_server());
        }
        let (builtin_registry, builtin_init_errors) = build_builtin_registry(
            &builtin_servers,
            TaskService::new(self.config.clone(), self.store.clone()),
            self.ui_prompt_service.clone(),
        );
        let mut builtin_registry = builtin_registry;
        if task_process_logging_enabled {
            builtin_registry.register(TaskProcessLogBuiltinProvider::new(
                TASK_PROCESS_LOG_INTERNAL_SERVER_NAME,
                TaskService::new(self.config.clone(), self.store.clone()),
                task.id.clone(),
                run.id.clone(),
            ));
        }
        for err in builtin_init_errors {
            if let Err(event_err) = self
                .store
                .append_run_event(TaskRunEventRecord::new(
                    run.id.clone(),
                    "builtin_provider_warning",
                    Some(err.clone()),
                    None,
                ))
                .await
            {
                warn!(
                    "failed to append builtin warning event for run {}: {}",
                    run.id, event_err
                );
            }
            warn!("task runner builtin provider warning: {err}");
        }
        let mcp_builder = McpExecutorBuilder::new()
            .with_builtin_servers(builtin_servers)
            .with_builtin_registry(builtin_registry);

        let store_for_callbacks = self.store.clone();
        let run_id_for_chunk = run.id.clone();
        let pending_stream_event =
            Arc::new(parking_lot::Mutex::new(PendingRunStreamEvent::default()));

        let callbacks = RuntimeCallbacks {
            on_chunk: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run_id_for_chunk.clone();
                let pending = Arc::clone(&pending_stream_event);
                move |chunk| {
                    if chunk.is_empty() {
                        return;
                    }
                    let flushed = {
                        let mut state = pending.lock();
                        state.push("chunk", &chunk)
                    };
                    if let Some(flushed) = flushed {
                        append_pending_stream_event(&store, run_id.as_str(), flushed);
                    }
                }
            })),
            on_thinking: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run.id.clone();
                let pending = Arc::clone(&pending_stream_event);
                move |chunk| {
                    if chunk.is_empty() {
                        return;
                    }
                    let flushed = {
                        let mut state = pending.lock();
                        state.push("thinking", &chunk)
                    };
                    if let Some(flushed) = flushed {
                        append_pending_stream_event(&store, run_id.as_str(), flushed);
                    }
                }
            })),
            on_tools_start: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run.id.clone();
                let pending = Arc::clone(&pending_stream_event);
                move |payload| {
                    flush_pending_stream_event(&store, run_id.as_str(), &pending);
                    store.append_run_event_sync(TaskRunEventRecord::new(
                        run_id.clone(),
                        "tools_start",
                        Some("开始调用工具".to_string()),
                        Some(payload),
                    ));
                }
            })),
            on_tools_stream: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run.id.clone();
                move |payload| {
                    store.append_run_event_sync(TaskRunEventRecord::new(
                        run_id.clone(),
                        "tool_stream",
                        None,
                        Some(payload),
                    ));
                }
            })),
            on_tools_end: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run.id.clone();
                move |payload| {
                    store.append_run_event_sync(TaskRunEventRecord::new(
                        run_id.clone(),
                        "tools_end",
                        Some("工具调用结束".to_string()),
                        Some(payload),
                    ));
                }
            })),
            on_before_model_request: Some(Arc::new({
                let store = store_for_callbacks.clone();
                let run_id = run.id.clone();
                let pending = Arc::clone(&pending_stream_event);
                move |payload| {
                    flush_pending_stream_event(&store, run_id.as_str(), &pending);
                    store.append_run_event_sync(TaskRunEventRecord::new(
                        run_id.clone(),
                        "model_request",
                        Some("即将发起模型请求".to_string()),
                        Some(payload),
                    ));
                }
            })),
        };

        let cancel_requested = Arc::new(AtomicBool::new(self.store.is_cancel_requested(&run.id)));
        let stop_cancel_poll = Arc::new(AtomicBool::new(false));
        let cancel_poll_handle = tokio::spawn({
            let store = self.store.clone();
            let run_id = run.id.clone();
            let cancel_requested = Arc::clone(&cancel_requested);
            let stop_cancel_poll = Arc::clone(&stop_cancel_poll);
            async move {
                while !stop_cancel_poll.load(Ordering::Relaxed) {
                    match store.fetch_cancel_requested(&run_id).await {
                        Ok(is_requested) => {
                            cancel_requested.store(is_requested, Ordering::Relaxed);
                            if is_requested {
                                break;
                            }
                        }
                        Err(err) => {
                            warn!(
                                "failed to refresh cancel_requested flag for run {}: {}",
                                run_id, err
                            );
                        }
                    }
                    tokio::time::sleep(RUN_CANCEL_POLL_INTERVAL).await;
                }
            }
        });

        let runtime_options = AiRuntimeOptions::new(Some(run.id.clone()), Some(run.id.clone()))
            .with_caller_model(Some(model_config.model.clone()))
            .with_record_options(run_spec.record_options.clone())
            .with_callbacks(callbacks)
            .with_abort_checker(Some(Arc::new({
                let cancel_requested = Arc::clone(&cancel_requested);
                move |_| cancel_requested.load(Ordering::Relaxed)
            })));

        let execution = TaskRunExecution::new(runtime_config, run_spec);
        let report = match tokio::time::timeout(
            self.config.execution_timeout,
            execution.run_report_with_mcp_builder_and_options(mcp_builder, runtime_options),
        )
        .await
        {
            Ok(report) => report,
            Err(_) => TaskRunReport::from_ai_report(
                task.id.clone(),
                run.id.clone(),
                Some(model_config.id.clone()),
                AiTurnReport::failed(format!(
                    "execution timed out after {} seconds",
                    self.config.execution_timeout.as_secs()
                )),
            ),
        };
        stop_cancel_poll.store(true, Ordering::Relaxed);
        cancel_poll_handle.abort();
        flush_pending_stream_event(&self.store, run.id.as_str(), &pending_stream_event);

        let report_json = serde_json::to_value(&report).ok();
        let result_summary = summarized_report_content(&report.content);
        run.updated_at = now_rfc3339();
        run.finished_at = Some(report.completed_at.clone());
        run.result_summary = result_summary.clone();
        run.error_message = report.error.clone();
        run.usage = report.usage.clone();
        run.report = report_json.clone();
        run.cancel_requested = false;
        run.status = match report.status {
            chatos_ai_runtime::AiTurnStatus::Completed => TaskRunStatus::Succeeded,
            chatos_ai_runtime::AiTurnStatus::Failed => TaskRunStatus::Failed,
            chatos_ai_runtime::AiTurnStatus::Aborted => TaskRunStatus::Cancelled,
        };
        if let Err(err) = self.store.save_run(run.clone()).await {
            warn!("failed to persist completed task run {}: {}", run.id, err);
        }

        let event_type = match run.status {
            TaskRunStatus::Succeeded => "completed",
            TaskRunStatus::Failed => "failed",
            TaskRunStatus::Cancelled => "cancelled",
            TaskRunStatus::Blocked => "blocked",
            TaskRunStatus::Queued | TaskRunStatus::Running => "finished",
        };
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                event_type,
                Some(report.user_message()),
                report_json.clone(),
            ))
            .await
        {
            warn!(
                "failed to append completion event for run {}: {}",
                run.id, err
            );
        }

        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_record.status = match run.status {
                TaskRunStatus::Succeeded => TaskStatus::Succeeded,
                TaskRunStatus::Failed => TaskStatus::Failed,
                TaskRunStatus::Cancelled => TaskStatus::Cancelled,
                TaskRunStatus::Blocked => TaskStatus::Blocked,
                TaskRunStatus::Queued | TaskRunStatus::Running => TaskStatus::Running,
            };
            task_record.result_summary = result_summary;
            task_record.last_run_id = Some(run.id.clone());
            task_record.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_task(task_record).await {
                warn!("failed to persist completed task {}: {}", task.id, err);
            }
        }
        self.try_send_terminal_callback(task.id.as_str(), &run)
            .await;

        if matches!(run.status, TaskRunStatus::Succeeded)
            && self.config.memory_engine_base_url.is_some()
            && self.config.auto_memory_summary
        {
            if let Err(err) = self.trigger_memory_summary(&task, &mut run).await {
                if let Err(event_err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "memory_summary_error",
                        Some(format!("触发 Memory Engine 总结失败: {err}")),
                        None,
                    ))
                    .await
                {
                    warn!(
                        "failed to append memory summary error event for run {}: {}",
                        run.id, event_err
                    );
                }
                warn!(
                    "failed to trigger memory summary for run {}: {}",
                    run.id, err
                );
            }
        } else if matches!(run.status, TaskRunStatus::Succeeded)
            && self.config.memory_engine_base_url.is_some()
            && !self.config.auto_memory_summary
        {
            info!(
                run_id = run.id.as_str(),
                task_id = task.id.as_str(),
                task_title = task.title.as_str(),
                memory_thread_id = task.memory_thread_id.as_str(),
                "task runner skipped automatic memory summary because TASK_RUNNER_AUTO_MEMORY_SUMMARY is disabled"
            );
        }

        self.store.clear_cancel_requested(&run.id);
    }

    async fn ensure_task_thread(&self, task: &TaskRecord) -> Result<(), String> {
        let Some(client) = self.config.memory_client()? else {
            return Ok(());
        };
        client
            .upsert_thread(
                &task.memory_thread_id,
                &SdkUpsertThreadRequest {
                    tenant_id: task.tenant_id.clone(),
                    subject_id: task.subject_id.clone(),
                    thread_type: "task".to_string(),
                    external_thread_id: Some(task.id.clone()),
                    title: Some(task.title.clone()),
                    labels: Some(vec![
                        "task_runner".to_string(),
                        format!("task_status:{}", task.status.status_string()),
                    ]),
                    metadata: Some(json!({
                        "task_id": task.id,
                        "service": "task_runner_service",
                    })),
                    status: Some("active".to_string()),
                    created_at: None,
                    updated_at: None,
                    archived_at: None,
                },
            )
            .await
            .map(|_| ())
    }

    async fn compose_context_snapshot(&self, memory_scope: Option<&MemoryScope>) -> Option<Value> {
        let scope = memory_scope?;
        let Some(base_url) = self.config.memory_engine_base_url.clone() else {
            return None;
        };
        let composer = MemoryContextComposer::new_direct(
            base_url,
            self.config.memory_timeout,
            self.config.memory_engine_source_id.clone(),
        )
        .ok()?;
        match composer.compose(scope).await {
            Ok(response) => serde_json::to_value(response).ok(),
            Err(err) => {
                warn!("failed to compose context snapshot: {}", err);
                None
            }
        }
    }

    async fn trigger_memory_summary(
        &self,
        task: &TaskRecord,
        run: &mut TaskRunRecord,
    ) -> Result<(), String> {
        let Some(client) = self.config.memory_client()? else {
            return Ok(());
        };
        let response = client
            .run_thread_repair_summary(&task.memory_thread_id, &task.tenant_id)
            .await?;
        info!(
            run_id = run.id.as_str(),
            task_id = task.id.as_str(),
            task_title = task.title.as_str(),
            memory_thread_id = task.memory_thread_id.as_str(),
            summary_job_run_id = response.job_run_id.as_deref().unwrap_or(""),
            "task runner triggered memory summary job"
        );
        run.summary_job_run_id = response.job_run_id.clone();
        run.updated_at = now_rfc3339();
        self.store.save_run(run.clone()).await?;
        self.store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "memory_summary_requested",
                Some("已触发 Memory Engine repair summary".to_string()),
                Some(serde_json::to_value(response).unwrap_or_else(|_| json!({}))),
            ))
            .await?;
        Ok(())
    }

    async fn finish_cancelled_before_start(&self, task: &TaskRecord, run: &mut TaskRunRecord) {
        run.status = TaskRunStatus::Cancelled;
        run.cancel_requested = false;
        run.finished_at = Some(now_rfc3339());
        run.updated_at = now_rfc3339();
        if let Err(err) = self.store.save_run(run.clone()).await {
            warn!(
                "failed to persist pre-start cancelled run {}: {}",
                run.id, err
            );
        }
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                "cancelled",
                Some("任务在真正启动前已取消".to_string()),
                None,
            ))
            .await
        {
            warn!(
                "failed to append pre-start cancelled event for run {}: {}",
                run.id, err
            );
        }
        if let Ok(Some(mut task_record)) = self.store.get_task(&task.id).await {
            task_record.status = TaskStatus::Cancelled;
            task_record.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_task(task_record).await {
                warn!("failed to persist cancelled task {}: {}", task.id, err);
            }
        }
        self.try_send_terminal_callback(task.id.as_str(), run).await;
        self.store.clear_cancel_requested(&run.id);
    }

    async fn repair_stale_cancel_requested_runs(&self) -> Result<(), String> {
        let runs = self.store.list_runs(None).await?;
        for mut run in runs.into_iter().filter(|run| {
            run.cancel_requested
                && !matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running)
        }) {
            run.cancel_requested = false;
            run.updated_at = now_rfc3339();
            if let Err(err) = self.store.save_run(run.clone()).await {
                warn!(
                    "failed to repair stale cancel_requested flag for run {}: {}",
                    run.id, err
                );
            }
            self.store.clear_cancel_requested(&run.id);
        }
        Ok(())
    }

    async fn try_send_terminal_callback(&self, task_id: &str, run: &TaskRunRecord) {
        let event = match run.status {
            TaskRunStatus::Succeeded => "task.completed",
            TaskRunStatus::Failed
            | TaskRunStatus::Cancelled
            | TaskRunStatus::Blocked
            | TaskRunStatus::Queued
            | TaskRunStatus::Running => return,
        };
        self.try_send_task_callback(event, task_id, Some(run)).await;
    }

    async fn try_send_task_callback(
        &self,
        event: &str,
        task_id: &str,
        run: Option<&TaskRunRecord>,
    ) {
        let task = match load_task_snapshot_for_callback(&self.store, task_id).await {
            Ok(Some(task)) => task,
            Ok(None) => return,
            Err(err) => {
                warn!(
                    "failed to load callback task snapshot for task {} and event {}: {}",
                    task_id, event, err
                );
                return;
            }
        };
        let Some(payload) = build_chatos_task_callback_payload(event, &task, run, None) else {
            if task.source_session_id.is_some()
                || task.source_turn_id.is_some()
                || task.source_run_id.is_some()
            {
                warn!(
                    task_id = task.id.as_str(),
                    task_title = task.title.as_str(),
                    event,
                    source_session_id = task.source_session_id.as_deref().unwrap_or_default(),
                    source_turn_id = task.source_turn_id.as_deref().unwrap_or_default(),
                    source_user_message_id =
                        task.source_user_message_id.as_deref().unwrap_or_default(),
                    "skip task callback because source_user_message_id is missing"
                );
            }
            return;
        };
        let payload_task_id = payload.task_id.clone();
        let payload_run_id = payload.run_id.clone().unwrap_or_default();
        let payload_user_message_id = payload.source_user_message_id.clone().unwrap_or_default();
        let payload_event = payload.event.clone();
        if let Err(err) = send_chatos_task_callback(self.config.clone(), payload).await {
            warn!(
                "failed to send task callback for task {} and event {}: {}",
                task_id, event, err
            );
        } else {
            info!(
                task_id = payload_task_id.as_str(),
                run_id = payload_run_id.as_str(),
                event = payload_event.as_str(),
                source_user_message_id = payload_user_message_id.as_str(),
                "sent task callback to chatos"
            );
        }
    }

    fn apply_task_mcp_config(
        &self,
        mut runtime_config: TaskRuntimeConfig,
        mcp_config: &TaskMcpConfig,
    ) -> TaskRuntimeConfig {
        runtime_config = runtime_config
            .with_builtin_prompt_locale(mcp_config.locale())
            .with_builtin_prompt_mode(mcp_config.builtin_prompt_mode);
        if !mcp_config.enabled {
            runtime_config.with_mcp_init_mode(chatos_ai_runtime::TaskMcpInitMode::Disabled)
        } else {
            runtime_config.with_mcp_init_mode(mcp_config.init_mode)
        }
    }
}

impl McpCatalogService {
    pub fn new(task_service: TaskService, ui_prompt_service: UiPromptService) -> Self {
        Self {
            task_service,
            ui_prompt_service,
        }
    }

    pub fn list_catalog(&self) -> Vec<McpCatalogEntry> {
        let server_options =
            BuiltinMcpServerOptions::new(self.task_service.config.default_workspace_dir.clone())
                .with_auto_create_task(true);
        let runtime_defaults = default_runtime_builtin_kinds()
            .into_iter()
            .map(|kind| kind.kind_name().to_string())
            .collect::<Vec<_>>();
        configurable_builtin_kinds()
            .into_iter()
            .map(|kind| {
                let server = kind.server_with_options(&server_options);
                let guide = mcp_builtin_kind_guide(kind);
                let description = guide.description.to_string();
                let use_cases = guide
                    .use_cases
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect::<Vec<_>>();
                let capabilities = guide
                    .capabilities
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect::<Vec<_>>();
                match build_task_runner_builtin_provider(
                    &server,
                    self.task_service.clone(),
                    self.ui_prompt_service.clone(),
                ) {
                    Ok(Some(provider)) => {
                        let available_tool_names = provider
                            .list_tools()
                            .into_iter()
                            .filter_map(|tool| {
                                tool.get("name")
                                    .and_then(Value::as_str)
                                    .map(ToOwned::to_owned)
                            })
                            .collect::<Vec<_>>();
                        let unavailable_tools = provider
                            .unavailable_tools()
                            .into_iter()
                            .map(|(name, reason)| McpUnavailableTool { name, reason })
                            .collect::<Vec<_>>();
                        McpCatalogEntry {
                            kind: kind.kind_name().to_string(),
                            server_name: kind.server_name().to_string(),
                            config_id: kind.config_id().map(ToOwned::to_owned),
                            command: kind.command().map(ToOwned::to_owned),
                            description,
                            use_cases,
                            capabilities,
                            implemented: true,
                            runtime_default: runtime_defaults
                                .iter()
                                .any(|value| value == kind.kind_name()),
                            default_allow_writes: kind.default_allow_writes(),
                            available_tool_names,
                            unavailable_tools,
                            message: match kind {
                                chatos_mcp_runtime::BuiltinMcpKind::RemoteConnectionController => {
                                    Some("服务器列表来自 Task Runner 的“服务器”页面".to_string())
                                }
                                _ => None,
                            },
                        }
                    }
                    Ok(None) => McpCatalogEntry {
                        kind: kind.kind_name().to_string(),
                        server_name: kind.server_name().to_string(),
                        config_id: kind.config_id().map(ToOwned::to_owned),
                        command: kind.command().map(ToOwned::to_owned),
                        description,
                        use_cases,
                        capabilities,
                        implemented: false,
                        runtime_default: runtime_defaults
                            .iter()
                            .any(|value| value == kind.kind_name()),
                        default_allow_writes: kind.default_allow_writes(),
                        available_tool_names: Vec::new(),
                        unavailable_tools: Vec::new(),
                        message: Some(
                            "当前共享运行时尚未独立接线这个 builtin provider".to_string(),
                        ),
                    },
                    Err(err) => McpCatalogEntry {
                        kind: kind.kind_name().to_string(),
                        server_name: kind.server_name().to_string(),
                        config_id: kind.config_id().map(ToOwned::to_owned),
                        command: kind.command().map(ToOwned::to_owned),
                        description,
                        use_cases,
                        capabilities,
                        implemented: true,
                        runtime_default: runtime_defaults
                            .iter()
                            .any(|value| value == kind.kind_name()),
                        default_allow_writes: kind.default_allow_writes(),
                        available_tool_names: Vec::new(),
                        unavailable_tools: Vec::new(),
                        message: Some(err),
                    },
                }
            })
            .collect()
    }

    pub async fn preview_task_prompt(
        &self,
        task_id: &str,
    ) -> Result<Option<McpPromptPreviewResponse>, String> {
        let Some(task) = self.task_service.get_task(task_id).await? else {
            return Ok(None);
        };

        self.preview_prompt(McpPromptPreviewRequest {
            enabled: Some(task.mcp_config.enabled),
            init_mode: Some(task.mcp_config.init_mode),
            builtin_prompt_mode: Some(task.mcp_config.builtin_prompt_mode),
            builtin_prompt_locale: Some(task.mcp_config.builtin_prompt_locale),
            enabled_builtin_kinds: Some(task.mcp_config.enabled_builtin_kinds),
            workspace_dir: task.mcp_config.workspace_dir,
            default_remote_server_id: task.mcp_config.default_remote_server_id,
        })
        .map(Some)
    }

    pub fn preview_prompt(
        &self,
        request: McpPromptPreviewRequest,
    ) -> Result<McpPromptPreviewResponse, String> {
        let enabled = request.enabled.unwrap_or(true);
        let init_mode = request
            .init_mode
            .unwrap_or(chatos_ai_runtime::TaskMcpInitMode::BuiltinOnly);
        let builtin_prompt_mode = request
            .builtin_prompt_mode
            .unwrap_or(TaskBuiltinMcpPromptMode::Effective);
        let builtin_prompt_locale = request
            .builtin_prompt_locale
            .clone()
            .unwrap_or_else(|| BuiltinMcpPromptLocale::DEFAULT_KEY.to_string());
        let selected_kind_names = request.enabled_builtin_kinds.unwrap_or_default();

        let mcp_config = TaskMcpConfig {
            enabled,
            init_mode,
            builtin_prompt_mode,
            builtin_prompt_locale: builtin_prompt_locale.clone(),
            enabled_builtin_kinds: selected_kind_names,
            workspace_dir: normalized_optional(request.workspace_dir),
            default_remote_server_id: normalized_optional(request.default_remote_server_id),
        };
        let selected_builtin_kinds =
            if enabled && !matches!(init_mode, chatos_ai_runtime::TaskMcpInitMode::Disabled) {
                selected_builtin_kinds(&mcp_config)
            } else {
                Vec::new()
            };

        let mut server_options = BuiltinMcpServerOptions::new(resolve_workspace_dir_with_base(
            self.task_service.config.default_workspace_dir.as_str(),
            mcp_config.workspace_dir.as_deref(),
        ))
        .with_auto_create_task(true);
        if let Some(remote_server_id) = mcp_config.default_remote_server_id.clone() {
            server_options = server_options.with_remote_connection_id(remote_server_id);
        }
        let builtin_servers =
            builtin_servers_from_kinds(selected_builtin_kinds.clone(), &server_options);
        let (builtin_registry, _) = build_builtin_registry(
            &builtin_servers,
            self.task_service.clone(),
            self.ui_prompt_service.clone(),
        );
        let executor = McpExecutorBuilder::new()
            .with_builtin_servers(builtin_servers)
            .with_builtin_registry(builtin_registry)
            .build_builtin_only()?;
        let locale = BuiltinMcpPromptLocale::from_key(Some(&builtin_prompt_locale));
        let build = match builtin_prompt_mode {
            TaskBuiltinMcpPromptMode::Configured => {
                executor.inspect_builtin_mcp_system_prompt(locale)
            }
            TaskBuiltinMcpPromptMode::Effective => {
                executor.inspect_effective_builtin_mcp_system_prompt(locale)
            }
        };

        Ok(McpPromptPreviewResponse {
            enabled,
            init_mode,
            builtin_prompt_mode,
            builtin_prompt_locale,
            selected_builtin_kinds: selected_builtin_kinds
                .into_iter()
                .map(|kind| kind.kind_name().to_string())
                .collect(),
            build,
        })
    }
}

impl ToolingStateService {
    pub(crate) fn new(config: AppConfig) -> Self {
        Self { config }
    }

    pub async fn list_notepad_folders(&self, user_id: Option<&str>) -> Result<Value, String> {
        self.notepad_store(user_id)?.list_folders().await
    }

    pub async fn list_notepad_notes(
        &self,
        user_id: Option<&str>,
        folder: Option<String>,
        tags: Vec<String>,
        query: Option<String>,
        limit: Option<usize>,
        match_any: bool,
        recursive: bool,
    ) -> Result<Value, String> {
        self.notepad_store(user_id)?
            .list_notes(json!({
                "folder": folder,
                "recursive": recursive,
                "tags": tags,
                "match_any": match_any,
                "query": query,
                "limit": limit.unwrap_or(200).clamp(1, 500),
            }))
            .await
    }

    pub async fn read_notepad_note(
        &self,
        user_id: Option<&str>,
        note_id: &str,
    ) -> Result<Value, String> {
        self.notepad_store(user_id)?.read_note(note_id).await
    }

    pub async fn list_notepad_tags(&self, user_id: Option<&str>) -> Result<Value, String> {
        self.notepad_store(user_id)?.list_tags().await
    }

    pub async fn list_terminal_processes(
        &self,
        user_id: Option<String>,
        project_id: Option<String>,
        include_exited: bool,
        limit: usize,
    ) -> Result<Value, String> {
        TaskRunnerTerminalControllerStore
            .process_list(
                self.terminal_context(user_id, project_id),
                include_exited,
                limit.clamp(1, 100),
            )
            .await
    }

    pub async fn get_terminal_process_logs(
        &self,
        terminal_id: &str,
        user_id: Option<String>,
        project_id: Option<String>,
        offset: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Value, String> {
        TaskRunnerTerminalControllerStore
            .process_poll(
                self.terminal_context(user_id, project_id),
                terminal_id.to_string(),
                offset,
                limit.unwrap_or(200).clamp(1, 200),
            )
            .await
    }

    pub async fn kill_terminal_process(
        &self,
        terminal_id: &str,
        user_id: Option<String>,
        project_id: Option<String>,
    ) -> Result<Value, String> {
        TaskRunnerTerminalControllerStore
            .process_kill(
                self.terminal_context(user_id, project_id),
                terminal_id.to_string(),
            )
            .await
    }

    pub async fn write_terminal_process(
        &self,
        terminal_id: &str,
        user_id: Option<String>,
        project_id: Option<String>,
        data: String,
        submit: bool,
    ) -> Result<Value, String> {
        TaskRunnerTerminalControllerStore
            .process_write(
                self.terminal_context(user_id, project_id),
                terminal_id.to_string(),
                data,
                submit,
            )
            .await
    }

    fn notepad_store(&self, user_id: Option<&str>) -> Result<TaskRunnerNotepadStore, String> {
        let root = PathBuf::from(&self.config.default_workspace_dir)
            .join(".task_runner")
            .join("notepad");
        TaskRunnerNotepadStore::new(root, user_id.unwrap_or("task_runner"))
    }

    fn terminal_context(
        &self,
        user_id: Option<String>,
        project_id: Option<String>,
    ) -> TerminalControllerContext {
        TerminalControllerContext {
            root: PathBuf::from(&self.config.default_workspace_dir),
            user_id: normalized_optional(user_id),
            project_id: normalized_optional(project_id),
            idle_timeout_ms: 5_000,
            max_wait_ms: 60_000,
            max_output_chars: 20_000,
        }
    }
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
    task_execution_max_iterations: usize,
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
        execution_timeout_ms: config.execution_timeout.as_millis() as u64,
        scheduler_poll_interval_ms: config.scheduler_poll_interval.as_millis() as u64,
        auto_memory_summary: config.auto_memory_summary,
        default_task_execution_max_iterations: config.default_task_execution_max_iterations,
        task_execution_max_iterations,
    }
}

fn build_task_prompt(
    task: &TaskRecord,
    prompt_override: Option<&str>,
    prerequisite_context: &[PrerequisiteTaskContext],
) -> String {
    let current_task_prompt = if let Some(prompt) = prompt_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.to_string()
    } else {
        let mut parts = vec![
            format!("任务标题:\n{}", task.title),
            format!("任务目标:\n{}", task.objective),
        ];
        if let Some(description) = task
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            parts.push(format!("任务说明:\n{description}"));
        }
        if let Some(input_payload) = &task.input_payload {
            let payload_text = serde_json::to_string_pretty(input_payload)
                .unwrap_or_else(|_| input_payload.to_string());
            parts.push(format!("输入数据:\n{payload_text}"));
        }
        parts.push("请根据任务目标直接开始执行；如果有可用工具，请在必要时调用；最终给出清晰的结果、关键发现和后续建议。".to_string());
        parts.join("\n\n")
    };

    if prerequisite_context.is_empty() {
        return current_task_prompt;
    }

    format!(
        "{}\n\n当前任务:\n\n{}",
        format_prerequisite_context_for_prompt(prerequisite_context),
        current_task_prompt
    )
}

fn format_prerequisite_context_for_prompt(contexts: &[PrerequisiteTaskContext]) -> String {
    let mut parts = vec!["前置任务执行结果:".to_string()];
    for (index, context) in contexts.iter().enumerate() {
        let mut item = vec![
            format!(
                "{}. [{}] {} / {}",
                index + 1,
                context.status.status_string(),
                context.task_id,
                context.title
            ),
            format!("目标:\n{}", context.objective),
        ];
        if let Some(run_id) = context.run_id.as_deref() {
            item.push(format!("最近成功运行:\n{run_id}"));
        }
        if let Some(summary) = context
            .run_result_summary
            .as_deref()
            .or(context.result_summary.as_deref())
        {
            item.push(format!("结果摘要:\n{}", truncate_chars(summary, 2_000)));
        }
        if let Some(process_log) = context.process_log.as_deref() {
            item.push(format!("执行过程:\n{}", truncate_chars(process_log, 4_000)));
        }
        if let Some(content) = context.report_content.as_deref() {
            item.push(format!("关键输出:\n{}", truncate_chars(content, 4_000)));
        }
        parts.push(item.join("\n"));
    }
    truncate_chars(&parts.join("\n\n"), 20_000)
}

fn build_prerequisite_context(
    task: &TaskRecord,
    run: Option<&TaskRunRecord>,
) -> PrerequisiteTaskContext {
    PrerequisiteTaskContext {
        task_id: task.id.clone(),
        title: task.title.clone(),
        objective: task.objective.clone(),
        status: task.status,
        run_id: run.map(|run| run.id.clone()),
        result_summary: task.result_summary.clone(),
        run_result_summary: run.and_then(|run| run.result_summary.clone()),
        process_log: task.process_log.clone(),
        report_content: run.and_then(extract_report_content),
    }
}

fn extract_report_content(run: &TaskRunRecord) -> Option<String> {
    run.report
        .as_ref()
        .and_then(|report| report.get("content"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| truncate_chars(value, 4_000))
}

fn prerequisite_context_json(contexts: &[PrerequisiteTaskContext]) -> Value {
    json!(
        contexts
            .iter()
            .map(|context| {
                json!({
                    "task_id": context.task_id,
                    "title": context.title,
                    "objective": context.objective,
                    "status": context.status.status_string(),
                    "run_id": context.run_id,
                    "result_summary": context.result_summary,
                    "run_result_summary": context.run_result_summary,
                    "process_log": context.process_log,
                    "report_content": context.report_content,
                })
            })
            .collect::<Vec<_>>()
    )
}

fn attach_prerequisite_context_to_run(
    run: &mut TaskRunRecord,
    contexts: &[PrerequisiteTaskContext],
) {
    let context_json = prerequisite_context_json(contexts);
    if let Some(object) = run.input_snapshot.as_object_mut() {
        object.insert("resolved_prerequisites".to_string(), context_json);
    }
}

async fn load_task_snapshot_for_callback(
    store: &AppStore,
    task_id: &str,
) -> Result<Option<TaskRecord>, String> {
    let Some(mut task) = store.get_task(task_id).await? else {
        return Ok(None);
    };
    task.prerequisite_task_ids = store
        .list_task_prerequisites(task_id)
        .await?
        .into_iter()
        .map(|item| item.prerequisite_task_id)
        .collect();
    Ok(Some(task))
}

fn build_chatos_task_callback_payload(
    event: &str,
    task: &TaskRecord,
    run: Option<&TaskRunRecord>,
    error_message: Option<String>,
) -> Option<ChatosTaskCallbackPayload> {
    if task
        .source_user_message_id
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return None;
    }
    Some(ChatosTaskCallbackPayload {
        event: event.to_string(),
        task_id: task.id.clone(),
        run_id: run.map(|item| item.id.clone()),
        status: task.status.status_string().to_string(),
        task_title: task.title.clone(),
        result_summary: truncate_optional_chars(
            run.and_then(|item| item.result_summary.clone())
                .or_else(|| task.result_summary.clone()),
            2_000,
        ),
        error_message: truncate_optional_chars(
            error_message.or_else(|| run.and_then(|item| item.error_message.clone())),
            2_000,
        ),
        report_content: run
            .and_then(extract_report_content)
            .map(|value| truncate_chars(value.as_str(), 2_000)),
        process_log: None,
        source_session_id: task.source_session_id.clone(),
        source_turn_id: task.source_turn_id.clone(),
        source_user_message_id: task.source_user_message_id.clone(),
        parent_task_id: task.parent_task_id.clone(),
        source_run_id: task.source_run_id.clone(),
        prerequisite_task_ids: task.prerequisite_task_ids.clone(),
        schedule_mode: task.schedule.mode.mode_key().to_string(),
        callback_at: now_rfc3339(),
    })
}

async fn send_chatos_task_callback(
    config: AppConfig,
    payload: ChatosTaskCallbackPayload,
) -> Result<(), String> {
    let Some(url) = config.chatos_callback_url.clone() else {
        return Err("TASK_RUNNER_CHATOS_CALLBACK_URL not configured".to_string());
    };
    let client = reqwest::Client::builder()
        .timeout(config.callback_timeout)
        .build()
        .map_err(|err| err.to_string())?;
    let mut request = client.post(url).json(&payload);
    if let Some(secret) = config.chatos_callback_secret.clone() {
        request = request.header("X-Task-Runner-Callback-Secret", secret);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let body = response.text().await.unwrap_or_default();
    Err(format!("callback request failed: {status} {body}"))
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

fn truncate_chars(value: &str, _max_chars: usize) -> String {
    value.to_string()
}

fn truncate_optional_chars(value: Option<String>, max_chars: usize) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| truncate_chars(value.as_str(), max_chars))
}

fn summarized_report_content(content: &Option<String>) -> Option<String> {
    content
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn task_process_logging_enabled(mcp_config: &TaskMcpConfig) -> bool {
    mcp_config.enabled
        && !matches!(
            mcp_config.init_mode,
            chatos_ai_runtime::TaskMcpInitMode::Disabled
        )
}

fn task_process_log_builtin_server() -> McpBuiltinServer {
    McpBuiltinServer {
        name: TASK_PROCESS_LOG_INTERNAL_SERVER_NAME.to_string(),
        kind: TASK_PROCESS_LOG_INTERNAL_SERVER_NAME.to_string(),
        workspace_dir: String::new(),
        user_id: None,
        project_id: None,
        remote_connection_id: None,
        contact_agent_id: None,
        auto_create_task: false,
        allow_writes: true,
        max_file_bytes: 0,
        max_write_bytes: 0,
        search_limit: 0,
    }
}

fn task_process_log_prefixed_input_items(locale: BuiltinMcpPromptLocale) -> Vec<Value> {
    let tool_name = format!(
        "{}_{}",
        TASK_PROCESS_LOG_INTERNAL_SERVER_NAME, TASK_PROCESS_LOG_INTERNAL_TOOL_NAME
    );
    let text = if locale.is_english() {
        format!(
            "[Task Execution Process]\nA private internal tool `{tool_name}` is available during this task run. Use it to append visible progress notes, observations, verification results, blockers, and next steps for the current task only. Do not record hidden chain-of-thought, credentials, secrets, or unrelated drafts. This tool is internal to Task Runner execution and is not part of the external Task Runner MCP API."
        )
    } else {
        format!(
            "[任务执行过程]\n本次任务执行期间提供内部专用工具 `{tool_name}`。仅用它为当前任务追加可展示的进展、观察、验证结果、阻塞和下一步；不要记录隐藏思维链、凭证、密钥或无关草稿。这个工具只属于 Task Runner 内部执行，不属于对外 Task Runner MCP API。"
        )
    };
    vec![json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": text
        }]
    })]
}

#[derive(Debug, Deserialize)]
struct InternalRecordProcessArgs {
    #[serde(default)]
    operation: TaskProcessLogOperation,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    heading: Option<String>,
}

impl InternalRecordProcessArgs {
    fn into_request(self) -> RecordTaskProcessRequest {
        RecordTaskProcessRequest {
            operation: self.operation,
            content: self.content,
            heading: self.heading,
        }
    }
}

#[derive(Clone)]
struct TaskProcessLogBuiltinProvider {
    server_name: String,
    task_service: TaskService,
    task_id: String,
    run_id: String,
}

impl TaskProcessLogBuiltinProvider {
    fn new(
        server_name: impl Into<String>,
        task_service: TaskService,
        task_id: String,
        run_id: String,
    ) -> Self {
        Self {
            server_name: server_name.into(),
            task_service,
            task_id,
            run_id,
        }
    }
}

#[async_trait]
impl BuiltinToolProvider for TaskProcessLogBuiltinProvider {
    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }

    fn list_tools(&self) -> Vec<Value> {
        vec![json!({
            "name": TASK_PROCESS_LOG_INTERNAL_TOOL_NAME,
            "description": "Record visible execution process notes for the current Task Runner task only. Use append for progress, observations, verification results, blockers, and next steps. Do not record hidden chain-of-thought, credentials, secrets, or unrelated drafts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["append", "replace", "clear"],
                        "default": "append",
                        "description": "append adds one timestamped entry; replace rewrites the full process log; clear removes the process log."
                    },
                    "heading": {
                        "type": ["string", "null"],
                        "description": "Short visible heading for append entries, or null when not needed."
                    },
                    "content": {
                        "type": ["string", "null"],
                        "description": "Visible process content. Required for append/replace; pass null for clear."
                    }
                },
                "required": ["operation", "heading", "content"],
                "additionalProperties": false
            }
        })]
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _context: ToolCallContext,
        _on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        if name != TASK_PROCESS_LOG_INTERNAL_TOOL_NAME {
            return Err(format!("未知任务过程记录工具: {name}"));
        }
        let input: InternalRecordProcessArgs =
            serde_json::from_value(args).map_err(|err| err.to_string())?;
        let task = self
            .task_service
            .record_task_process(self.task_id.as_str(), input.into_request())
            .await?
            .ok_or_else(|| format!("任务不存在: {}", self.task_id))?;
        Ok(json!({
            "task_id": task.id,
            "run_id": self.run_id,
            "process_log": task.process_log,
            "updated_at": task.updated_at,
        }))
    }
}

fn build_builtin_registry(
    servers: &[McpBuiltinServer],
    task_service: TaskService,
    ui_prompt_service: UiPromptService,
) -> (BuiltinToolRegistry, Vec<String>) {
    let mut registry = BuiltinToolRegistry::new();
    let mut errors = Vec::new();
    for server in servers {
        match build_task_runner_builtin_provider(
            server,
            task_service.clone(),
            ui_prompt_service.clone(),
        ) {
            Ok(Some(provider)) => registry.register(provider),
            Ok(None) => {}
            Err(err) => errors.push(format!("{} 初始化失败: {err}", server.name)),
        }
    }
    (registry, errors)
}

#[derive(Clone)]
enum TaskRunnerBuiltinToolService {
    Shared(SharedBuiltinToolService),
    Notepad(NotepadBuiltinService),
    TaskManager(TaskManagerService),
    TerminalController(TerminalControllerService),
    UiPrompter(UiPrompterService),
}

impl TaskRunnerBuiltinToolService {
    fn list_tools(&self) -> Vec<Value> {
        match self {
            Self::Shared(service) => service.list_tools(),
            Self::Notepad(service) => service.list_tools(),
            Self::TaskManager(service) => service.list_tools(),
            Self::TerminalController(service) => service.list_tools(),
            Self::UiPrompter(service) => service.list_tools(),
        }
    }

    fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: &ToolCallContext,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        match self {
            Self::Shared(service) => service.call_tool(name, args, context, on_stream_chunk),
            Self::Notepad(service) => service.call_tool(name, args),
            Self::TaskManager(service) => {
                let callback = on_stream_chunk.map(|callback| -> TaskStreamChunkCallback {
                    Arc::new(move |chunk| callback(chunk))
                });
                service.call_tool(
                    name,
                    args,
                    context.conversation_id.as_deref(),
                    context.conversation_turn_id.as_deref(),
                    callback,
                )
            }
            Self::TerminalController(service) => {
                service.call_tool(name, args, context.conversation_id.as_deref())
            }
            Self::UiPrompter(service) => service.call_tool(
                name,
                args,
                context.conversation_id.as_deref(),
                context.conversation_turn_id.as_deref(),
                on_stream_chunk.map(|callback| {
                    Arc::new(move |chunk| callback(chunk))
                        as chatos_builtin_tools::UiPromptStreamChunkCallback
                }),
            ),
        }
    }

    fn unavailable_tools(&self) -> Vec<(String, String)> {
        match self {
            Self::Shared(service) => service.unavailable_tools(),
            Self::Notepad(_) => Vec::new(),
            Self::TaskManager(_) => Vec::new(),
            Self::TerminalController(_) => Vec::new(),
            Self::UiPrompter(_) => Vec::new(),
        }
    }
}

#[derive(Clone)]
struct TaskRunnerBuiltinProvider {
    server_name: String,
    service: TaskRunnerBuiltinToolService,
}

impl TaskRunnerBuiltinProvider {
    fn new(server_name: impl Into<String>, service: TaskRunnerBuiltinToolService) -> Self {
        Self {
            server_name: server_name.into(),
            service,
        }
    }
}

#[async_trait]
impl BuiltinToolProvider for TaskRunnerBuiltinProvider {
    fn server_name(&self) -> &str {
        self.server_name.as_str()
    }

    fn list_tools(&self) -> Vec<Value> {
        self.service.list_tools()
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        context: ToolCallContext,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        self.service
            .call_tool(name, args, &context, on_stream_chunk)
    }

    fn unavailable_tools(&self) -> Vec<(String, String)> {
        self.service.unavailable_tools()
    }
}

fn build_task_runner_builtin_provider(
    server: &McpBuiltinServer,
    task_service: TaskService,
    ui_prompt_service: UiPromptService,
) -> Result<Option<TaskRunnerBuiltinProvider>, String> {
    let Some(kind) = builtin_kind_by_any(server.kind.as_str()) else {
        return Ok(None);
    };
    match kind {
        chatos_mcp_runtime::BuiltinMcpKind::TaskManager => {
            let service = TaskManagerService::new(TaskManagerOptions {
                server_name: server.name.clone(),
                review_timeout_ms: REVIEW_TIMEOUT_MS_DEFAULT,
                auto_create_task: true,
                store: TaskManagerStoreRef::new(Arc::new(TaskRunnerTaskManagerStore::new(
                    task_service,
                ))),
            })?;
            Ok(Some(TaskRunnerBuiltinProvider::new(
                server.name.clone(),
                TaskRunnerBuiltinToolService::TaskManager(service),
            )))
        }
        chatos_mcp_runtime::BuiltinMcpKind::Notepad => {
            let user_id = server
                .user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("task_runner");
            let root = PathBuf::from(&task_service.config.default_workspace_dir)
                .join(".task_runner")
                .join("notepad");
            let store = TaskRunnerNotepadStore::new(root, user_id)?;
            let service = NotepadBuiltinService::new(NotepadOptions {
                server_name: server.name.clone(),
                store: NotepadStoreRef::new(Arc::new(store)),
            })?;
            Ok(Some(TaskRunnerBuiltinProvider::new(
                server.name.clone(),
                TaskRunnerBuiltinToolService::Notepad(service),
            )))
        }
        chatos_mcp_runtime::BuiltinMcpKind::TerminalController => {
            let service = TerminalControllerService::new(TerminalControllerOptions {
                root: PathBuf::from(&task_service.config.default_workspace_dir),
                user_id: server.user_id.clone(),
                project_id: server.project_id.clone(),
                idle_timeout_ms: 5_000,
                max_wait_ms: 60_000,
                max_output_chars: 20_000,
                store: TerminalControllerStoreRef::new(Arc::new(TaskRunnerTerminalControllerStore)),
            })?;
            Ok(Some(TaskRunnerBuiltinProvider::new(
                server.name.clone(),
                TaskRunnerBuiltinToolService::TerminalController(service),
            )))
        }
        chatos_mcp_runtime::BuiltinMcpKind::RemoteConnectionController => {
            let service =
                RemoteConnectionControllerService::new(RemoteConnectionControllerOptions {
                    server_name: server.name.clone(),
                    user_id: server
                        .user_id
                        .clone()
                        .or_else(|| Some(task_service.config.default_subject_id.clone())),
                    default_remote_connection_id: server.remote_connection_id.clone(),
                    command_timeout_seconds: 20,
                    max_command_timeout_seconds: 120,
                    max_output_chars: 20_000,
                    max_read_file_bytes: 256 * 1024,
                    store: RemoteConnectionControllerStoreRef::new(Arc::new(
                        TaskRunnerRemoteConnectionStore::new(task_service.store.clone()),
                    )),
                })?;
            Ok(Some(TaskRunnerBuiltinProvider::new(
                server.name.clone(),
                TaskRunnerBuiltinToolService::Shared(
                    SharedBuiltinToolService::RemoteConnectionController(service),
                ),
            )))
        }
        chatos_mcp_runtime::BuiltinMcpKind::UiPrompter => {
            let service = UiPrompterService::new(UiPrompterOptions {
                server_name: server.name.clone(),
                prompt_timeout_ms: UI_PROMPT_TIMEOUT_MS_DEFAULT,
                store: UiPrompterStoreRef::new(Arc::new(ui_prompt_service)),
            })?;
            Ok(Some(TaskRunnerBuiltinProvider::new(
                server.name.clone(),
                TaskRunnerBuiltinToolService::UiPrompter(service),
            )))
        }
        _ => Ok(build_shared_builtin_tool_service(server)?.map(|service| {
            TaskRunnerBuiltinProvider::new(
                server.name.clone(),
                TaskRunnerBuiltinToolService::Shared(service),
            )
        })),
    }
}

fn selected_builtin_kinds(mcp_config: &TaskMcpConfig) -> Vec<chatos_mcp_runtime::BuiltinMcpKind> {
    let mut kinds = mcp_config
        .enabled_builtin_kinds
        .iter()
        .filter_map(|value| builtin_kind_by_any(value))
        .collect::<Vec<_>>();
    if kinds.is_empty() && mcp_config.enabled {
        kinds = configurable_builtin_kinds();
    }
    kinds
}

fn normalize_builtin_kind_names(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .filter_map(|value| builtin_kind_by_any(&value))
        .map(|kind| kind.kind_name().to_string())
        .collect()
}

fn task_belongs_to_context(task: &TaskRecord, root_task_id: &str) -> bool {
    task.id == root_task_id || task.parent_task_id.as_deref() == Some(root_task_id)
}

fn task_to_manager_value(task: &TaskRecord) -> Value {
    json!({
        "id": task.id.clone(),
        "parent_task_id": task.parent_task_id.clone(),
        "source_run_id": task.source_run_id.clone(),
        "title": task.title.clone(),
        "details": task
            .description
            .clone()
            .or_else(|| normalized_optional(Some(task.objective.clone()))),
        "priority": task_priority_to_manager_label(task.priority),
        "status": task_manager_status_from_task_status(task.status),
        "tags": task.tags.clone(),
        "due_at": task.task_tool_state.due_at.clone(),
        "outcome_summary": task.result_summary.clone(),
        "outcome_items": tool_state_outcomes_into_shared(&task.task_tool_state.outcome_items),
        "resume_hint": task.task_tool_state.resume_hint.clone(),
        "blocker_reason": task.task_tool_state.blocker_reason.clone(),
        "blocker_needs": task.task_tool_state.blocker_needs.clone(),
        "blocker_kind": task.task_tool_state.blocker_kind.clone(),
        "completed_at": task.task_tool_state.completed_at.clone(),
        "last_outcome_at": task.task_tool_state.last_outcome_at.clone(),
        "created_at": task.created_at.clone(),
        "updated_at": task.updated_at.clone(),
    })
}

fn apply_manager_patch(
    task: &mut TaskRecord,
    patch: SharedTaskUpdatePatch,
    mark_complete: bool,
    now: &str,
) -> Result<(), String> {
    let requested_status = patch.status.as_deref().map(task_status_from_manager_status);
    if let Some(title) = patch.title {
        validate_required("title", &title)?;
        task.title = title.trim().to_string();
    }
    if let Some(details) = patch.details {
        let normalized = normalized_optional(Some(details));
        task.description = normalized.clone();
        if task.parent_task_id.is_some() {
            task.objective = normalized.unwrap_or_else(|| task.title.clone());
        }
    }
    if let Some(priority) = patch.priority {
        task.priority = task_priority_from_manager_label(priority.as_str());
    }
    if let Some(status) = requested_status {
        task.status = status;
    }
    if let Some(tags) = patch.tags {
        task.tags = normalize_strings(tags);
    }
    if let Some(due_at) = patch.due_at {
        task.task_tool_state.due_at = normalized_optional_nested(due_at);
    }
    if let Some(outcome_summary) = patch.outcome_summary {
        task.result_summary = normalized_optional(Some(outcome_summary));
        if task.result_summary.is_some() {
            task.task_tool_state.last_outcome_at = Some(now.to_string());
        }
    }
    if let Some(outcome_items) = patch.outcome_items {
        task.task_tool_state.outcome_items = shared_outcome_items_into_tool_state(outcome_items);
        if !task.task_tool_state.outcome_items.is_empty() {
            task.task_tool_state.last_outcome_at = Some(now.to_string());
        }
    }
    if let Some(resume_hint) = patch.resume_hint {
        task.task_tool_state.resume_hint = normalized_optional(Some(resume_hint));
    }
    if let Some(blocker_reason) = patch.blocker_reason {
        task.task_tool_state.blocker_reason = normalized_optional(Some(blocker_reason));
    }
    if let Some(blocker_needs) = patch.blocker_needs {
        task.task_tool_state.blocker_needs = normalize_strings(blocker_needs);
    }
    if let Some(blocker_kind) = patch.blocker_kind {
        task.task_tool_state.blocker_kind = normalized_optional(Some(blocker_kind));
    }
    if let Some(completed_at) = patch.completed_at {
        task.task_tool_state.completed_at = normalized_optional_nested(completed_at);
    }
    if let Some(last_outcome_at) = patch.last_outcome_at {
        task.task_tool_state.last_outcome_at = normalized_optional_nested(last_outcome_at);
    }
    if mark_complete || matches!(task.status, TaskStatus::Succeeded) {
        task.status = TaskStatus::Succeeded;
        if task.task_tool_state.completed_at.is_none() {
            task.task_tool_state.completed_at = Some(now.to_string());
        }
        if task.task_tool_state.last_outcome_at.is_none() {
            task.task_tool_state.last_outcome_at = Some(now.to_string());
        }
    }
    Ok(())
}

fn task_status_from_manager_status(value: &str) -> TaskStatus {
    match value.trim().to_ascii_lowercase().as_str() {
        "doing" => TaskStatus::Running,
        "blocked" => TaskStatus::Blocked,
        "done" => TaskStatus::Succeeded,
        _ => TaskStatus::Ready,
    }
}

fn task_manager_status_from_task_status(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Running => "doing",
        TaskStatus::Blocked | TaskStatus::Failed => "blocked",
        TaskStatus::Succeeded | TaskStatus::Cancelled | TaskStatus::Archived => "done",
        TaskStatus::Draft | TaskStatus::Ready => "todo",
    }
}

fn task_priority_from_manager_label(value: &str) -> i32 {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" => 100,
        "low" => 10,
        _ => 50,
    }
}

fn task_priority_to_manager_label(value: i32) -> &'static str {
    if value >= 80 {
        "high"
    } else if value <= 20 {
        "low"
    } else {
        "medium"
    }
}

fn shared_outcome_items_into_tool_state(
    items: Vec<SharedTaskOutcomeItem>,
) -> Vec<TaskToolOutcomeItem> {
    items
        .into_iter()
        .map(|item| TaskToolOutcomeItem {
            kind: item.kind,
            text: item.text,
            importance: item.importance,
            refs: item.refs,
        })
        .collect()
}

fn tool_state_outcomes_into_shared(items: &[TaskToolOutcomeItem]) -> Vec<SharedTaskOutcomeItem> {
    items
        .iter()
        .map(|item| SharedTaskOutcomeItem {
            kind: item.kind.clone(),
            text: item.text.clone(),
            importance: item.importance.clone(),
            refs: item.refs.clone(),
        })
        .collect()
}

fn sanitize_task_mcp_config(mut config: TaskMcpConfig) -> TaskMcpConfig {
    config.builtin_prompt_locale = normalized_optional(Some(config.builtin_prompt_locale))
        .unwrap_or_else(|| chatos_mcp_runtime::BuiltinMcpPromptLocale::DEFAULT_KEY.to_string());
    config.enabled_builtin_kinds = normalize_builtin_kind_names(config.enabled_builtin_kinds);
    config.workspace_dir = normalized_optional(config.workspace_dir);
    config.default_remote_server_id = normalized_optional(config.default_remote_server_id);
    config
}

fn ensure_effective_task_workspace_dir(
    config: &AppConfig,
    task: &TaskRecord,
    model_config: &ModelConfigRecord,
) -> Result<String, String> {
    ensure_workspace_dir_available(
        config.default_workspace_dir.as_str(),
        task.mcp_config
            .workspace_dir
            .as_deref()
            .or(model_config.request_cwd.as_deref()),
    )
}

fn resolve_workspace_dir_with_base(base_dir: &str, configured: Option<&str>) -> String {
    let candidate = configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(base_dir);
    let path = PathBuf::from(candidate);
    let resolved = if path.is_absolute() {
        path
    } else {
        PathBuf::from(base_dir).join(path)
    };
    std::fs::canonicalize(&resolved)
        .unwrap_or(resolved)
        .to_string_lossy()
        .to_string()
}

fn ensure_workspace_dir_available(
    base_dir: &str,
    configured: Option<&str>,
) -> Result<String, String> {
    let resolved = resolve_workspace_dir_with_base(base_dir, configured);
    let path = PathBuf::from(&resolved);

    match std::fs::metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(format!("工作目录不是目录: {}", path.display()));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(&path).map_err(|create_err| {
                format!(
                    "create workspace dir {} failed: {}",
                    path.display(),
                    create_err
                )
            })?;
        }
        Err(err) => {
            return Err(format!(
                "read workspace dir {} failed: {}",
                path.display(),
                err
            ));
        }
    }

    Ok(path
        .canonicalize()
        .unwrap_or(path)
        .to_string_lossy()
        .to_string())
}

#[derive(Debug, Default)]
struct PendingRunStreamEvent {
    event_type: Option<&'static str>,
    text: String,
    chunk_count: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct FlushedRunStreamEvent {
    event_type: &'static str,
    text: String,
    chunk_count: usize,
}

impl PendingRunStreamEvent {
    fn push(&mut self, event_type: &'static str, chunk: &str) -> Option<FlushedRunStreamEvent> {
        let flushed = if self.event_type.is_some() && self.event_type != Some(event_type) {
            self.take()
        } else {
            None
        };

        if self.event_type.is_none() {
            self.event_type = Some(event_type);
        }
        self.text.push_str(chunk);
        self.chunk_count += 1;
        flushed
    }

    fn take(&mut self) -> Option<FlushedRunStreamEvent> {
        let event_type = self.event_type.take()?;
        let text = std::mem::take(&mut self.text);
        let chunk_count = std::mem::take(&mut self.chunk_count);
        if text.is_empty() {
            return None;
        }
        Some(FlushedRunStreamEvent {
            event_type,
            text,
            chunk_count,
        })
    }
}

fn flush_pending_stream_event(
    store: &AppStore,
    run_id: &str,
    pending: &Arc<parking_lot::Mutex<PendingRunStreamEvent>>,
) {
    let flushed = {
        let mut state = pending.lock();
        state.take()
    };
    if let Some(flushed) = flushed {
        append_pending_stream_event(store, run_id, flushed);
    }
}

fn append_pending_stream_event(store: &AppStore, run_id: &str, event: FlushedRunStreamEvent) {
    let chunk_chars = event.text.chars().count();
    store.append_run_event_sync(TaskRunEventRecord::new(
        run_id.to_string(),
        event.event_type,
        None,
        Some(json!({
            "text": event.text,
            "chunk_count": event.chunk_count,
            "chunk_chars": chunk_chars,
        })),
    ));
}

fn sanitize_task_schedule_config(
    mut schedule: TaskScheduleConfig,
    existing: Option<&TaskScheduleConfig>,
) -> Result<TaskScheduleConfig, String> {
    schedule.run_at = normalized_optional(schedule.run_at);
    schedule.next_run_at = normalized_optional(schedule.next_run_at);
    schedule.last_scheduled_at = existing
        .and_then(|item| item.last_scheduled_at.clone())
        .or(schedule.last_scheduled_at);

    match schedule.mode {
        TaskScheduleMode::Manual => {
            schedule.run_at = None;
            schedule.interval_seconds = None;
            schedule.next_run_at = None;
            schedule.last_scheduled_at = existing.and_then(|item| item.last_scheduled_at.clone());
        }
        TaskScheduleMode::Once => {
            let run_at = schedule
                .run_at
                .clone()
                .ok_or_else(|| "一次性调度必须提供执行时间".to_string())?;
            ensure_rfc3339("schedule.run_at", &run_at)?;
            schedule.interval_seconds = None;
            schedule.next_run_at = Some(run_at);
        }
        TaskScheduleMode::Interval => {
            let seconds = schedule
                .interval_seconds
                .ok_or_else(|| "循环调度必须提供间隔秒数".to_string())?;
            if seconds < 60 {
                return Err("循环调度的最小间隔为 60 秒".to_string());
            }
            if let Some(run_at) = schedule.run_at.clone() {
                ensure_rfc3339("schedule.run_at", &run_at)?;
                if schedule.next_run_at.is_none() {
                    schedule.next_run_at = Some(run_at);
                }
            }
            if let Some(next_run_at) = schedule.next_run_at.clone() {
                ensure_rfc3339("schedule.next_run_at", &next_run_at)?;
            } else {
                schedule.next_run_at = existing
                    .and_then(|item| item.next_run_at.clone())
                    .or_else(|| Some(now_rfc3339()));
            }
        }
        TaskScheduleMode::ContactAsync => {
            let run_at = schedule
                .run_at
                .clone()
                .ok_or_else(|| "联系人异步调度必须提供执行时间".to_string())?;
            ensure_rfc3339("schedule.run_at", &run_at)?;
            schedule.interval_seconds = None;
            schedule.next_run_at = Some(run_at);
        }
    }

    Ok(schedule)
}

fn sanitize_task_memory_context_policy(options: TaskMemoryContextOptions) -> ComposeContextPolicy {
    ComposeContextPolicy {
        include_recent_records: Some(options.include_recent_records.unwrap_or(true)),
        include_thread_summary: Some(options.include_thread_summary.unwrap_or(true)),
        include_subject_memory: Some(options.include_subject_memory.unwrap_or(false)),
        recent_record_limit: Some(options.recent_record_limit.unwrap_or(12).clamp(1, 100)),
        summary_limit: Some(options.summary_limit.unwrap_or(6).clamp(1, 50)),
    }
}

fn sanitize_task_memory_records_options(
    options: TaskMemoryRecordsOptions,
) -> TaskMemoryRecordsOptions {
    let limit = options.limit.unwrap_or(50).clamp(1, 200);
    let offset = options.offset.unwrap_or(0).max(0);
    let order = normalized_optional(options.order)
        .map(|value| {
            if value.eq_ignore_ascii_case("asc") {
                "asc".to_string()
            } else {
                "desc".to_string()
            }
        })
        .unwrap_or_else(|| "desc".to_string());

    TaskMemoryRecordsOptions {
        role: normalized_optional(options.role),
        record_type: normalized_optional(options.record_type),
        summary_status: normalized_optional(options.summary_status),
        limit: Some(limit),
        offset: Some(offset),
        order: Some(order),
    }
}

fn sanitize_task_list_filters(mut filters: TaskListFilters) -> TaskListFilters {
    filters.keyword = normalized_optional(filters.keyword).map(|value| value.to_ascii_lowercase());
    filters.tag = normalized_optional(filters.tag);
    filters.model_config_id = normalized_optional(filters.model_config_id);
    filters.creator_user_id = normalized_optional(filters.creator_user_id);
    filters.parent_task_id = normalized_optional(filters.parent_task_id);
    filters.source_run_id = normalized_optional(filters.source_run_id);
    filters.limit = filters.limit.map(|value| value.clamp(1, 500));
    filters.offset = filters.offset.map(|value| value.min(100_000));
    filters
}

fn sanitize_run_list_filters(mut filters: RunListFilters) -> RunListFilters {
    filters.task_id = normalized_optional(filters.task_id);
    filters.model_config_id = normalized_optional(filters.model_config_id);
    filters.keyword = normalized_optional(filters.keyword).map(|value| value.to_ascii_lowercase());
    filters.limit = filters.limit.map(|value| value.clamp(1, 500));
    filters.offset = filters.offset.map(|value| value.min(100_000));
    filters
}

pub(crate) fn sanitize_prompt_list_filters(mut filters: PromptListFilters) -> PromptListFilters {
    filters.task_id = normalized_optional(filters.task_id);
    filters.run_id = normalized_optional(filters.run_id);
    filters.limit = Some(filters.limit.unwrap_or(20).clamp(1, 500));
    filters.offset = Some(filters.offset.unwrap_or(0).min(100_000));
    filters
}

fn advance_task_schedule_after_dispatch(
    schedule: &TaskScheduleConfig,
    started_at: DateTime<Utc>,
) -> Result<TaskScheduleConfig, String> {
    let mut next = schedule.clone();
    next.last_scheduled_at = Some(started_at.to_rfc3339());
    match next.mode {
        TaskScheduleMode::Manual => {
            next.next_run_at = None;
        }
        TaskScheduleMode::Once => {
            next.next_run_at = None;
        }
        TaskScheduleMode::Interval => {
            let seconds = next
                .interval_seconds
                .ok_or_else(|| "循环调度缺少 interval_seconds".to_string())?;
            next.next_run_at = Some((started_at + ChronoDuration::seconds(seconds)).to_rfc3339());
        }
        TaskScheduleMode::ContactAsync => {
            next.next_run_at = None;
        }
    }
    Ok(next)
}

fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|item| item.with_timezone(&Utc))
}

fn ensure_rfc3339(label: &str, value: &str) -> Result<(), String> {
    if parse_rfc3339(value).is_some() {
        Ok(())
    } else {
        Err(format!("{label} 必须是 RFC3339 时间"))
    }
}

fn normalize_batch_task_ids(task_ids: Vec<String>) -> Result<Vec<String>, String> {
    let task_ids = task_ids
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    if task_ids.is_empty() {
        Err("task_ids 不能为空".to_string())
    } else {
        Ok(task_ids)
    }
}

fn sanitize_id_list(ids: Vec<String>) -> Vec<String> {
    ids.into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .take(200)
        .collect()
}

fn normalize_prerequisite_task_ids(ids: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        let id = id.trim().to_string();
        if id.is_empty() || out.iter().any(|item| item == &id) {
            continue;
        }
        out.push(id);
        if out.len() >= 50 {
            break;
        }
    }
    out
}

fn summarize_batch_results(results: Vec<BatchTaskOperationItem>) -> BatchTaskOperationResponse {
    let total = results.len();
    let succeeded = results.iter().filter(|item| item.ok).count();
    let failed = total.saturating_sub(succeeded);
    BatchTaskOperationResponse {
        total,
        succeeded,
        failed,
        results,
    }
}

fn normalize_tags(tags: Option<Vec<String>>) -> Vec<String> {
    tags.unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
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

fn apply_task_process_log_update(
    current: Option<String>,
    input: RecordTaskProcessRequest,
    now: &str,
) -> Result<Option<String>, String> {
    match input.operation {
        TaskProcessLogOperation::Clear => Ok(None),
        TaskProcessLogOperation::Replace => {
            let content = normalized_optional(input.content);
            validate_process_log_length(content.as_deref())?;
            Ok(content)
        }
        TaskProcessLogOperation::Append => {
            let content =
                normalized_optional(input.content).ok_or_else(|| "content 不能为空".to_string())?;
            let entry = format_task_process_entry(now, input.heading, content);
            let next = match normalized_optional(current) {
                Some(existing) => format!("{existing}\n\n{entry}"),
                None => entry,
            };
            validate_process_log_length(Some(next.as_str()))?;
            Ok(Some(next))
        }
    }
}

fn format_task_process_entry(now: &str, heading: Option<String>, content: String) -> String {
    let heading = normalized_optional(heading);
    match heading {
        Some(heading) => format!("[{now}] {heading}\n{content}"),
        None => format!("[{now}]\n{content}"),
    }
}

fn validate_process_log_length(value: Option<&str>) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let len = value.chars().count();
    if len > TASK_PROCESS_LOG_MAX_CHARS {
        Err(format!(
            "过程记录不能超过 {TASK_PROCESS_LOG_MAX_CHARS} 字符，当前 {len} 字符"
        ))
    } else {
        Ok(())
    }
}

fn validate_required(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{label} 不能为空"))
    } else {
        Ok(())
    }
}

fn build_remote_server_record(
    input: CreateRemoteServerRequest,
    creator: Option<&CurrentUser>,
    task_id: Option<String>,
    now: String,
) -> Result<RemoteServerRecord, String> {
    validate_required("name", &input.name)?;
    validate_required("host", &input.host)?;
    validate_required("username", &input.username)?;
    validate_required("auth_type", &input.auth_type)?;

    let record = RemoteServerRecord {
        id: Uuid::new_v4().to_string(),
        name: input.name.trim().to_string(),
        host: input.host.trim().to_string(),
        port: normalize_remote_server_port(input.port)?,
        username: input.username.trim().to_string(),
        auth_type: normalize_remote_server_auth_type(&input.auth_type)?,
        password: normalized_optional(input.password),
        private_key_path: normalized_optional(input.private_key_path),
        certificate_path: normalized_optional(input.certificate_path),
        default_remote_path: normalized_optional(input.default_remote_path),
        host_key_policy: normalize_remote_server_host_key_policy(input.host_key_policy.as_deref())?,
        enabled: input.enabled.unwrap_or(true),
        last_tested_at: None,
        last_test_status: None,
        last_test_message: None,
        last_active_at: None,
        creator_user_id: creator.map(|user| user.id.clone()),
        creator_username: creator.map(|user| user.username.clone()),
        creator_display_name: creator.map(|user| user.display_name.clone()),
        task_id,
        created_at: now.clone(),
        updated_at: now,
    };
    validate_remote_server_auth_fields(&record)?;
    Ok(record)
}

fn normalize_remote_server_port(value: Option<i64>) -> Result<i64, String> {
    let port = value.unwrap_or(22);
    if port <= 0 || port > u16::MAX as i64 {
        Err("port 必须在 1-65535 之间".to_string())
    } else {
        Ok(port)
    }
}

fn normalize_remote_server_auth_type(value: &str) -> Result<String, String> {
    let normalized = value.trim();
    match normalized {
        "password" | "private_key" | "private_key_cert" => Ok(normalized.to_string()),
        _ => Err("auth_type 仅支持 password / private_key / private_key_cert".to_string()),
    }
}

fn normalize_remote_server_host_key_policy(value: Option<&str>) -> Result<String, String> {
    let normalized = value
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .unwrap_or("accept_new");
    match normalized {
        "accept_new" | "strict" => Ok(normalized.to_string()),
        _ => Err("host_key_policy 仅支持 accept_new / strict".to_string()),
    }
}

fn validate_remote_server_auth_fields(record: &RemoteServerRecord) -> Result<(), String> {
    match record.auth_type.as_str() {
        "password" => {
            if record
                .password
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                return Err("password 模式需要提供 password".to_string());
            }
        }
        "private_key" | "private_key_cert" => {
            if record
                .private_key_path
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                return Err("private_key 模式需要提供 private_key_path".to_string());
            }
        }
        _ => return Err("不支持的认证方式".to_string()),
    }
    Ok(())
}

trait TaskStatusExt {
    fn status_string(&self) -> &'static str;
}

impl TaskStatusExt for TaskStatus {
    fn status_string(&self) -> &'static str {
        match self {
            TaskStatus::Draft => "draft",
            TaskStatus::Ready => "ready",
            TaskStatus::Running => "running",
            TaskStatus::Succeeded => "succeeded",
            TaskStatus::Failed => "failed",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Cancelled => "cancelled",
            TaskStatus::Archived => "archived",
        }
    }
}

impl TaskStatusExt for TaskRunStatus {
    fn status_string(&self) -> &'static str {
        match self {
            TaskRunStatus::Queued => "queued",
            TaskRunStatus::Running => "running",
            TaskRunStatus::Succeeded => "succeeded",
            TaskRunStatus::Failed => "failed",
            TaskRunStatus::Cancelled => "cancelled",
            TaskRunStatus::Blocked => "blocked",
        }
    }
}

trait TaskScheduleModeExt {
    fn mode_key(&self) -> &'static str;
}

impl TaskScheduleModeExt for TaskScheduleMode {
    fn mode_key(&self) -> &'static str {
        match self {
            TaskScheduleMode::Manual => "manual",
            TaskScheduleMode::Once => "once",
            TaskScheduleMode::Interval => "interval",
            TaskScheduleMode::ContactAsync => "contact_async",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FlushedRunStreamEvent, PendingRunStreamEvent, ensure_workspace_dir_available,
        resolve_workspace_dir_with_base,
    };
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

    #[test]
    fn pending_run_stream_event_merges_same_type_chunks() {
        let mut pending = PendingRunStreamEvent::default();

        assert_eq!(pending.push("chunk", "hello"), None);
        assert_eq!(pending.push("chunk", " world"), None);
        assert_eq!(
            pending.take(),
            Some(FlushedRunStreamEvent {
                event_type: "chunk",
                text: "hello world".to_string(),
                chunk_count: 2,
            })
        );
    }

    #[test]
    fn pending_run_stream_event_flushes_when_type_changes() {
        let mut pending = PendingRunStreamEvent::default();

        assert_eq!(pending.push("thinking", "step1"), None);
        assert_eq!(
            pending.push("chunk", "answer"),
            Some(FlushedRunStreamEvent {
                event_type: "thinking",
                text: "step1".to_string(),
                chunk_count: 1,
            })
        );
        assert_eq!(
            pending.take(),
            Some(FlushedRunStreamEvent {
                event_type: "chunk",
                text: "answer".to_string(),
                chunk_count: 1,
            })
        );
    }
}
