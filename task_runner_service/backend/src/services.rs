use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, Mutex as AsyncMutex};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::config::AppConfig;
use crate::models::{
    now_rfc3339, BatchTaskDeleteRequest, BatchTaskOperationItem, BatchTaskOperationResponse,
    BatchTaskRunRequest, BatchTaskStatusUpdateRequest, CreateTaskRequest, HealthResponse,
    PaginatedResponse, RecordTaskProcessRequest, RunListFilters,
    RunSummaryRecord, RuntimeSettingsRecord, StartTaskRunRequest, SystemConfigResponse,
    TaskIndexResponse, TaskListFilters, TaskMcpConfig, TaskRecord, TaskRunEventRecord,
    TaskRunRecord, TaskRunStatus, TaskSourceContext, TaskStatsResponse, TaskStatus,
    TaskSummaryRecord, TaskToolState, UpdateRuntimeSettingsRequest, UpdateTaskMcpRequest,
    UpdateTaskRequest,
};
use crate::store::AppStore;
use crate::ui_prompt_service::UiPromptService;

mod chatos_callbacks;
mod builtin_providers;
mod batch_ops;
mod chatos_message_tasks;
mod mcp_catalog_service;
mod memory_options;
mod model_catalog;
mod model_config_service;
mod prerequisite_context;
mod remote_servers;
mod remote_server_service;
mod schedule_helpers;
mod filter_sanitize;
mod status_display;
mod stream_events;
mod task_manager_bridge;
mod task_dependencies;
mod task_process_log;
mod task_memory;
mod process_log_text;
mod run_execution_support;
mod run_control;
mod run_recovery;
mod run_model_phase;
mod run_prerequisites;
mod tooling_state;
mod workspace_mcp;

use self::builtin_providers::build_builtin_registry;
use self::batch_ops::{
    normalize_batch_task_ids, normalize_prerequisite_task_ids, normalize_tags, sanitize_id_list,
    summarize_batch_results,
};
pub(crate) use self::filter_sanitize::sanitize_prompt_list_filters;
use self::filter_sanitize::{sanitize_run_list_filters, sanitize_task_list_filters};
use self::process_log_text::apply_task_process_log_update;
use self::remote_servers::build_remote_server_record;
use self::schedule_helpers::{
    advance_task_schedule_after_dispatch, sanitize_task_schedule_config,
};
use self::status_display::{TaskScheduleModeExt, TaskStatusExt};
use self::workspace_mcp::{
    ensure_workspace_dir_available, normalize_builtin_kind_names, sanitize_task_mcp_config,
};
pub use self::chatos_message_tasks::{
    ChatosMessageRunDetail, ChatosMessageTaskDetail, ChatosMessageTaskRun,
    ChatosMessageTaskRunEvent, ChatosMessageTaskSummary,
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
    use super::workspace_mcp::{
        ensure_workspace_dir_available, resolve_workspace_dir_with_base,
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
}
