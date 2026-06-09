use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    bson::{self, doc, Bson, Document},
    options::{FindOptions, IndexOptions, ReplaceOptions},
    Client, Collection, IndexModel,
};
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};
use tokio::sync::broadcast;
use tracing::warn;

use crate::config::{AppConfig, StoreMode};
use crate::models::{
    ModelConfigRecord, ModelConfigUsageRecord, PaginatedResponse, PromptListFilters,
    RemoteServerRecord, RunListFilters, RunSummaryRecord, TaskListFilters, TaskRecord,
    TaskRunEventRecord, TaskRunRecord, TaskRunStatus, TaskScheduleMode, TaskStatsResponse,
    TaskStatus, TaskSummaryRecord, UiPromptRecord, UiPromptStatus, UiPromptTaskCountRecord,
    UserRecord,
};

const ACTIVE_TASK_RUN_UNIQUE_INDEX_NAME: &str = "idx_task_runs_active_task_unique";
const TASK_RUNS_TASK_CREATED_INDEX_NAME: &str = "idx_task_runs_task_created_at";

#[derive(Default)]
struct StoreData {
    tasks: BTreeMap<String, TaskRecord>,
    model_configs: BTreeMap<String, ModelConfigRecord>,
    remote_servers: BTreeMap<String, RemoteServerRecord>,
    runs: BTreeMap<String, TaskRunRecord>,
    run_events: BTreeMap<String, Vec<TaskRunEventRecord>>,
    ui_prompts: BTreeMap<String, UiPromptRecord>,
    users: BTreeMap<String, UserRecord>,
    cancel_requested_runs: HashSet<String>,
}

#[derive(Clone)]
pub(crate) struct InMemoryStore {
    inner: Arc<RwLock<StoreData>>,
    run_event_sender: broadcast::Sender<TaskRunEventRecord>,
}

#[derive(Clone)]
pub(crate) struct SqliteStore {
    pool: SqlitePool,
    cancel_requested_runs: Arc<RwLock<HashSet<String>>>,
    run_event_sender: broadcast::Sender<TaskRunEventRecord>,
}

#[derive(Clone)]
pub(crate) struct MongoStore {
    tasks: Collection<TaskRecord>,
    model_configs: Collection<ModelConfigRecord>,
    remote_servers: Collection<RemoteServerRecord>,
    runs: Collection<TaskRunRecord>,
    run_events: Collection<TaskRunEventRecord>,
    ui_prompts: Collection<UiPromptRecord>,
    users: Collection<UserRecord>,
    cancel_requested_runs: Arc<RwLock<HashSet<String>>>,
    run_event_sender: broadcast::Sender<TaskRunEventRecord>,
}

#[derive(Clone)]
pub(crate) enum AppStore {
    InMemory(InMemoryStore),
    Sqlite(SqliteStore),
    Mongo(MongoStore),
}

impl AppStore {
    pub async fn new(config: &AppConfig) -> Result<Self, String> {
        let (run_event_sender, _) = broadcast::channel(512);
        match config.store_mode {
            StoreMode::Memory => Ok(Self::InMemory(InMemoryStore::new(run_event_sender))),
            StoreMode::Sqlite => Ok(Self::Sqlite(
                SqliteStore::connect(&config.database_url, run_event_sender).await?,
            )),
            StoreMode::Mongo => Ok(Self::Mongo(
                MongoStore::connect(&config.database_url, run_event_sender).await?,
            )),
        }
    }

    pub async fn list_tasks(&self) -> Result<Vec<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_tasks()),
            Self::Sqlite(store) => store.list_tasks().await,
            Self::Mongo(store) => store.list_tasks().await,
        }
    }

    pub async fn list_tasks_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_tasks_filtered(filters)),
            Self::Sqlite(store) => store.list_tasks_filtered(filters).await,
            Self::Mongo(store) => store.list_tasks_filtered(filters).await,
        }
    }

    pub async fn list_tasks_page(
        &self,
        filters: &TaskListFilters,
    ) -> Result<PaginatedResponse<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_tasks_page(filters)),
            Self::Sqlite(store) => store.list_tasks_page(filters).await,
            Self::Mongo(store) => store.list_tasks_page(filters).await,
        }
    }

    pub async fn get_task(&self, id: &str) -> Result<Option<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_task(id)),
            Self::Sqlite(store) => store.get_task(id).await,
            Self::Mongo(store) => store.get_task(id).await,
        }
    }

    pub async fn list_task_summaries(&self) -> Result<Vec<TaskSummaryRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_summaries()),
            Self::Sqlite(store) => store.list_task_summaries().await,
            Self::Mongo(store) => store.list_task_summaries().await,
        }
    }

    pub async fn list_task_summaries_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_summaries_filtered(filters)),
            Self::Sqlite(store) => store.list_task_summaries_filtered(filters).await,
            Self::Mongo(store) => store.list_task_summaries_filtered(filters).await,
        }
    }

    pub async fn get_task_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_task_summaries_by_ids(ids)),
            Self::Sqlite(store) => store.get_task_summaries_by_ids(ids).await,
            Self::Mongo(store) => store.get_task_summaries_by_ids(ids).await,
        }
    }

    pub async fn list_task_tags(&self) -> Result<Vec<String>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_task_tags()),
            Self::Sqlite(store) => store.list_task_tags().await,
            Self::Mongo(store) => store.list_task_tags().await,
        }
    }

    pub async fn task_stats(&self) -> Result<TaskStatsResponse, String> {
        match self {
            Self::InMemory(store) => Ok(store.task_stats()),
            Self::Sqlite(store) => store.task_stats().await,
            Self::Mongo(store) => store.task_stats().await,
        }
    }

    pub async fn list_due_scheduled_tasks(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<TaskRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_due_scheduled_tasks(now)),
            Self::Sqlite(store) => store.list_due_scheduled_tasks(now).await,
            Self::Mongo(store) => store.list_due_scheduled_tasks(now).await,
        }
    }

    pub async fn save_task(&self, task: TaskRecord) -> Result<TaskRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_task(task)),
            Self::Sqlite(store) => store.save_task(task).await,
            Self::Mongo(store) => store.save_task(task).await,
        }
    }

    pub async fn count_users(&self) -> Result<i64, String> {
        match self {
            Self::InMemory(store) => Ok(store.count_users()),
            Self::Sqlite(store) => store.count_users().await,
            Self::Mongo(store) => store.count_users().await,
        }
    }

    pub async fn list_users(&self) -> Result<Vec<UserRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_users()),
            Self::Sqlite(store) => store.list_users().await,
            Self::Mongo(store) => store.list_users().await,
        }
    }

    pub async fn get_user(&self, id: &str) -> Result<Option<UserRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_user(id)),
            Self::Sqlite(store) => store.get_user(id).await,
            Self::Mongo(store) => store.get_user(id).await,
        }
    }

    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<UserRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_user_by_username(username)),
            Self::Sqlite(store) => store.get_user_by_username(username).await,
            Self::Mongo(store) => store.get_user_by_username(username).await,
        }
    }

    pub async fn save_user(&self, user: UserRecord) -> Result<UserRecord, String> {
        match self {
            Self::InMemory(store) => store.save_user(user),
            Self::Sqlite(store) => store.save_user(user).await,
            Self::Mongo(store) => store.save_user(user).await,
        }
    }

    pub async fn delete_user(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_user(id)),
            Self::Sqlite(store) => store.delete_user(id).await,
            Self::Mongo(store) => store.delete_user(id).await,
        }
    }

    pub async fn delete_task(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_task(id)),
            Self::Sqlite(store) => store.delete_task(id).await,
            Self::Mongo(store) => store.delete_task(id).await,
        }
    }

    pub async fn list_model_configs(&self) -> Result<Vec<ModelConfigRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_model_configs()),
            Self::Sqlite(store) => store.list_model_configs().await,
            Self::Mongo(store) => store.list_model_configs().await,
        }
    }

    pub async fn get_model_config(&self, id: &str) -> Result<Option<ModelConfigRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_model_config(id)),
            Self::Sqlite(store) => store.get_model_config(id).await,
            Self::Mongo(store) => store.get_model_config(id).await,
        }
    }

    pub async fn save_model_config(
        &self,
        model: ModelConfigRecord,
    ) -> Result<ModelConfigRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_model_config(model)),
            Self::Sqlite(store) => store.save_model_config(model).await,
            Self::Mongo(store) => store.save_model_config(model).await,
        }
    }

    pub async fn delete_model_config(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_model_config(id)),
            Self::Sqlite(store) => store.delete_model_config(id).await,
            Self::Mongo(store) => store.delete_model_config(id).await,
        }
    }

    pub async fn list_remote_servers(&self) -> Result<Vec<RemoteServerRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_remote_servers()),
            Self::Sqlite(store) => store.list_remote_servers().await,
            Self::Mongo(store) => store.list_remote_servers().await,
        }
    }

    pub async fn get_remote_server(&self, id: &str) -> Result<Option<RemoteServerRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_remote_server(id)),
            Self::Sqlite(store) => store.get_remote_server(id).await,
            Self::Mongo(store) => store.get_remote_server(id).await,
        }
    }

    pub async fn save_remote_server(
        &self,
        server: RemoteServerRecord,
    ) -> Result<RemoteServerRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_remote_server(server)),
            Self::Sqlite(store) => store.save_remote_server(server).await,
            Self::Mongo(store) => store.save_remote_server(server).await,
        }
    }

    pub async fn delete_remote_server(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete_remote_server(id)),
            Self::Sqlite(store) => store.delete_remote_server(id).await,
            Self::Mongo(store) => store.delete_remote_server(id).await,
        }
    }

    pub async fn list_runs(&self, task_id: Option<&str>) -> Result<Vec<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_runs(task_id)),
            Self::Sqlite(store) => store.list_runs(task_id).await,
            Self::Mongo(store) => store.list_runs(task_id).await,
        }
    }

    pub async fn list_runs_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<Vec<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_runs_filtered(filters)),
            Self::Sqlite(store) => store.list_runs_filtered(filters).await,
            Self::Mongo(store) => store.list_runs_filtered(filters).await,
        }
    }

    pub async fn list_runs_page(
        &self,
        filters: &RunListFilters,
    ) -> Result<PaginatedResponse<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_runs_page(filters)),
            Self::Sqlite(store) => store.list_runs_page(filters).await,
            Self::Mongo(store) => store.list_runs_page(filters).await,
        }
    }

    pub async fn list_run_summaries_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_run_summaries_filtered(filters)),
            Self::Sqlite(store) => store.list_run_summaries_filtered(filters).await,
            Self::Mongo(store) => store.list_run_summaries_filtered(filters).await,
        }
    }

    pub async fn get_run_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<RunSummaryRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_run_summaries_by_ids(ids)),
            Self::Sqlite(store) => store.get_run_summaries_by_ids(ids).await,
            Self::Mongo(store) => store.get_run_summaries_by_ids(ids).await,
        }
    }

    pub async fn list_model_config_usage(&self) -> Result<Vec<ModelConfigUsageRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_model_config_usage()),
            Self::Sqlite(store) => store.list_model_config_usage().await,
            Self::Mongo(store) => store.list_model_config_usage().await,
        }
    }

    pub async fn get_run(&self, id: &str) -> Result<Option<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_run(id)),
            Self::Sqlite(store) => store.get_run(id).await,
            Self::Mongo(store) => store.get_run(id).await,
        }
    }

    pub async fn save_run(&self, run: TaskRunRecord) -> Result<TaskRunRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_run(run)),
            Self::Sqlite(store) => store.save_run(run).await,
            Self::Mongo(store) => store.save_run(run).await,
        }
    }

    pub async fn list_run_events(&self, run_id: &str) -> Result<Vec<TaskRunEventRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_run_events(run_id)),
            Self::Sqlite(store) => store.list_run_events(run_id).await,
            Self::Mongo(store) => store.list_run_events(run_id).await,
        }
    }

    pub async fn append_run_event(&self, event: TaskRunEventRecord) -> Result<(), String> {
        match self {
            Self::InMemory(store) => {
                store.append_run_event(event);
                Ok(())
            }
            Self::Sqlite(store) => store.append_run_event(event).await,
            Self::Mongo(store) => store.append_run_event(event).await,
        }
    }

    pub async fn list_ui_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_ui_prompts(task_id, run_id, status)),
            Self::Sqlite(store) => store.list_ui_prompts(task_id, run_id, status).await,
            Self::Mongo(store) => store.list_ui_prompts(task_id, run_id, status).await,
        }
    }

    pub async fn list_ui_prompts_page(
        &self,
        filters: &PromptListFilters,
    ) -> Result<PaginatedResponse<UiPromptRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_ui_prompts_page(filters)),
            Self::Sqlite(store) => store.list_ui_prompts_page(filters).await,
            Self::Mongo(store) => store.list_ui_prompts_page(filters).await,
        }
    }

    pub async fn get_ui_prompt(&self, id: &str) -> Result<Option<UiPromptRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get_ui_prompt(id)),
            Self::Sqlite(store) => store.get_ui_prompt(id).await,
            Self::Mongo(store) => store.get_ui_prompt(id).await,
        }
    }

    pub async fn save_ui_prompt(&self, prompt: UiPromptRecord) -> Result<UiPromptRecord, String> {
        match self {
            Self::InMemory(store) => Ok(store.save_ui_prompt(prompt)),
            Self::Sqlite(store) => store.save_ui_prompt(prompt).await,
            Self::Mongo(store) => store.save_ui_prompt(prompt).await,
        }
    }

    pub async fn list_ui_prompt_task_counts(
        &self,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptTaskCountRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_ui_prompt_task_counts(status)),
            Self::Sqlite(store) => store.list_ui_prompt_task_counts(status).await,
            Self::Mongo(store) => store.list_ui_prompt_task_counts(status).await,
        }
    }

    pub fn append_run_event_sync(&self, event: TaskRunEventRecord) {
        match self.clone() {
            Self::InMemory(store) => store.append_run_event(event),
            Self::Sqlite(store) => {
                tokio::spawn(async move {
                    if let Err(err) = store.append_run_event(event).await {
                        warn!("failed to append run event: {err}");
                    }
                });
            }
            Self::Mongo(store) => {
                tokio::spawn(async move {
                    if let Err(err) = store.append_run_event(event).await {
                        warn!("failed to append run event: {err}");
                    }
                });
            }
        }
    }

    pub async fn mark_cancel_requested(
        &self,
        run_id: &str,
    ) -> Result<Option<TaskRunRecord>, String> {
        match self {
            Self::InMemory(store) => Ok(store.mark_cancel_requested(run_id)),
            Self::Sqlite(store) => store.mark_cancel_requested(run_id).await,
            Self::Mongo(store) => store.mark_cancel_requested(run_id).await,
        }
    }

    pub fn clear_cancel_requested(&self, run_id: &str) {
        match self.clone() {
            Self::InMemory(store) => store.clear_cancel_requested(run_id),
            Self::Sqlite(store) => store.clear_cancel_requested(run_id),
            Self::Mongo(store) => store.clear_cancel_requested(run_id),
        }
    }

    pub fn is_cancel_requested(&self, run_id: &str) -> bool {
        match self {
            Self::InMemory(store) => store.is_cancel_requested(run_id),
            Self::Sqlite(store) => store.is_cancel_requested(run_id),
            Self::Mongo(store) => store.is_cancel_requested(run_id),
        }
    }

    pub async fn fetch_cancel_requested(&self, run_id: &str) -> Result<bool, String> {
        if self.is_cancel_requested(run_id) {
            return Ok(true);
        }
        Ok(self
            .get_run(run_id)
            .await?
            .is_some_and(|run| run.cancel_requested))
    }

    pub async fn refresh_runtime_guards(&self) -> Result<(), String> {
        match self {
            Self::InMemory(_) => Ok(()),
            Self::Sqlite(store) => store.ensure_active_run_index().await,
            Self::Mongo(store) => store.ensure_task_run_indexes().await,
        }
    }

    pub async fn has_active_run_for_task(&self, task_id: &str) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.has_active_run_for_task(task_id)),
            Self::Sqlite(store) => store.has_active_run_for_task(task_id).await,
            Self::Mongo(store) => store.has_active_run_for_task(task_id).await,
        }
    }

    pub fn subscribe_run_events(&self) -> broadcast::Receiver<TaskRunEventRecord> {
        match self {
            Self::InMemory(store) => store.run_event_sender.subscribe(),
            Self::Sqlite(store) => store.run_event_sender.subscribe(),
            Self::Mongo(store) => store.run_event_sender.subscribe(),
        }
    }
}

impl InMemoryStore {
    pub(crate) fn new(run_event_sender: broadcast::Sender<TaskRunEventRecord>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(StoreData::default())),
            run_event_sender,
        }
    }

    fn list_tasks(&self) -> Vec<TaskRecord> {
        let data = self.inner.read();
        let mut items = data.tasks.values().cloned().collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    fn list_tasks_filtered(&self, filters: &TaskListFilters) -> Vec<TaskRecord> {
        let data = self.inner.read();
        let mut items = data
            .tasks
            .values()
            .filter(|task| filters.status.is_none_or(|value| task.status == value))
            .filter(|task| {
                filters
                    .keyword
                    .as_deref()
                    .is_none_or(|value| task_matches_keyword(task, value))
            })
            .filter(|task| {
                filters
                    .tag
                    .as_deref()
                    .is_none_or(|value| task.tags.iter().any(|item| item == value))
            })
            .filter(|task| {
                filters
                    .model_config_id
                    .as_deref()
                    .is_none_or(|value| task.default_model_config_id.as_deref() == Some(value))
            })
            .filter(|task| {
                !filters.scheduled_only.unwrap_or(false)
                    || !matches!(task.schedule.mode, TaskScheduleMode::Manual)
            })
            .filter(|task| {
                filters
                    .parent_task_id
                    .as_deref()
                    .is_none_or(|value| task.parent_task_id.as_deref() == Some(value))
            })
            .filter(|task| {
                filters
                    .source_run_id
                    .as_deref()
                    .is_none_or(|value| task.source_run_id.as_deref() == Some(value))
            })
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        apply_offset_limit(&mut items, filters.offset, filters.limit);
        items
    }

    fn list_tasks_page(&self, filters: &TaskListFilters) -> PaginatedResponse<TaskRecord> {
        let mut count_filters = filters.clone();
        count_filters.limit = None;
        count_filters.offset = None;
        let total = self.list_tasks_filtered(&count_filters).len();
        build_page_response(
            self.list_tasks_filtered(filters),
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        )
    }

    fn get_task(&self, id: &str) -> Option<TaskRecord> {
        self.inner.read().tasks.get(id).cloned()
    }

    fn list_task_summaries(&self) -> Vec<TaskSummaryRecord> {
        let data = self.inner.read();
        let mut items = data
            .tasks
            .values()
            .map(TaskSummaryRecord::from)
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    fn list_task_summaries_filtered(&self, filters: &TaskListFilters) -> Vec<TaskSummaryRecord> {
        self.list_tasks_filtered(filters)
            .iter()
            .map(TaskSummaryRecord::from)
            .collect()
    }

    fn get_task_summaries_by_ids(&self, ids: &[String]) -> Vec<TaskSummaryRecord> {
        let wanted = ids.iter().collect::<std::collections::HashSet<_>>();
        let data = self.inner.read();
        let mut items = data
            .tasks
            .values()
            .filter(|task| wanted.contains(&task.id))
            .map(TaskSummaryRecord::from)
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    fn list_task_tags(&self) -> Vec<String> {
        let data = self.inner.read();
        let mut tags = data
            .tasks
            .values()
            .flat_map(|task| task.tags.iter().cloned())
            .collect::<Vec<_>>();
        tags.sort();
        tags.dedup();
        tags
    }

    fn task_stats(&self) -> TaskStatsResponse {
        let data = self.inner.read();
        let mut stats = empty_task_stats();

        for task in data.tasks.values() {
            stats.total += 1;
            if !matches!(task.schedule.mode, TaskScheduleMode::Manual) {
                stats.scheduled += 1;
            }
            if task.parent_task_id.is_some() {
                stats.follow_up += 1;
            }
            match task.status {
                TaskStatus::Draft => stats.draft += 1,
                TaskStatus::Ready => stats.ready += 1,
                TaskStatus::Running => stats.running += 1,
                TaskStatus::Succeeded => stats.succeeded += 1,
                TaskStatus::Failed => stats.failed += 1,
                TaskStatus::Blocked => stats.blocked += 1,
                TaskStatus::Cancelled => stats.cancelled += 1,
                TaskStatus::Archived => stats.archived += 1,
            }
        }

        stats
    }

    fn list_due_scheduled_tasks(&self, now: DateTime<Utc>) -> Vec<TaskRecord> {
        let data = self.inner.read();
        let mut items = data
            .tasks
            .values()
            .filter(|task| task_due_for_scheduler(task, &now))
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            task_due_at(left)
                .cmp(&task_due_at(right))
                .then(left.id.cmp(&right.id))
        });
        items
    }

    fn save_task(&self, task: TaskRecord) -> TaskRecord {
        let mut data = self.inner.write();
        data.tasks.insert(task.id.clone(), task.clone());
        task
    }

    fn count_users(&self) -> i64 {
        self.inner.read().users.len() as i64
    }

    fn list_users(&self) -> Vec<UserRecord> {
        let data = self.inner.read();
        let mut items = data.users.values().cloned().collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then(left.username.cmp(&right.username))
        });
        items
    }

    fn get_user(&self, id: &str) -> Option<UserRecord> {
        self.inner.read().users.get(id).cloned()
    }

    fn get_user_by_username(&self, username: &str) -> Option<UserRecord> {
        self.inner
            .read()
            .users
            .values()
            .find(|user| user.username.eq_ignore_ascii_case(username))
            .cloned()
    }

    fn save_user(&self, user: UserRecord) -> Result<UserRecord, String> {
        let mut data = self.inner.write();
        if data
            .users
            .values()
            .any(|existing| existing.id != user.id && existing.username == user.username)
        {
            return Err(format!("用户名已存在: {}", user.username));
        }
        data.users.insert(user.id.clone(), user.clone());
        Ok(user)
    }

    fn delete_user(&self, id: &str) -> bool {
        self.inner.write().users.remove(id).is_some()
    }

    fn delete_task(&self, id: &str) -> bool {
        let mut data = self.inner.write();
        let Some(_) = data.tasks.remove(id) else {
            return false;
        };
        let run_ids = data
            .runs
            .values()
            .filter(|run| run.task_id == id)
            .map(|run| run.id.clone())
            .collect::<Vec<_>>();
        data.runs.retain(|_, run| run.task_id != id);
        for run_id in &run_ids {
            data.run_events.remove(run_id.as_str());
            data.cancel_requested_runs.remove(run_id.as_str());
        }
        data.ui_prompts.retain(|_, prompt| {
            prompt.task_id.as_deref() != Some(id)
                && prompt
                    .run_id
                    .as_deref()
                    .is_none_or(|run_id| !run_ids.iter().any(|candidate| candidate == run_id))
        });
        true
    }

    fn list_model_configs(&self) -> Vec<ModelConfigRecord> {
        let data = self.inner.read();
        let mut items = data.model_configs.values().cloned().collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    fn get_model_config(&self, id: &str) -> Option<ModelConfigRecord> {
        self.inner.read().model_configs.get(id).cloned()
    }

    fn save_model_config(&self, model: ModelConfigRecord) -> ModelConfigRecord {
        let mut data = self.inner.write();
        data.model_configs.insert(model.id.clone(), model.clone());
        model
    }

    fn delete_model_config(&self, id: &str) -> bool {
        let mut data = self.inner.write();
        let deleted = data.model_configs.remove(id).is_some();
        if deleted {
            for task in data.tasks.values_mut() {
                if task.default_model_config_id.as_deref() == Some(id) {
                    task.default_model_config_id = None;
                }
            }
        }
        deleted
    }

    fn list_remote_servers(&self) -> Vec<RemoteServerRecord> {
        let data = self.inner.read();
        let mut items = data.remote_servers.values().cloned().collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    fn get_remote_server(&self, id: &str) -> Option<RemoteServerRecord> {
        self.inner.read().remote_servers.get(id).cloned()
    }

    fn save_remote_server(&self, server: RemoteServerRecord) -> RemoteServerRecord {
        let mut data = self.inner.write();
        data.remote_servers
            .insert(server.id.clone(), server.clone());
        server
    }

    fn delete_remote_server(&self, id: &str) -> bool {
        self.inner.write().remote_servers.remove(id).is_some()
    }

    fn list_runs(&self, task_id: Option<&str>) -> Vec<TaskRunRecord> {
        let data = self.inner.read();
        let mut items = data
            .runs
            .values()
            .filter(|run| task_id.is_none_or(|value| run.task_id == value))
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        items
    }

    fn list_runs_filtered(&self, filters: &RunListFilters) -> Vec<TaskRunRecord> {
        let data = self.inner.read();
        let mut items = data
            .runs
            .values()
            .filter(|run| {
                filters
                    .task_id
                    .as_deref()
                    .is_none_or(|value| run.task_id == value)
            })
            .filter(|run| filters.status.is_none_or(|value| run.status == value))
            .filter(|run| {
                filters
                    .model_config_id
                    .as_deref()
                    .is_none_or(|value| run.model_config_id == value)
            })
            .filter(|run| {
                filters.keyword.as_deref().is_none_or(|value| {
                    run.id.to_ascii_lowercase().contains(value)
                        || run.task_id.to_ascii_lowercase().contains(value)
                        || run.model_config_id.to_ascii_lowercase().contains(value)
                        || run
                            .result_summary
                            .as_deref()
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                            .contains(value)
                        || run
                            .error_message
                            .as_deref()
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                            .contains(value)
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        apply_offset_limit(&mut items, filters.offset, filters.limit);
        items
    }

    fn list_runs_page(&self, filters: &RunListFilters) -> PaginatedResponse<TaskRunRecord> {
        let mut count_filters = filters.clone();
        count_filters.limit = None;
        count_filters.offset = None;
        let total = self.list_runs_filtered(&count_filters).len();
        build_page_response(
            self.list_runs_filtered(filters),
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        )
    }

    fn list_run_summaries_filtered(&self, filters: &RunListFilters) -> Vec<RunSummaryRecord> {
        self.list_runs_filtered(filters)
            .iter()
            .map(RunSummaryRecord::from)
            .collect()
    }

    fn get_run_summaries_by_ids(&self, ids: &[String]) -> Vec<RunSummaryRecord> {
        let wanted = ids.iter().collect::<std::collections::HashSet<_>>();
        let data = self.inner.read();
        let mut items = data
            .runs
            .values()
            .filter(|run| wanted.contains(&run.id))
            .map(RunSummaryRecord::from)
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    fn list_model_config_usage(&self) -> Vec<ModelConfigUsageRecord> {
        let data = self.inner.read();
        let mut usage = BTreeMap::<String, ModelConfigUsageRecord>::new();

        for task in data.tasks.values() {
            let Some(model_config_id) = task.default_model_config_id.clone() else {
                continue;
            };
            let entry = usage
                .entry(model_config_id.clone())
                .or_insert(ModelConfigUsageRecord {
                    model_config_id,
                    task_count: 0,
                    run_count: 0,
                });
            entry.task_count += 1;
        }

        for run in data.runs.values() {
            let entry =
                usage
                    .entry(run.model_config_id.clone())
                    .or_insert(ModelConfigUsageRecord {
                        model_config_id: run.model_config_id.clone(),
                        task_count: 0,
                        run_count: 0,
                    });
            entry.run_count += 1;
        }

        usage.into_values().collect()
    }

    fn get_run(&self, id: &str) -> Option<TaskRunRecord> {
        self.inner.read().runs.get(id).cloned()
    }

    fn save_run(&self, run: TaskRunRecord) -> TaskRunRecord {
        let mut data = self.inner.write();
        data.runs.insert(run.id.clone(), run.clone());
        run
    }

    fn list_run_events(&self, run_id: &str) -> Vec<TaskRunEventRecord> {
        self.inner
            .read()
            .run_events
            .get(run_id)
            .cloned()
            .unwrap_or_default()
    }

    fn append_run_event(&self, event: TaskRunEventRecord) {
        let mut data = self.inner.write();
        data.run_events
            .entry(event.run_id.clone())
            .or_default()
            .push(event.clone());
        let _ = self.run_event_sender.send(event);
    }

    fn list_ui_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<UiPromptStatus>,
    ) -> Vec<UiPromptRecord> {
        let data = self.inner.read();
        let mut items = data
            .ui_prompts
            .values()
            .filter(|prompt| task_id.is_none_or(|value| prompt.task_id.as_deref() == Some(value)))
            .filter(|prompt| run_id.is_none_or(|value| prompt.run_id.as_deref() == Some(value)))
            .filter(|prompt| status.is_none_or(|value| prompt.status == value))
            .cloned()
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        items
    }

    fn list_ui_prompts_page(
        &self,
        filters: &PromptListFilters,
    ) -> PaginatedResponse<UiPromptRecord> {
        let items = self.list_ui_prompts(
            filters.task_id.as_deref(),
            filters.run_id.as_deref(),
            filters.status,
        );
        let total = items.len();
        build_page_response(
            slice_page_items(
                items,
                filters.offset.unwrap_or(0),
                filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            ),
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        )
    }

    fn get_ui_prompt(&self, id: &str) -> Option<UiPromptRecord> {
        self.inner.read().ui_prompts.get(id).cloned()
    }

    fn save_ui_prompt(&self, prompt: UiPromptRecord) -> UiPromptRecord {
        let mut data = self.inner.write();
        data.ui_prompts.insert(prompt.id.clone(), prompt.clone());
        prompt
    }

    fn list_ui_prompt_task_counts(
        &self,
        status: Option<UiPromptStatus>,
    ) -> Vec<UiPromptTaskCountRecord> {
        let data = self.inner.read();
        let mut counts = BTreeMap::<String, usize>::new();

        for prompt in data.ui_prompts.values() {
            if status.is_some_and(|value| prompt.status != value) {
                continue;
            }
            let Some(task_id) = prompt.task_id.as_deref() else {
                continue;
            };
            *counts.entry(task_id.to_string()).or_default() += 1;
        }

        let mut items = counts
            .into_iter()
            .map(|(task_id, count)| UiPromptTaskCountRecord { task_id, count })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .count
                .cmp(&left.count)
                .then(left.task_id.cmp(&right.task_id))
        });
        items
    }

    fn mark_cancel_requested(&self, run_id: &str) -> Option<TaskRunRecord> {
        let mut data = self.inner.write();
        data.cancel_requested_runs.insert(run_id.to_string());
        let run = data.runs.get_mut(run_id)?;
        run.cancel_requested = true;
        Some(run.clone())
    }

    fn clear_cancel_requested(&self, run_id: &str) {
        let mut data = self.inner.write();
        data.cancel_requested_runs.remove(run_id);
        if let Some(run) = data.runs.get_mut(run_id) {
            run.cancel_requested = false;
        }
    }

    fn is_cancel_requested(&self, run_id: &str) -> bool {
        self.inner.read().cancel_requested_runs.contains(run_id)
    }

    fn has_active_run_for_task(&self, task_id: &str) -> bool {
        self.inner.read().runs.values().any(|run| {
            run.task_id == task_id
                && matches!(run.status, TaskRunStatus::Queued | TaskRunStatus::Running)
        })
    }
}

impl MongoStore {
    async fn connect(
        database_url: &str,
        run_event_sender: broadcast::Sender<TaskRunEventRecord>,
    ) -> Result<Self, String> {
        let client = Client::with_uri_str(database_url)
            .await
            .map_err(|err| err.to_string())?;
        let database = client
            .default_database()
            .ok_or_else(|| "mongodb connection string must include a database name".to_string())?;
        let store = Self {
            tasks: database.collection::<TaskRecord>("tasks"),
            model_configs: database.collection::<ModelConfigRecord>("model_configs"),
            remote_servers: database.collection::<RemoteServerRecord>("remote_servers"),
            runs: database.collection::<TaskRunRecord>("task_runs"),
            run_events: database.collection::<TaskRunEventRecord>("task_run_events"),
            ui_prompts: database.collection::<UiPromptRecord>("ui_prompts"),
            users: database.collection::<UserRecord>("users"),
            cancel_requested_runs: Arc::new(RwLock::new(HashSet::new())),
            run_event_sender,
        };
        store.ensure_indexes().await?;
        store.reload_cancel_requested_runs().await?;
        Ok(store)
    }

    async fn ensure_indexes(&self) -> Result<(), String> {
        self.ensure_index(&self.tasks, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.tasks, doc! { "status": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "default_model_config_id": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "updated_at": -1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "tags": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "parent_task_id": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "source_run_id": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "creator_user_id": 1 }, false)
            .await?;
        self.ensure_index(&self.tasks, doc! { "schedule.next_run_at": 1 }, false)
            .await?;
        self.ensure_index(
            &self.tasks,
            doc! { "schedule.mode": 1, "schedule.next_run_at": 1 },
            false,
        )
        .await?;

        self.ensure_index(&self.model_configs, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.model_configs, doc! { "updated_at": -1 }, false)
            .await?;

        self.ensure_index(&self.remote_servers, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.remote_servers, doc! { "enabled": 1 }, false)
            .await?;
        self.ensure_index(&self.remote_servers, doc! { "updated_at": -1 }, false)
            .await?;

        self.ensure_index(&self.runs, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.runs, doc! { "model_config_id": 1 }, false)
            .await?;
        self.ensure_index(
            &self.runs,
            doc! { "model_config_id": 1, "created_at": -1 },
            false,
        )
        .await?;
        self.ensure_index(&self.runs, doc! { "status": 1 }, false)
            .await?;
        self.ensure_index(&self.runs, doc! { "created_at": -1 }, false)
            .await?;
        self.ensure_index(&self.runs, doc! { "updated_at": -1 }, false)
            .await?;
        self.ensure_index(&self.runs, doc! { "cancel_requested": 1 }, false)
            .await?;
        self.ensure_task_run_indexes().await?;

        self.ensure_index(&self.run_events, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.run_events, doc! { "run_id": 1 }, false)
            .await?;
        self.ensure_index(&self.run_events, doc! { "created_at": 1 }, false)
            .await?;
        self.ensure_index(
            &self.run_events,
            doc! { "run_id": 1, "created_at": 1, "id": 1 },
            false,
        )
        .await?;

        self.ensure_index(&self.ui_prompts, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.ui_prompts, doc! { "task_id": 1 }, false)
            .await?;
        self.ensure_index(&self.ui_prompts, doc! { "run_id": 1 }, false)
            .await?;
        self.ensure_index(&self.ui_prompts, doc! { "status": 1 }, false)
            .await?;
        self.ensure_index(&self.ui_prompts, doc! { "status": 1, "task_id": 1 }, false)
            .await?;
        self.ensure_index(
            &self.ui_prompts,
            doc! { "task_id": 1, "updated_at": -1 },
            false,
        )
        .await?;
        self.ensure_index(
            &self.ui_prompts,
            doc! { "run_id": 1, "updated_at": -1 },
            false,
        )
        .await?;
        self.ensure_index(&self.ui_prompts, doc! { "updated_at": -1 }, false)
            .await?;

        self.ensure_index(&self.users, doc! { "id": 1 }, true)
            .await?;
        self.ensure_index(&self.users, doc! { "username": 1 }, true)
            .await?;
        self.ensure_index(&self.users, doc! { "enabled": 1 }, false)
            .await?;

        Ok(())
    }

    async fn ensure_task_run_indexes(&self) -> Result<(), String> {
        let _ = self.runs.drop_index("task_id_1", None).await;

        self.runs
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "task_id": 1, "created_at": -1 })
                    .options(
                        IndexOptions::builder()
                            .name(Some(TASK_RUNS_TASK_CREATED_INDEX_NAME.to_string()))
                            .build(),
                    )
                    .build(),
                None,
            )
            .await
            .map_err(|err| err.to_string())?;

        let create_unique_index = self
            .runs
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "task_id": 1 })
                    .options(
                        IndexOptions::builder()
                            .name(Some(ACTIVE_TASK_RUN_UNIQUE_INDEX_NAME.to_string()))
                            .unique(true)
                            .partial_filter_expression(doc! {
                                "status": {
                                    "$in": ["queued", "running"]
                                }
                            })
                            .build(),
                    )
                    .build(),
                None,
            )
            .await;

        if let Err(err) = create_unique_index {
            if is_mongo_active_run_index_conflict(&err.to_string()) {
                warn!(
                    "skipping active task run unique index creation due to existing duplicate active runs: {}",
                    err
                );
            } else {
                return Err(err.to_string());
            }
        }

        Ok(())
    }

    async fn ensure_index<T>(
        &self,
        collection: &Collection<T>,
        keys: Document,
        unique: bool,
    ) -> Result<(), String>
    where
        T: Send + Sync,
    {
        collection
            .create_index(
                IndexModel::builder()
                    .keys(keys)
                    .options(unique.then(|| IndexOptions::builder().unique(true).build()))
                    .build(),
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn reload_cancel_requested_runs(&self) -> Result<(), String> {
        let ids = self
            .runs
            .distinct("id", Some(doc! { "cancel_requested": true }), None)
            .await
            .map_err(|err| err.to_string())?;
        let mut cancel_requested_runs = self.cancel_requested_runs.write();
        cancel_requested_runs.clear();
        for value in ids {
            if let Bson::String(id) = value {
                cancel_requested_runs.insert(id);
            }
        }
        Ok(())
    }

    async fn load_collection_items_with_query<T>(
        &self,
        collection: &Collection<T>,
        filter: Document,
        options: Option<FindOptions>,
    ) -> Result<Vec<T>, String>
    where
        T: DeserializeOwned + Unpin + Send + Sync,
    {
        let mut cursor = collection
            .find(filter, options)
            .await
            .map_err(|err| err.to_string())?;
        let mut items = Vec::new();
        while let Some(item) = cursor.try_next().await.map_err(|err| err.to_string())? {
            items.push(item);
        }
        Ok(items)
    }

    async fn aggregate_documents<T>(
        &self,
        collection: &Collection<T>,
        pipeline: Vec<Document>,
    ) -> Result<Vec<Document>, String>
    where
        T: Send + Sync,
    {
        let mut cursor = collection
            .aggregate(pipeline, None)
            .await
            .map_err(|err| err.to_string())?;
        let mut items = Vec::new();
        while let Some(item) = cursor.try_next().await.map_err(|err| err.to_string())? {
            items.push(item);
        }
        Ok(items)
    }

    async fn aggregate_collection_items<T>(
        &self,
        collection: &Collection<T>,
        pipeline: Vec<Document>,
    ) -> Result<Vec<T>, String>
    where
        T: DeserializeOwned + Send + Sync,
    {
        self.aggregate_documents(collection, pipeline)
            .await?
            .into_iter()
            .map(|doc| bson::from_document(doc).map_err(|err| err.to_string()))
            .collect()
    }

    async fn aggregate_into_items<S, T>(
        &self,
        collection: &Collection<S>,
        pipeline: Vec<Document>,
    ) -> Result<Vec<T>, String>
    where
        S: Send + Sync,
        T: DeserializeOwned,
    {
        self.aggregate_documents(collection, pipeline)
            .await?
            .into_iter()
            .map(|doc| bson::from_document(doc).map_err(|err| err.to_string()))
            .collect()
    }

    async fn find_by_id<T>(&self, collection: &Collection<T>, id: &str) -> Result<Option<T>, String>
    where
        T: DeserializeOwned + Unpin + Send + Sync,
    {
        collection
            .find_one(doc! { "id": id }, None)
            .await
            .map_err(|err| err.to_string())
    }

    async fn upsert_by_id<T>(
        &self,
        collection: &Collection<T>,
        id: &str,
        value: &T,
    ) -> Result<(), String>
    where
        T: Serialize + Send + Sync,
    {
        collection
            .replace_one(
                doc! { "id": id },
                value,
                ReplaceOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn delete_by_id<T>(&self, collection: &Collection<T>, id: &str) -> Result<bool, String>
    where
        T: Send + Sync,
    {
        collection
            .delete_one(doc! { "id": id }, None)
            .await
            .map(|result| result.deleted_count > 0)
            .map_err(|err| err.to_string())
    }

    async fn list_tasks(&self) -> Result<Vec<TaskRecord>, String> {
        self.load_collection_items_with_query(
            &self.tasks,
            doc! {},
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    async fn list_tasks_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskRecord>, String> {
        let filter = build_mongo_task_filter(filters);
        self.load_collection_items_with_query(
            &self.tasks,
            filter,
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                filters.offset,
                filters.limit,
            )),
        )
        .await
    }

    async fn list_tasks_page(
        &self,
        filters: &TaskListFilters,
    ) -> Result<PaginatedResponse<TaskRecord>, String> {
        let filter = build_mongo_task_filter(filters);
        let total = self
            .tasks
            .count_documents(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())? as usize;
        let items = self
            .load_collection_items_with_query(
                &self.tasks,
                filter,
                Some(mongo_find_options(
                    doc! { "updated_at": -1, "id": -1 },
                    filters.offset,
                    filters.limit,
                )),
            )
            .await?;
        Ok(build_page_response(
            items,
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        ))
    }

    async fn get_task(&self, id: &str) -> Result<Option<TaskRecord>, String> {
        self.find_by_id(&self.tasks, id).await
    }

    async fn list_task_summaries(&self) -> Result<Vec<TaskSummaryRecord>, String> {
        self.aggregate_into_items(
            &self.tasks,
            vec![
                doc! {
                    "$project": {
                        "_id": 0,
                        "id": 1,
                        "title": 1,
                        "status": 1,
                        "default_model_config_id": 1,
                        "creator_user_id": 1,
                        "creator_username": 1,
                        "creator_display_name": 1,
                        "last_run_id": 1,
                        "updated_at": 1,
                    }
                },
                doc! { "$sort": { "updated_at": -1, "id": -1 } },
            ],
        )
        .await
    }

    async fn list_task_summaries_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        let filter = build_mongo_task_filter(filters);
        self.aggregate_into_items(
            &self.tasks,
            vec![
                doc! { "$match": filter },
                doc! {
                    "$project": {
                        "_id": 0,
                        "id": 1,
                        "title": 1,
                        "status": 1,
                        "default_model_config_id": 1,
                        "creator_user_id": 1,
                        "creator_username": 1,
                        "creator_display_name": 1,
                        "last_run_id": 1,
                        "updated_at": 1,
                    }
                },
                doc! { "$sort": { "updated_at": -1, "id": -1 } },
                build_skip_stage(filters.offset),
                build_limit_stage(filters.limit),
            ]
            .into_iter()
            .filter(|stage| !stage.is_empty())
            .collect(),
        )
        .await
    }

    async fn get_task_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.aggregate_into_items(
            &self.tasks,
            vec![
                doc! { "$match": { "id": { "$in": ids.to_vec() } } },
                doc! {
                    "$project": {
                        "_id": 0,
                        "id": 1,
                        "title": 1,
                        "status": 1,
                        "default_model_config_id": 1,
                        "creator_user_id": 1,
                        "creator_username": 1,
                        "creator_display_name": 1,
                        "last_run_id": 1,
                        "updated_at": 1,
                    }
                },
                doc! { "$sort": { "updated_at": -1, "id": -1 } },
            ],
        )
        .await
    }

    async fn list_task_tags(&self) -> Result<Vec<String>, String> {
        let mut tags = self
            .tasks
            .distinct("tags", None, None)
            .await
            .map_err(|err| err.to_string())?
            .into_iter()
            .filter_map(|value| match value {
                Bson::String(tag) => Some(tag),
                _ => None,
            })
            .collect::<Vec<_>>();
        tags.sort();
        Ok(tags)
    }

    async fn task_stats(&self) -> Result<TaskStatsResponse, String> {
        let rows = self
            .aggregate_documents(
                &self.tasks,
                vec![doc! {
                    "$group": {
                        "_id": Bson::Null,
                        "total": { "$sum": 1_i32 },
                        "scheduled": {
                            "$sum": {
                                "$cond": [
                                    { "$ne": ["$schedule.mode", "manual"] },
                                    1_i32,
                                    0_i32
                                ]
                            }
                        },
                        "follow_up": {
                            "$sum": {
                                "$cond": [
                                    { "$ne": [{ "$ifNull": ["$parent_task_id", Bson::Null] }, Bson::Null] },
                                    1_i32,
                                    0_i32
                                ]
                            }
                        },
                        "draft": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "draft"] }, 1_i32, 0_i32]
                            }
                        },
                        "ready": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "ready"] }, 1_i32, 0_i32]
                            }
                        },
                        "running": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "running"] }, 1_i32, 0_i32]
                            }
                        },
                        "succeeded": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "succeeded"] }, 1_i32, 0_i32]
                            }
                        },
                        "failed": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "failed"] }, 1_i32, 0_i32]
                            }
                        },
                        "blocked": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "blocked"] }, 1_i32, 0_i32]
                            }
                        },
                        "cancelled": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "cancelled"] }, 1_i32, 0_i32]
                            }
                        },
                        "archived": {
                            "$sum": {
                                "$cond": [{ "$eq": ["$status", "archived"] }, 1_i32, 0_i32]
                            }
                        }
                    }
                }],
            )
            .await?;

        let Some(row) = rows.first() else {
            return Ok(empty_task_stats());
        };

        Ok(TaskStatsResponse {
            total: bson_usize_field(row, "total").unwrap_or(0),
            scheduled: bson_usize_field(row, "scheduled").unwrap_or(0),
            follow_up: bson_usize_field(row, "follow_up").unwrap_or(0),
            draft: bson_usize_field(row, "draft").unwrap_or(0),
            ready: bson_usize_field(row, "ready").unwrap_or(0),
            running: bson_usize_field(row, "running").unwrap_or(0),
            succeeded: bson_usize_field(row, "succeeded").unwrap_or(0),
            failed: bson_usize_field(row, "failed").unwrap_or(0),
            blocked: bson_usize_field(row, "blocked").unwrap_or(0),
            cancelled: bson_usize_field(row, "cancelled").unwrap_or(0),
            archived: bson_usize_field(row, "archived").unwrap_or(0),
        })
    }

    async fn list_due_scheduled_tasks(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<TaskRecord>, String> {
        self.aggregate_collection_items(
            &self.tasks,
            vec![
                doc! {
                    "$match": {
                        "status": { "$nin": ["archived", "running"] },
                        "schedule.mode": { "$ne": "manual" },
                        "schedule.next_run_at": { "$exists": true, "$ne": Bson::Null },
                    }
                },
                doc! {
                    "$addFields": {
                        "_due_at": {
                            "$dateFromString": {
                                "dateString": "$schedule.next_run_at",
                                "onError": Bson::Null,
                                "onNull": Bson::Null,
                            }
                        }
                    }
                },
                doc! {
                    "$match": {
                        "$expr": {
                            "$and": [
                                { "$ne": ["$_due_at", Bson::Null] },
                                {
                                    "$lte": [
                                        "$_due_at",
                                        Bson::DateTime(mongodb::bson::DateTime::from_millis(now.timestamp_millis()))
                                    ]
                                }
                            ]
                        }
                    }
                },
                doc! { "$sort": { "_due_at": 1, "id": 1 } },
                doc! { "$project": { "_due_at": 0 } },
            ],
        )
        .await
    }

    async fn save_task(&self, task: TaskRecord) -> Result<TaskRecord, String> {
        self.upsert_by_id(&self.tasks, &task.id, &task).await?;
        Ok(task)
    }

    async fn count_users(&self) -> Result<i64, String> {
        self.users
            .count_documents(doc! {}, None)
            .await
            .map(|count| count as i64)
            .map_err(|err| err.to_string())
    }

    async fn list_users(&self) -> Result<Vec<UserRecord>, String> {
        self.load_collection_items_with_query(
            &self.users,
            doc! {},
            Some(mongo_find_options(
                doc! { "updated_at": -1, "username": 1 },
                None,
                None,
            )),
        )
        .await
    }

    async fn get_user(&self, id: &str) -> Result<Option<UserRecord>, String> {
        self.find_by_id(&self.users, id).await
    }

    async fn get_user_by_username(&self, username: &str) -> Result<Option<UserRecord>, String> {
        self.users
            .find_one(doc! { "username": username }, None)
            .await
            .map_err(|err| err.to_string())
    }

    async fn save_user(&self, user: UserRecord) -> Result<UserRecord, String> {
        self.upsert_by_id(&self.users, &user.id, &user).await?;
        Ok(user)
    }

    async fn delete_user(&self, id: &str) -> Result<bool, String> {
        self.delete_by_id(&self.users, id).await
    }

    async fn delete_task(&self, id: &str) -> Result<bool, String> {
        if self.find_by_id(&self.tasks, id).await?.is_none() {
            return Ok(false);
        }
        let run_ids = self
            .list_runs(Some(id))
            .await?
            .into_iter()
            .map(|run| run.id)
            .collect::<Vec<_>>();

        self.ui_prompts
            .delete_many(
                doc! {
                    "$or": [
                        doc! { "task_id": id },
                        doc! { "run_id": { "$in": run_ids.clone() } },
                    ]
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        self.run_events
            .delete_many(doc! { "run_id": { "$in": run_ids.clone() } }, None)
            .await
            .map_err(|err| err.to_string())?;
        self.runs
            .delete_many(doc! { "task_id": id }, None)
            .await
            .map_err(|err| err.to_string())?;

        {
            let mut cancel_requested_runs = self.cancel_requested_runs.write();
            for run_id in run_ids {
                cancel_requested_runs.remove(&run_id);
            }
        }

        self.delete_by_id(&self.tasks, id).await
    }

    async fn list_model_configs(&self) -> Result<Vec<ModelConfigRecord>, String> {
        self.load_collection_items_with_query(
            &self.model_configs,
            doc! {},
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    async fn get_model_config(&self, id: &str) -> Result<Option<ModelConfigRecord>, String> {
        self.find_by_id(&self.model_configs, id).await
    }

    async fn save_model_config(
        &self,
        model: ModelConfigRecord,
    ) -> Result<ModelConfigRecord, String> {
        self.upsert_by_id(&self.model_configs, &model.id, &model)
            .await?;
        Ok(model)
    }

    async fn delete_model_config(&self, id: &str) -> Result<bool, String> {
        let deleted = self.delete_by_id(&self.model_configs, id).await?;
        if !deleted {
            return Ok(false);
        }
        self.tasks
            .update_many(
                doc! { "default_model_config_id": id },
                doc! { "$set": { "default_model_config_id": Bson::Null } },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(true)
    }

    async fn list_remote_servers(&self) -> Result<Vec<RemoteServerRecord>, String> {
        self.load_collection_items_with_query(
            &self.remote_servers,
            doc! {},
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    async fn get_remote_server(&self, id: &str) -> Result<Option<RemoteServerRecord>, String> {
        self.find_by_id(&self.remote_servers, id).await
    }

    async fn save_remote_server(
        &self,
        server: RemoteServerRecord,
    ) -> Result<RemoteServerRecord, String> {
        self.upsert_by_id(&self.remote_servers, &server.id, &server)
            .await?;
        Ok(server)
    }

    async fn delete_remote_server(&self, id: &str) -> Result<bool, String> {
        self.delete_by_id(&self.remote_servers, id).await
    }

    async fn list_runs(&self, task_id: Option<&str>) -> Result<Vec<TaskRunRecord>, String> {
        let filter = task_id.map_or_else(|| doc! {}, |value| doc! { "task_id": value });
        self.load_collection_items_with_query(
            &self.runs,
            filter,
            Some(mongo_find_options(
                doc! { "created_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    async fn list_runs_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let filter = build_mongo_run_filter(filters);
        self.load_collection_items_with_query(
            &self.runs,
            filter,
            Some(mongo_find_options(
                doc! { "created_at": -1, "id": -1 },
                filters.offset,
                filters.limit,
            )),
        )
        .await
    }

    async fn list_runs_page(
        &self,
        filters: &RunListFilters,
    ) -> Result<PaginatedResponse<TaskRunRecord>, String> {
        let filter = build_mongo_run_filter(filters);
        let total = self
            .runs
            .count_documents(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())? as usize;
        let items = self
            .load_collection_items_with_query(
                &self.runs,
                filter,
                Some(mongo_find_options(
                    doc! { "created_at": -1, "id": -1 },
                    filters.offset,
                    filters.limit,
                )),
            )
            .await?;
        Ok(build_page_response(
            items,
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        ))
    }

    async fn list_run_summaries_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        let filter = build_mongo_run_filter(filters);
        self.aggregate_into_items(
            &self.runs,
            vec![
                doc! { "$match": filter },
                doc! {
                    "$project": {
                        "_id": 0,
                        "id": 1,
                        "task_id": 1,
                        "status": 1,
                        "model_config_id": 1,
                        "updated_at": 1,
                    }
                },
                doc! { "$sort": { "updated_at": -1, "id": -1 } },
                build_skip_stage(filters.offset),
                build_limit_stage(filters.limit),
            ]
            .into_iter()
            .filter(|stage| !stage.is_empty())
            .collect(),
        )
        .await
    }

    async fn get_run_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<RunSummaryRecord>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.aggregate_into_items(
            &self.runs,
            vec![
                doc! { "$match": { "id": { "$in": ids.to_vec() } } },
                doc! {
                    "$project": {
                        "_id": 0,
                        "id": 1,
                        "task_id": 1,
                        "status": 1,
                        "model_config_id": 1,
                        "updated_at": 1,
                    }
                },
                doc! { "$sort": { "updated_at": -1, "id": -1 } },
            ],
        )
        .await
    }

    async fn list_model_config_usage(&self) -> Result<Vec<ModelConfigUsageRecord>, String> {
        let task_counts = self
            .aggregate_documents(
                &self.tasks,
                vec![
                    doc! {
                        "$match": {
                            "default_model_config_id": {
                                "$exists": true,
                                "$ne": Bson::Null,
                            }
                        }
                    },
                    doc! {
                        "$group": {
                            "_id": "$default_model_config_id",
                            "task_count": { "$sum": 1_i32 },
                        }
                    },
                ],
            )
            .await?;
        let run_counts = self
            .aggregate_documents(
                &self.runs,
                vec![doc! {
                    "$group": {
                        "_id": "$model_config_id",
                        "run_count": { "$sum": 1_i32 },
                    }
                }],
            )
            .await?;

        let mut usage = BTreeMap::<String, ModelConfigUsageRecord>::new();
        for row in task_counts {
            let Some(model_config_id) = bson_string_field(&row, "_id") else {
                continue;
            };
            let entry = usage
                .entry(model_config_id.clone())
                .or_insert(ModelConfigUsageRecord {
                    model_config_id,
                    task_count: 0,
                    run_count: 0,
                });
            entry.task_count = bson_usize_field(&row, "task_count").unwrap_or(0);
        }
        for row in run_counts {
            let Some(model_config_id) = bson_string_field(&row, "_id") else {
                continue;
            };
            let entry = usage
                .entry(model_config_id.clone())
                .or_insert(ModelConfigUsageRecord {
                    model_config_id,
                    task_count: 0,
                    run_count: 0,
                });
            entry.run_count = bson_usize_field(&row, "run_count").unwrap_or(0);
        }

        Ok(usage.into_values().collect())
    }

    async fn get_run(&self, id: &str) -> Result<Option<TaskRunRecord>, String> {
        self.find_by_id(&self.runs, id).await
    }

    async fn save_run(&self, run: TaskRunRecord) -> Result<TaskRunRecord, String> {
        self.runs
            .replace_one(
                doc! { "id": &run.id },
                &run,
                ReplaceOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|err| {
                if is_mongo_active_run_conflict(&err.to_string()) {
                    "当前任务已有正在执行的运行".to_string()
                } else {
                    err.to_string()
                }
            })?;
        let mut cancel_requested_runs = self.cancel_requested_runs.write();
        if run.cancel_requested {
            cancel_requested_runs.insert(run.id.clone());
        } else {
            cancel_requested_runs.remove(&run.id);
        }
        Ok(run)
    }

    async fn list_run_events(&self, run_id: &str) -> Result<Vec<TaskRunEventRecord>, String> {
        self.load_collection_items_with_query(
            &self.run_events,
            doc! { "run_id": run_id },
            Some(mongo_find_options(
                doc! { "created_at": 1, "id": 1 },
                None,
                None,
            )),
        )
        .await
    }

    async fn append_run_event(&self, event: TaskRunEventRecord) -> Result<(), String> {
        self.upsert_by_id(&self.run_events, &event.id, &event)
            .await?;
        let _ = self.run_event_sender.send(event);
        Ok(())
    }

    async fn list_ui_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptRecord>, String> {
        let filter = build_mongo_prompt_filter(task_id, run_id, status);
        self.load_collection_items_with_query(
            &self.ui_prompts,
            filter,
            Some(mongo_find_options(
                doc! { "updated_at": -1, "id": -1 },
                None,
                None,
            )),
        )
        .await
    }

    async fn list_ui_prompts_page(
        &self,
        filters: &PromptListFilters,
    ) -> Result<PaginatedResponse<UiPromptRecord>, String> {
        let filter = build_mongo_prompt_filter(
            filters.task_id.as_deref(),
            filters.run_id.as_deref(),
            filters.status,
        );
        let total = self
            .ui_prompts
            .count_documents(filter.clone(), None)
            .await
            .map_err(|err| err.to_string())? as usize;
        let items = self
            .load_collection_items_with_query(
                &self.ui_prompts,
                filter,
                Some(mongo_find_options(
                    doc! { "updated_at": -1, "id": -1 },
                    filters.offset,
                    filters.limit,
                )),
            )
            .await?;
        Ok(build_page_response(
            items,
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        ))
    }

    async fn get_ui_prompt(&self, id: &str) -> Result<Option<UiPromptRecord>, String> {
        self.find_by_id(&self.ui_prompts, id).await
    }

    async fn save_ui_prompt(&self, prompt: UiPromptRecord) -> Result<UiPromptRecord, String> {
        self.upsert_by_id(&self.ui_prompts, &prompt.id, &prompt)
            .await?;
        Ok(prompt)
    }

    async fn list_ui_prompt_task_counts(
        &self,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptTaskCountRecord>, String> {
        let mut match_filter = doc! {
            "task_id": {
                "$exists": true,
                "$ne": Bson::Null,
            }
        };
        if let Some(status) = status {
            match_filter.insert("status", ui_prompt_status_to_str(status));
        }
        let rows = self
            .aggregate_documents(
                &self.ui_prompts,
                vec![
                    doc! { "$match": match_filter },
                    doc! {
                        "$group": {
                            "_id": "$task_id",
                            "prompt_count": { "$sum": 1_i32 },
                        }
                    },
                    doc! { "$sort": { "prompt_count": -1, "_id": 1 } },
                ],
            )
            .await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(UiPromptTaskCountRecord {
                    task_id: bson_string_field(&row, "_id")?,
                    count: bson_usize_field(&row, "prompt_count")?,
                })
            })
            .collect())
    }

    async fn mark_cancel_requested(&self, run_id: &str) -> Result<Option<TaskRunRecord>, String> {
        let result = self
            .runs
            .update_one(
                doc! { "id": run_id },
                doc! {
                    "$set": {
                        "cancel_requested": true,
                        "updated_at": Utc::now().to_rfc3339(),
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        if result.matched_count == 0 {
            return Ok(None);
        }
        self.cancel_requested_runs
            .write()
            .insert(run_id.to_string());
        self.get_run(run_id).await
    }

    fn clear_cancel_requested(&self, run_id: &str) {
        self.cancel_requested_runs.write().remove(run_id);
        let runs = self.runs.clone();
        let run_id = run_id.to_string();
        tokio::spawn(async move {
            if let Err(err) = runs
                .update_one(
                    doc! { "id": &run_id },
                    doc! {
                        "$set": {
                            "cancel_requested": false,
                            "updated_at": Utc::now().to_rfc3339(),
                        }
                    },
                    None,
                )
                .await
            {
                warn!("failed to clear cancel_requested flag: {err}");
            }
        });
    }

    fn is_cancel_requested(&self, run_id: &str) -> bool {
        self.cancel_requested_runs.read().contains(run_id)
    }

    async fn has_active_run_for_task(&self, task_id: &str) -> Result<bool, String> {
        let count = self
            .runs
            .count_documents(
                doc! {
                    "task_id": task_id,
                    "status": {
                        "$in": ["queued", "running"]
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(count > 0)
    }
}

impl SqliteStore {
    async fn connect(
        database_url: &str,
        run_event_sender: broadcast::Sender<TaskRunEventRecord>,
    ) -> Result<Self, String> {
        ensure_sqlite_parent_dir(database_url)?;
        let connect_options = SqliteConnectOptions::from_str(database_url)
            .map_err(|err| err.to_string())?
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(connect_options)
            .await
            .map_err(|err| err.to_string())?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(Self {
            pool,
            cancel_requested_runs: Arc::new(RwLock::new(HashSet::new())),
            run_event_sender,
        })
    }

    async fn ensure_active_run_index(&self) -> Result<(), String> {
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_task_runs_active_task_unique
             ON task_runs(task_id)
             WHERE status IN ('queued', 'running')",
        )
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())
    }

    async fn list_tasks(&self) -> Result<Vec<TaskRecord>, String> {
        let rows = sqlx::query("SELECT * FROM tasks ORDER BY datetime(updated_at) DESC, id DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        rows.iter().map(task_from_row).collect()
    }

    async fn list_tasks_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskRecord>, String> {
        let mut clauses = Vec::new();
        let mut sql = String::from("SELECT * FROM tasks");

        if filters.status.is_some() {
            clauses.push("status = ?");
        }
        if filters.keyword.is_some() {
            clauses.push(
                "(LOWER(id) LIKE ? OR LOWER(title) LIKE ? OR LOWER(objective) LIKE ? OR LOWER(COALESCE(description, '')) LIKE ? OR LOWER(COALESCE(result_summary, '')) LIKE ? OR EXISTS (SELECT 1 FROM json_each(tasks.tags_json) WHERE LOWER(CAST(json_each.value AS TEXT)) LIKE ?))",
            );
        }
        if filters.tag.is_some() {
            clauses.push(
                "EXISTS (SELECT 1 FROM json_each(tasks.tags_json) WHERE CAST(json_each.value AS TEXT) = ?)",
            );
        }
        if filters.model_config_id.is_some() {
            clauses.push("default_model_config_id = ?");
        }
        if filters.scheduled_only.unwrap_or(false) {
            clauses.push("json_extract(schedule_json, '$.mode') <> 'manual'");
        }
        if filters.parent_task_id.is_some() {
            clauses.push("parent_task_id = ?");
        }
        if filters.source_run_id.is_some() {
            clauses.push("source_run_id = ?");
        }

        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY datetime(updated_at) DESC, id DESC");
        if filters.limit.is_some() {
            sql.push_str(" LIMIT ?");
        }
        if filters.offset.is_some() {
            if filters.limit.is_none() {
                sql.push_str(" LIMIT -1");
            }
            sql.push_str(" OFFSET ?");
        }

        let mut query = sqlx::query(&sql);
        if let Some(status) = filters.status {
            query = query.bind(task_status_to_str(status));
        }
        if let Some(keyword) = filters.keyword.as_deref() {
            let pattern = format!("%{keyword}%");
            for _ in 0..6 {
                query = query.bind(pattern.clone());
            }
        }
        if let Some(tag) = filters.tag.as_deref() {
            query = query.bind(tag);
        }
        if let Some(model_config_id) = filters.model_config_id.as_deref() {
            query = query.bind(model_config_id);
        }
        if let Some(parent_task_id) = filters.parent_task_id.as_deref() {
            query = query.bind(parent_task_id);
        }
        if let Some(source_run_id) = filters.source_run_id.as_deref() {
            query = query.bind(source_run_id);
        }
        if let Some(limit) = filters.limit {
            query = query.bind(limit as i64);
        }
        if let Some(offset) = filters.offset {
            query = query.bind(offset as i64);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        rows.iter().map(task_from_row).collect()
    }

    async fn list_tasks_page(
        &self,
        filters: &TaskListFilters,
    ) -> Result<PaginatedResponse<TaskRecord>, String> {
        let total = self.count_tasks_filtered(filters).await?;
        let items = self.list_tasks_filtered(filters).await?;
        Ok(build_page_response(
            items,
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        ))
    }

    async fn get_task(&self, id: &str) -> Result<Option<TaskRecord>, String> {
        let row = sqlx::query("SELECT * FROM tasks WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(task_from_row).transpose()
    }

    async fn list_task_summaries(&self) -> Result<Vec<TaskSummaryRecord>, String> {
        let rows = sqlx::query(
            "SELECT id, title, status, default_model_config_id, creator_user_id,
                    creator_username, creator_display_name, last_run_id, updated_at
             FROM tasks
             ORDER BY datetime(updated_at) DESC, id DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        rows.iter().map(task_summary_from_row).collect()
    }

    async fn list_task_summaries_filtered(
        &self,
        filters: &TaskListFilters,
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        Ok(self
            .list_tasks_filtered(filters)
            .await?
            .iter()
            .map(TaskSummaryRecord::from)
            .collect())
    }

    async fn get_task_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<TaskSummaryRecord>, String> {
        let mut items = Vec::new();
        for id in ids {
            if let Some(task) = self.get_task(id).await? {
                items.push(TaskSummaryRecord::from(&task));
            }
        }
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(items)
    }

    async fn list_task_tags(&self) -> Result<Vec<String>, String> {
        let rows = sqlx::query("SELECT tags_json FROM tasks WHERE tags_json IS NOT NULL")
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        let mut tags = Vec::new();
        for row in rows {
            let row_tags = decode_json::<Vec<String>>(row.get("tags_json"))?;
            tags.extend(row_tags);
        }
        tags.sort();
        tags.dedup();
        Ok(tags)
    }

    async fn task_stats(&self) -> Result<TaskStatsResponse, String> {
        let row = sqlx::query(
            "SELECT
                COUNT(1) AS total,
                COALESCE(SUM(CASE WHEN json_extract(schedule_json, '$.mode') <> 'manual' THEN 1 ELSE 0 END), 0) AS scheduled,
                COALESCE(SUM(CASE WHEN parent_task_id IS NOT NULL THEN 1 ELSE 0 END), 0) AS follow_up,
                COALESCE(SUM(CASE WHEN status = 'draft' THEN 1 ELSE 0 END), 0) AS draft,
                COALESCE(SUM(CASE WHEN status = 'ready' THEN 1 ELSE 0 END), 0) AS ready,
                COALESCE(SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END), 0) AS running,
                COALESCE(SUM(CASE WHEN status = 'succeeded' THEN 1 ELSE 0 END), 0) AS succeeded,
                COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) AS failed,
                COALESCE(SUM(CASE WHEN status = 'blocked' THEN 1 ELSE 0 END), 0) AS blocked,
                COALESCE(SUM(CASE WHEN status = 'cancelled' THEN 1 ELSE 0 END), 0) AS cancelled,
                COALESCE(SUM(CASE WHEN status = 'archived' THEN 1 ELSE 0 END), 0) AS archived
            FROM tasks",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|err| err.to_string())?;

        Ok(TaskStatsResponse {
            total: row.get::<i64, _>("total") as usize,
            scheduled: row.get::<i64, _>("scheduled") as usize,
            follow_up: row.get::<i64, _>("follow_up") as usize,
            draft: row.get::<i64, _>("draft") as usize,
            ready: row.get::<i64, _>("ready") as usize,
            running: row.get::<i64, _>("running") as usize,
            succeeded: row.get::<i64, _>("succeeded") as usize,
            failed: row.get::<i64, _>("failed") as usize,
            blocked: row.get::<i64, _>("blocked") as usize,
            cancelled: row.get::<i64, _>("cancelled") as usize,
            archived: row.get::<i64, _>("archived") as usize,
        })
    }

    async fn list_due_scheduled_tasks(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<TaskRecord>, String> {
        let rows = sqlx::query(
            "SELECT * FROM tasks
             WHERE status NOT IN ('archived', 'running')
               AND json_extract(schedule_json, '$.mode') <> 'manual'
               AND json_extract(schedule_json, '$.next_run_at') IS NOT NULL
               AND datetime(json_extract(schedule_json, '$.next_run_at')) <= datetime(?)
             ORDER BY datetime(json_extract(schedule_json, '$.next_run_at')) ASC, id ASC",
        )
        .bind(now.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        rows.iter().map(task_from_row).collect()
    }

    async fn save_task(&self, task: TaskRecord) -> Result<TaskRecord, String> {
        sqlx::query(
            "INSERT INTO tasks (
                id, title, description, objective, input_payload_json, status, priority,
                tags_json, default_model_config_id, memory_thread_id, tenant_id, subject_id,
                creator_user_id, creator_username, creator_display_name, result_summary,
                last_run_id, schedule_json, parent_task_id, source_run_id, task_tool_state_json,
                mcp_config_json, created_at, updated_at, deleted_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                description = excluded.description,
                objective = excluded.objective,
                input_payload_json = excluded.input_payload_json,
                status = excluded.status,
                priority = excluded.priority,
                tags_json = excluded.tags_json,
                default_model_config_id = excluded.default_model_config_id,
                memory_thread_id = excluded.memory_thread_id,
                tenant_id = excluded.tenant_id,
                subject_id = excluded.subject_id,
                creator_user_id = excluded.creator_user_id,
                creator_username = excluded.creator_username,
                creator_display_name = excluded.creator_display_name,
                result_summary = excluded.result_summary,
                last_run_id = excluded.last_run_id,
                schedule_json = excluded.schedule_json,
                parent_task_id = excluded.parent_task_id,
                source_run_id = excluded.source_run_id,
                task_tool_state_json = excluded.task_tool_state_json,
                mcp_config_json = excluded.mcp_config_json,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                deleted_at = excluded.deleted_at",
        )
        .bind(&task.id)
        .bind(&task.title)
        .bind(task.description.clone())
        .bind(&task.objective)
        .bind(encode_json_option(&task.input_payload)?)
        .bind(task_status_to_str(task.status))
        .bind(task.priority)
        .bind(encode_json(&task.tags)?)
        .bind(task.default_model_config_id.clone())
        .bind(&task.memory_thread_id)
        .bind(&task.tenant_id)
        .bind(&task.subject_id)
        .bind(task.creator_user_id.clone())
        .bind(task.creator_username.clone())
        .bind(task.creator_display_name.clone())
        .bind(task.result_summary.clone())
        .bind(task.last_run_id.clone())
        .bind(encode_json(&task.schedule)?)
        .bind(task.parent_task_id.clone())
        .bind(task.source_run_id.clone())
        .bind(encode_json(&task.task_tool_state)?)
        .bind(encode_json(&task.mcp_config)?)
        .bind(&task.created_at)
        .bind(&task.updated_at)
        .bind(task.deleted_at.clone())
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(task)
    }

    async fn count_users(&self) -> Result<i64, String> {
        let row = sqlx::query("SELECT COUNT(1) AS total FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.get::<i64, _>("total"))
    }

    async fn list_users(&self) -> Result<Vec<UserRecord>, String> {
        let rows =
            sqlx::query("SELECT * FROM users ORDER BY datetime(updated_at) DESC, username ASC")
                .fetch_all(&self.pool)
                .await
                .map_err(|err| err.to_string())?;
        rows.iter().map(user_from_row).collect()
    }

    async fn get_user(&self, id: &str) -> Result<Option<UserRecord>, String> {
        let row = sqlx::query("SELECT * FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(user_from_row).transpose()
    }

    async fn get_user_by_username(&self, username: &str) -> Result<Option<UserRecord>, String> {
        let row = sqlx::query("SELECT * FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(user_from_row).transpose()
    }

    async fn save_user(&self, user: UserRecord) -> Result<UserRecord, String> {
        sqlx::query(
            "INSERT INTO users (
                id, username, display_name, password_hash, enabled, created_at, updated_at,
                last_login_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                username = excluded.username,
                display_name = excluded.display_name,
                password_hash = excluded.password_hash,
                enabled = excluded.enabled,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                last_login_at = excluded.last_login_at",
        )
        .bind(&user.id)
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(&user.password_hash)
        .bind(bool_to_int(user.enabled))
        .bind(&user.created_at)
        .bind(&user.updated_at)
        .bind(user.last_login_at.clone())
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(user)
    }

    async fn delete_user(&self, id: &str) -> Result<bool, String> {
        let result = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }

    async fn delete_task(&self, id: &str) -> Result<bool, String> {
        let mut tx = self.pool.begin().await.map_err(|err| err.to_string())?;
        sqlx::query(
            "DELETE FROM ui_prompts WHERE task_id = ? OR run_id IN (SELECT id FROM task_runs WHERE task_id = ?)",
        )
        .bind(id)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
        sqlx::query("DELETE FROM task_run_events WHERE run_id IN (SELECT id FROM task_runs WHERE task_id = ?)")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        sqlx::query("DELETE FROM task_runs WHERE task_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        let result = sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        tx.commit().await.map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_model_configs(&self) -> Result<Vec<ModelConfigRecord>, String> {
        let rows =
            sqlx::query("SELECT * FROM model_configs ORDER BY datetime(updated_at) DESC, id DESC")
                .fetch_all(&self.pool)
                .await
                .map_err(|err| err.to_string())?;
        rows.iter().map(model_config_from_row).collect()
    }

    async fn get_model_config(&self, id: &str) -> Result<Option<ModelConfigRecord>, String> {
        let row = sqlx::query("SELECT * FROM model_configs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(model_config_from_row).transpose()
    }

    async fn save_model_config(
        &self,
        model: ModelConfigRecord,
    ) -> Result<ModelConfigRecord, String> {
        sqlx::query(
            "INSERT INTO model_configs (
                id, name, provider, base_url, api_key, model, temperature, max_output_tokens,
                thinking_level, supports_responses, instructions, request_cwd,
                include_prompt_cache_retention, request_body_limit_bytes, enabled,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                provider = excluded.provider,
                base_url = excluded.base_url,
                api_key = excluded.api_key,
                model = excluded.model,
                temperature = excluded.temperature,
                max_output_tokens = excluded.max_output_tokens,
                thinking_level = excluded.thinking_level,
                supports_responses = excluded.supports_responses,
                instructions = excluded.instructions,
                request_cwd = excluded.request_cwd,
                include_prompt_cache_retention = excluded.include_prompt_cache_retention,
                request_body_limit_bytes = excluded.request_body_limit_bytes,
                enabled = excluded.enabled,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
        )
        .bind(&model.id)
        .bind(&model.name)
        .bind(&model.provider)
        .bind(&model.base_url)
        .bind(&model.api_key)
        .bind(&model.model)
        .bind(model.temperature)
        .bind(model.max_output_tokens)
        .bind(model.thinking_level.clone())
        .bind(bool_to_int(model.supports_responses))
        .bind(model.instructions.clone())
        .bind(model.request_cwd.clone())
        .bind(bool_to_int(model.include_prompt_cache_retention))
        .bind(model.request_body_limit_bytes.map(|value| value as i64))
        .bind(bool_to_int(model.enabled))
        .bind(&model.created_at)
        .bind(&model.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(model)
    }

    async fn delete_model_config(&self, id: &str) -> Result<bool, String> {
        let mut tx = self.pool.begin().await.map_err(|err| err.to_string())?;
        let result = sqlx::query("DELETE FROM model_configs WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|err| err.to_string())?;
        sqlx::query(
            "UPDATE tasks SET default_model_config_id = NULL WHERE default_model_config_id = ?",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
        tx.commit().await.map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_remote_servers(&self) -> Result<Vec<RemoteServerRecord>, String> {
        let rows =
            sqlx::query("SELECT * FROM remote_servers ORDER BY datetime(updated_at) DESC, id DESC")
                .fetch_all(&self.pool)
                .await
                .map_err(|err| err.to_string())?;
        rows.iter().map(remote_server_from_row).collect()
    }

    async fn get_remote_server(&self, id: &str) -> Result<Option<RemoteServerRecord>, String> {
        let row = sqlx::query("SELECT * FROM remote_servers WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(remote_server_from_row).transpose()
    }

    async fn save_remote_server(
        &self,
        server: RemoteServerRecord,
    ) -> Result<RemoteServerRecord, String> {
        sqlx::query(
            "INSERT INTO remote_servers (
                id, name, host, port, username, auth_type, password, private_key_path,
                certificate_path, default_remote_path, host_key_policy, enabled,
                last_tested_at, last_test_status, last_test_message, last_active_at,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                host = excluded.host,
                port = excluded.port,
                username = excluded.username,
                auth_type = excluded.auth_type,
                password = excluded.password,
                private_key_path = excluded.private_key_path,
                certificate_path = excluded.certificate_path,
                default_remote_path = excluded.default_remote_path,
                host_key_policy = excluded.host_key_policy,
                enabled = excluded.enabled,
                last_tested_at = excluded.last_tested_at,
                last_test_status = excluded.last_test_status,
                last_test_message = excluded.last_test_message,
                last_active_at = excluded.last_active_at,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
        )
        .bind(&server.id)
        .bind(&server.name)
        .bind(&server.host)
        .bind(server.port)
        .bind(&server.username)
        .bind(&server.auth_type)
        .bind(server.password.clone())
        .bind(server.private_key_path.clone())
        .bind(server.certificate_path.clone())
        .bind(server.default_remote_path.clone())
        .bind(&server.host_key_policy)
        .bind(bool_to_int(server.enabled))
        .bind(server.last_tested_at.clone())
        .bind(server.last_test_status.clone())
        .bind(server.last_test_message.clone())
        .bind(server.last_active_at.clone())
        .bind(&server.created_at)
        .bind(&server.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(server)
    }

    async fn delete_remote_server(&self, id: &str) -> Result<bool, String> {
        let result = sqlx::query("DELETE FROM remote_servers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_runs(&self, task_id: Option<&str>) -> Result<Vec<TaskRunRecord>, String> {
        let rows = if let Some(task_id) = task_id {
            sqlx::query(
                "SELECT * FROM task_runs WHERE task_id = ? ORDER BY datetime(created_at) DESC, id DESC",
            )
            .bind(task_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?
        } else {
            sqlx::query("SELECT * FROM task_runs ORDER BY datetime(created_at) DESC, id DESC")
                .fetch_all(&self.pool)
                .await
                .map_err(|err| err.to_string())?
        };
        rows.iter().map(task_run_from_row).collect()
    }

    async fn list_runs_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<Vec<TaskRunRecord>, String> {
        let mut clauses = Vec::new();
        let mut sql = String::from("SELECT * FROM task_runs");
        if filters.task_id.is_some() {
            clauses.push("task_id = ?");
        }
        if filters.status.is_some() {
            clauses.push("status = ?");
        }
        if filters.model_config_id.is_some() {
            clauses.push("model_config_id = ?");
        }
        if filters.keyword.is_some() {
            clauses.push(
                "(LOWER(id) LIKE ? OR LOWER(task_id) LIKE ? OR LOWER(model_config_id) LIKE ? OR LOWER(COALESCE(result_summary, '')) LIKE ? OR LOWER(COALESCE(error_message, '')) LIKE ?)",
            );
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY datetime(created_at) DESC, id DESC");
        if filters.limit.is_some() {
            sql.push_str(" LIMIT ?");
        }
        if filters.offset.is_some() {
            if filters.limit.is_none() {
                sql.push_str(" LIMIT -1");
            }
            sql.push_str(" OFFSET ?");
        }

        let mut query = sqlx::query(&sql);
        if let Some(task_id) = filters.task_id.as_deref() {
            query = query.bind(task_id);
        }
        if let Some(status) = filters.status {
            query = query.bind(task_run_status_to_str(status));
        }
        if let Some(model_config_id) = filters.model_config_id.as_deref() {
            query = query.bind(model_config_id);
        }
        if let Some(keyword) = filters.keyword.as_deref() {
            let pattern = format!("%{keyword}%");
            for _ in 0..5 {
                query = query.bind(pattern.clone());
            }
        }
        if let Some(limit) = filters.limit {
            query = query.bind(limit as i64);
        }
        if let Some(offset) = filters.offset {
            query = query.bind(offset as i64);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        rows.iter().map(task_run_from_row).collect()
    }

    async fn list_runs_page(
        &self,
        filters: &RunListFilters,
    ) -> Result<PaginatedResponse<TaskRunRecord>, String> {
        let total = self.count_runs_filtered(filters).await?;
        let items = self.list_runs_filtered(filters).await?;
        Ok(build_page_response(
            items,
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        ))
    }

    async fn list_run_summaries_filtered(
        &self,
        filters: &RunListFilters,
    ) -> Result<Vec<RunSummaryRecord>, String> {
        let mut clauses = Vec::new();
        let mut sql =
            String::from("SELECT id, task_id, status, model_config_id, updated_at FROM task_runs");
        if filters.task_id.is_some() {
            clauses.push("task_id = ?");
        }
        if filters.status.is_some() {
            clauses.push("status = ?");
        }
        if filters.model_config_id.is_some() {
            clauses.push("model_config_id = ?");
        }
        if filters.keyword.is_some() {
            clauses.push(
                "(LOWER(id) LIKE ? OR LOWER(task_id) LIKE ? OR LOWER(model_config_id) LIKE ? OR LOWER(COALESCE(result_summary, '')) LIKE ? OR LOWER(COALESCE(error_message, '')) LIKE ?)",
            );
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY datetime(updated_at) DESC, id DESC");
        if filters.limit.is_some() {
            sql.push_str(" LIMIT ?");
        }
        if filters.offset.is_some() {
            if filters.limit.is_none() {
                sql.push_str(" LIMIT -1");
            }
            sql.push_str(" OFFSET ?");
        }

        let mut query = sqlx::query(&sql);
        if let Some(task_id) = filters.task_id.as_deref() {
            query = query.bind(task_id);
        }
        if let Some(status) = filters.status {
            query = query.bind(task_run_status_to_str(status));
        }
        if let Some(model_config_id) = filters.model_config_id.as_deref() {
            query = query.bind(model_config_id);
        }
        if let Some(keyword) = filters.keyword.as_deref() {
            let pattern = format!("%{keyword}%");
            for _ in 0..5 {
                query = query.bind(pattern.clone());
            }
        }
        if let Some(limit) = filters.limit {
            query = query.bind(limit as i64);
        }
        if let Some(offset) = filters.offset {
            query = query.bind(offset as i64);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        rows.iter().map(run_summary_from_row).collect()
    }

    async fn get_run_summaries_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<RunSummaryRecord>, String> {
        let mut items = Vec::new();
        for id in ids {
            if let Some(run) = self.get_run(id).await? {
                items.push(RunSummaryRecord::from(&run));
            }
        }
        items.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(items)
    }

    async fn list_model_config_usage(&self) -> Result<Vec<ModelConfigUsageRecord>, String> {
        let rows = sqlx::query(
            "WITH task_counts AS (
                SELECT default_model_config_id AS model_config_id, COUNT(1) AS task_count
                FROM tasks
                WHERE default_model_config_id IS NOT NULL
                GROUP BY default_model_config_id
            ),
            run_counts AS (
                SELECT model_config_id, COUNT(1) AS run_count
                FROM task_runs
                GROUP BY model_config_id
            ),
            model_ids AS (
                SELECT model_config_id FROM task_counts
                UNION
                SELECT model_config_id FROM run_counts
            )
            SELECT
                model_ids.model_config_id AS model_config_id,
                COALESCE(task_counts.task_count, 0) AS task_count,
                COALESCE(run_counts.run_count, 0) AS run_count
            FROM model_ids
            LEFT JOIN task_counts ON task_counts.model_config_id = model_ids.model_config_id
            LEFT JOIN run_counts ON run_counts.model_config_id = model_ids.model_config_id
            ORDER BY model_ids.model_config_id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(rows
            .into_iter()
            .map(|row| ModelConfigUsageRecord {
                model_config_id: row.get("model_config_id"),
                task_count: row.get::<i64, _>("task_count") as usize,
                run_count: row.get::<i64, _>("run_count") as usize,
            })
            .collect())
    }

    async fn get_run(&self, id: &str) -> Result<Option<TaskRunRecord>, String> {
        let row = sqlx::query("SELECT * FROM task_runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(task_run_from_row).transpose()
    }

    async fn save_run(&self, run: TaskRunRecord) -> Result<TaskRunRecord, String> {
        if run.cancel_requested {
            self.cancel_requested_runs.write().insert(run.id.clone());
        } else {
            self.cancel_requested_runs.write().remove(&run.id);
        }
        sqlx::query(
            "INSERT INTO task_runs (
                id, task_id, model_config_id, memory_thread_id, status, started_at, finished_at,
                input_snapshot_json, context_snapshot_json, result_summary, error_message,
                usage_json, report_json, cancel_requested, summary_job_run_id, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                task_id = excluded.task_id,
                model_config_id = excluded.model_config_id,
                memory_thread_id = excluded.memory_thread_id,
                status = excluded.status,
                started_at = excluded.started_at,
                finished_at = excluded.finished_at,
                input_snapshot_json = excluded.input_snapshot_json,
                context_snapshot_json = excluded.context_snapshot_json,
                result_summary = excluded.result_summary,
                error_message = excluded.error_message,
                usage_json = excluded.usage_json,
                report_json = excluded.report_json,
                cancel_requested = excluded.cancel_requested,
                summary_job_run_id = excluded.summary_job_run_id,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
        )
        .bind(&run.id)
        .bind(&run.task_id)
        .bind(&run.model_config_id)
        .bind(&run.memory_thread_id)
        .bind(task_run_status_to_str(run.status))
        .bind(run.started_at.clone())
        .bind(run.finished_at.clone())
        .bind(encode_json(&run.input_snapshot)?)
        .bind(encode_json_option(&run.context_snapshot)?)
        .bind(run.result_summary.clone())
        .bind(run.error_message.clone())
        .bind(encode_json_option(&run.usage)?)
        .bind(encode_json_option(&run.report)?)
        .bind(bool_to_int(run.cancel_requested))
        .bind(run.summary_job_run_id.clone())
        .bind(&run.created_at)
        .bind(&run.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(run)
    }

    async fn list_run_events(&self, run_id: &str) -> Result<Vec<TaskRunEventRecord>, String> {
        let rows = sqlx::query(
            "SELECT * FROM task_run_events WHERE run_id = ? ORDER BY datetime(created_at) ASC, id ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        rows.iter().map(task_run_event_from_row).collect()
    }

    async fn append_run_event(&self, event: TaskRunEventRecord) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO task_run_events (id, run_id, event_type, message, payload_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&event.id)
        .bind(&event.run_id)
        .bind(&event.event_type)
        .bind(event.message.clone())
        .bind(encode_json_option(&event.payload)?)
        .bind(&event.created_at)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        let _ = self.run_event_sender.send(event);
        Ok(())
    }

    async fn list_ui_prompts(
        &self,
        task_id: Option<&str>,
        run_id: Option<&str>,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptRecord>, String> {
        let mut clauses = Vec::new();
        if task_id.is_some() {
            clauses.push("task_id = ?");
        }
        if run_id.is_some() {
            clauses.push("run_id = ?");
        }
        if status.is_some() {
            clauses.push("status = ?");
        }

        let mut sql = "SELECT * FROM ui_prompts".to_string();
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY datetime(updated_at) DESC, id DESC");

        let mut query = sqlx::query(&sql);
        if let Some(task_id) = task_id {
            query = query.bind(task_id);
        }
        if let Some(run_id) = run_id {
            query = query.bind(run_id);
        }
        if let Some(status) = status {
            query = query.bind(ui_prompt_status_to_str(status));
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        rows.iter().map(ui_prompt_from_row).collect()
    }

    async fn list_ui_prompts_page(
        &self,
        filters: &PromptListFilters,
    ) -> Result<PaginatedResponse<UiPromptRecord>, String> {
        let total = self.count_ui_prompts_filtered(filters).await?;
        let items = self.list_ui_prompts_filtered(filters).await?;
        Ok(build_page_response(
            items,
            total,
            filters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
            filters.offset.unwrap_or(0),
        ))
    }

    async fn list_ui_prompts_filtered(
        &self,
        filters: &PromptListFilters,
    ) -> Result<Vec<UiPromptRecord>, String> {
        let mut clauses = Vec::new();
        if filters.task_id.is_some() {
            clauses.push("task_id = ?");
        }
        if filters.run_id.is_some() {
            clauses.push("run_id = ?");
        }
        if filters.status.is_some() {
            clauses.push("status = ?");
        }

        let mut sql = "SELECT * FROM ui_prompts".to_string();
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY datetime(updated_at) DESC, id DESC");
        if filters.limit.is_some() {
            sql.push_str(" LIMIT ?");
        }
        if filters.offset.is_some() {
            if filters.limit.is_none() {
                sql.push_str(" LIMIT -1");
            }
            sql.push_str(" OFFSET ?");
        }

        let mut query = sqlx::query(&sql);
        if let Some(task_id) = filters.task_id.as_deref() {
            query = query.bind(task_id);
        }
        if let Some(run_id) = filters.run_id.as_deref() {
            query = query.bind(run_id);
        }
        if let Some(status) = filters.status {
            query = query.bind(ui_prompt_status_to_str(status));
        }
        if let Some(limit) = filters.limit {
            query = query.bind(limit as i64);
        }
        if let Some(offset) = filters.offset {
            query = query.bind(offset as i64);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        rows.iter().map(ui_prompt_from_row).collect()
    }

    async fn get_ui_prompt(&self, id: &str) -> Result<Option<UiPromptRecord>, String> {
        let row = sqlx::query("SELECT * FROM ui_prompts WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        row.as_ref().map(ui_prompt_from_row).transpose()
    }

    async fn save_ui_prompt(&self, prompt: UiPromptRecord) -> Result<UiPromptRecord, String> {
        sqlx::query(
            "INSERT INTO ui_prompts (
                id, task_id, run_id, conversation_id, conversation_turn_id, tool_call_id, kind,
                title, message, allow_cancel, timeout_ms, payload_json, response_json, status,
                created_at, updated_at, expires_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                task_id = excluded.task_id,
                run_id = excluded.run_id,
                conversation_id = excluded.conversation_id,
                conversation_turn_id = excluded.conversation_turn_id,
                tool_call_id = excluded.tool_call_id,
                kind = excluded.kind,
                title = excluded.title,
                message = excluded.message,
                allow_cancel = excluded.allow_cancel,
                timeout_ms = excluded.timeout_ms,
                payload_json = excluded.payload_json,
                response_json = excluded.response_json,
                status = excluded.status,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                expires_at = excluded.expires_at",
        )
        .bind(&prompt.id)
        .bind(prompt.task_id.clone())
        .bind(prompt.run_id.clone())
        .bind(&prompt.conversation_id)
        .bind(&prompt.conversation_turn_id)
        .bind(prompt.tool_call_id.clone())
        .bind(&prompt.kind)
        .bind(&prompt.title)
        .bind(&prompt.message)
        .bind(bool_to_int(prompt.allow_cancel))
        .bind(prompt.timeout_ms as i64)
        .bind(encode_json(&prompt.payload)?)
        .bind(encode_json_optional(prompt.response.as_ref())?)
        .bind(ui_prompt_status_to_str(prompt.status))
        .bind(&prompt.created_at)
        .bind(&prompt.updated_at)
        .bind(prompt.expires_at.clone())
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(prompt)
    }

    async fn list_ui_prompt_task_counts(
        &self,
        status: Option<UiPromptStatus>,
    ) -> Result<Vec<UiPromptTaskCountRecord>, String> {
        let mut sql =
            "SELECT task_id, COUNT(1) AS prompt_count FROM ui_prompts WHERE task_id IS NOT NULL"
                .to_string();
        if status.is_some() {
            sql.push_str(" AND status = ?");
        }
        sql.push_str(" GROUP BY task_id ORDER BY prompt_count DESC, task_id ASC");

        let mut query = sqlx::query(&sql);
        if let Some(status) = status {
            query = query.bind(ui_prompt_status_to_str(status));
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(rows
            .into_iter()
            .map(|row| UiPromptTaskCountRecord {
                task_id: row.get("task_id"),
                count: row.get::<i64, _>("prompt_count") as usize,
            })
            .collect())
    }

    async fn mark_cancel_requested(&self, run_id: &str) -> Result<Option<TaskRunRecord>, String> {
        self.cancel_requested_runs
            .write()
            .insert(run_id.to_string());
        sqlx::query(
            "UPDATE task_runs SET cancel_requested = 1, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        )
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        self.get_run(run_id).await
    }

    fn clear_cancel_requested(&self, run_id: &str) {
        self.cancel_requested_runs.write().remove(run_id);
        let pool = self.pool.clone();
        let run_id = run_id.to_string();
        tokio::spawn(async move {
            if let Err(err) = sqlx::query("UPDATE task_runs SET cancel_requested = 0 WHERE id = ?")
                .bind(run_id)
                .execute(&pool)
                .await
            {
                warn!("failed to clear cancel_requested flag: {err}");
            }
        });
    }

    fn is_cancel_requested(&self, run_id: &str) -> bool {
        self.cancel_requested_runs.read().contains(run_id)
    }

    async fn has_active_run_for_task(&self, task_id: &str) -> Result<bool, String> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM task_runs WHERE task_id = ? AND status IN ('queued', 'running')",
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| err.to_string())?;
        Ok(count > 0)
    }

    async fn count_tasks_filtered(&self, filters: &TaskListFilters) -> Result<usize, String> {
        let mut clauses = Vec::new();
        let mut sql = String::from("SELECT COUNT(1) AS total FROM tasks");

        if filters.status.is_some() {
            clauses.push("status = ?");
        }
        if filters.keyword.is_some() {
            clauses.push(
                "(LOWER(id) LIKE ? OR LOWER(title) LIKE ? OR LOWER(objective) LIKE ? OR LOWER(COALESCE(description, '')) LIKE ? OR LOWER(COALESCE(result_summary, '')) LIKE ? OR EXISTS (SELECT 1 FROM json_each(tasks.tags_json) WHERE LOWER(CAST(json_each.value AS TEXT)) LIKE ?))",
            );
        }
        if filters.tag.is_some() {
            clauses.push(
                "EXISTS (SELECT 1 FROM json_each(tasks.tags_json) WHERE CAST(json_each.value AS TEXT) = ?)",
            );
        }
        if filters.model_config_id.is_some() {
            clauses.push("default_model_config_id = ?");
        }
        if filters.scheduled_only.unwrap_or(false) {
            clauses.push("json_extract(schedule_json, '$.mode') <> 'manual'");
        }
        if filters.parent_task_id.is_some() {
            clauses.push("parent_task_id = ?");
        }
        if filters.source_run_id.is_some() {
            clauses.push("source_run_id = ?");
        }

        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }

        let mut query = sqlx::query(&sql);
        if let Some(status) = filters.status {
            query = query.bind(task_status_to_str(status));
        }
        if let Some(keyword) = filters.keyword.as_deref() {
            let pattern = format!("%{keyword}%");
            for _ in 0..6 {
                query = query.bind(pattern.clone());
            }
        }
        if let Some(tag) = filters.tag.as_deref() {
            query = query.bind(tag);
        }
        if let Some(model_config_id) = filters.model_config_id.as_deref() {
            query = query.bind(model_config_id);
        }
        if let Some(parent_task_id) = filters.parent_task_id.as_deref() {
            query = query.bind(parent_task_id);
        }
        if let Some(source_run_id) = filters.source_run_id.as_deref() {
            query = query.bind(source_run_id);
        }

        let row = query
            .fetch_one(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.get::<i64, _>("total") as usize)
    }

    async fn count_runs_filtered(&self, filters: &RunListFilters) -> Result<usize, String> {
        let mut clauses = Vec::new();
        let mut sql = String::from("SELECT COUNT(1) AS total FROM task_runs");
        if filters.task_id.is_some() {
            clauses.push("task_id = ?");
        }
        if filters.status.is_some() {
            clauses.push("status = ?");
        }
        if filters.model_config_id.is_some() {
            clauses.push("model_config_id = ?");
        }
        if filters.keyword.is_some() {
            clauses.push(
                "(LOWER(id) LIKE ? OR LOWER(task_id) LIKE ? OR LOWER(model_config_id) LIKE ? OR LOWER(COALESCE(result_summary, '')) LIKE ? OR LOWER(COALESCE(error_message, '')) LIKE ?)",
            );
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }

        let mut query = sqlx::query(&sql);
        if let Some(task_id) = filters.task_id.as_deref() {
            query = query.bind(task_id);
        }
        if let Some(status) = filters.status {
            query = query.bind(task_run_status_to_str(status));
        }
        if let Some(model_config_id) = filters.model_config_id.as_deref() {
            query = query.bind(model_config_id);
        }
        if let Some(keyword) = filters.keyword.as_deref() {
            let pattern = format!("%{keyword}%");
            for _ in 0..5 {
                query = query.bind(pattern.clone());
            }
        }

        let row = query
            .fetch_one(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.get::<i64, _>("total") as usize)
    }

    async fn count_ui_prompts_filtered(
        &self,
        filters: &PromptListFilters,
    ) -> Result<usize, String> {
        let mut clauses = Vec::new();
        let mut sql = String::from("SELECT COUNT(1) AS total FROM ui_prompts");
        if filters.task_id.is_some() {
            clauses.push("task_id = ?");
        }
        if filters.run_id.is_some() {
            clauses.push("run_id = ?");
        }
        if filters.status.is_some() {
            clauses.push("status = ?");
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }

        let mut query = sqlx::query(&sql);
        if let Some(task_id) = filters.task_id.as_deref() {
            query = query.bind(task_id);
        }
        if let Some(run_id) = filters.run_id.as_deref() {
            query = query.bind(run_id);
        }
        if let Some(status) = filters.status {
            query = query.bind(ui_prompt_status_to_str(status));
        }

        let row = query
            .fetch_one(&self.pool)
            .await
            .map_err(|err| err.to_string())?;
        Ok(row.get::<i64, _>("total") as usize)
    }
}

fn task_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<TaskRecord, String> {
    Ok(TaskRecord {
        id: row.get("id"),
        title: row.get("title"),
        description: row.get("description"),
        objective: row.get("objective"),
        input_payload: decode_json_option(row.get("input_payload_json"))?,
        status: task_status_from_str(row.get::<String, _>("status").as_str()),
        priority: row.get::<i64, _>("priority") as i32,
        tags: decode_json(row.get("tags_json"))?,
        default_model_config_id: row.get("default_model_config_id"),
        memory_thread_id: row.get("memory_thread_id"),
        tenant_id: row.get("tenant_id"),
        subject_id: row.get("subject_id"),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        result_summary: row.get("result_summary"),
        last_run_id: row.get("last_run_id"),
        schedule: decode_json(row.get("schedule_json"))?,
        parent_task_id: row.get("parent_task_id"),
        source_run_id: row.get("source_run_id"),
        task_tool_state: decode_json(row.get("task_tool_state_json"))?,
        mcp_config: decode_json(row.get("mcp_config_json"))?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        deleted_at: row.get("deleted_at"),
    })
}

fn task_summary_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<TaskSummaryRecord, String> {
    Ok(TaskSummaryRecord {
        id: row.get("id"),
        title: row.get("title"),
        status: task_status_from_str(row.get::<String, _>("status").as_str()),
        default_model_config_id: row.get("default_model_config_id"),
        creator_user_id: row.get("creator_user_id"),
        creator_username: row.get("creator_username"),
        creator_display_name: row.get("creator_display_name"),
        last_run_id: row.get("last_run_id"),
        updated_at: row.get("updated_at"),
    })
}

fn user_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<UserRecord, String> {
    Ok(UserRecord {
        id: row.get("id"),
        username: row.get("username"),
        display_name: row.get("display_name"),
        password_hash: row.get("password_hash"),
        enabled: int_to_bool(row.get::<i64, _>("enabled")),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        last_login_at: row.get("last_login_at"),
    })
}

fn model_config_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ModelConfigRecord, String> {
    Ok(ModelConfigRecord {
        id: row.get("id"),
        name: row.get("name"),
        provider: row.get("provider"),
        base_url: row.get("base_url"),
        api_key: row.get("api_key"),
        model: row.get("model"),
        temperature: row.get("temperature"),
        max_output_tokens: row.get("max_output_tokens"),
        thinking_level: row.get("thinking_level"),
        supports_responses: int_to_bool(row.get::<i64, _>("supports_responses")),
        instructions: row.get("instructions"),
        request_cwd: row.get("request_cwd"),
        include_prompt_cache_retention: int_to_bool(
            row.get::<i64, _>("include_prompt_cache_retention"),
        ),
        request_body_limit_bytes: row
            .get::<Option<i64>, _>("request_body_limit_bytes")
            .map(|value| value as usize),
        enabled: int_to_bool(row.get::<i64, _>("enabled")),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn remote_server_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<RemoteServerRecord, String> {
    Ok(RemoteServerRecord {
        id: row.get("id"),
        name: row.get("name"),
        host: row.get("host"),
        port: row.get("port"),
        username: row.get("username"),
        auth_type: row.get("auth_type"),
        password: row.get("password"),
        private_key_path: row.get("private_key_path"),
        certificate_path: row.get("certificate_path"),
        default_remote_path: row.get("default_remote_path"),
        host_key_policy: row.get("host_key_policy"),
        enabled: int_to_bool(row.get::<i64, _>("enabled")),
        last_tested_at: row.get("last_tested_at"),
        last_test_status: row.get("last_test_status"),
        last_test_message: row.get("last_test_message"),
        last_active_at: row.get("last_active_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn task_run_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<TaskRunRecord, String> {
    Ok(TaskRunRecord {
        id: row.get("id"),
        task_id: row.get("task_id"),
        model_config_id: row.get("model_config_id"),
        memory_thread_id: row.get("memory_thread_id"),
        status: task_run_status_from_str(row.get::<String, _>("status").as_str()),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        input_snapshot: decode_json(row.get("input_snapshot_json"))?,
        context_snapshot: decode_json_option(row.get("context_snapshot_json"))?,
        result_summary: row.get("result_summary"),
        error_message: row.get("error_message"),
        usage: decode_json_option(row.get("usage_json"))?,
        report: decode_json_option(row.get("report_json"))?,
        cancel_requested: int_to_bool(row.get::<i64, _>("cancel_requested")),
        summary_job_run_id: row.get("summary_job_run_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn run_summary_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<RunSummaryRecord, String> {
    Ok(RunSummaryRecord {
        id: row.get("id"),
        task_id: row.get("task_id"),
        status: task_run_status_from_str(row.get::<String, _>("status").as_str()),
        model_config_id: row.get("model_config_id"),
        updated_at: row.get("updated_at"),
    })
}

fn task_run_event_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<TaskRunEventRecord, String> {
    Ok(TaskRunEventRecord {
        id: row.get("id"),
        run_id: row.get("run_id"),
        event_type: row.get("event_type"),
        message: row.get("message"),
        payload: decode_json_option(row.get("payload_json"))?,
        created_at: row.get("created_at"),
    })
}

fn ui_prompt_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<UiPromptRecord, String> {
    Ok(UiPromptRecord {
        id: row.get("id"),
        task_id: row.get("task_id"),
        run_id: row.get("run_id"),
        conversation_id: row.get("conversation_id"),
        conversation_turn_id: row.get("conversation_turn_id"),
        tool_call_id: row.get("tool_call_id"),
        kind: row.get("kind"),
        title: row.get("title"),
        message: row.get("message"),
        allow_cancel: int_to_bool(row.get::<i64, _>("allow_cancel")),
        timeout_ms: row.get::<i64, _>("timeout_ms") as u64,
        payload: decode_json(row.get("payload_json"))?,
        response: decode_json_optional_typed(row.get("response_json"))?,
        status: ui_prompt_status_from_str(row.get::<String, _>("status").as_str()),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        expires_at: row.get("expires_at"),
    })
}

fn ensure_sqlite_parent_dir(database_url: &str) -> Result<(), String> {
    let Some(path) = sqlite_database_path(database_url) else {
        return Ok(());
    };
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|err| err.to_string())
}

fn sqlite_database_path(database_url: &str) -> Option<PathBuf> {
    let normalized = database_url.trim();
    if normalized.is_empty() || normalized == "sqlite::memory:" {
        return None;
    }
    let path = normalized
        .strip_prefix("sqlite://")
        .or_else(|| normalized.strip_prefix("sqlite:"))?;
    let path = path.split('?').next().unwrap_or(path).trim();
    if path.is_empty() || path == ":memory:" {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn is_mongo_active_run_index_conflict(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("e11000")
        || normalized.contains("duplicate key")
        || normalized.contains(ACTIVE_TASK_RUN_UNIQUE_INDEX_NAME)
}

fn is_mongo_active_run_conflict(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    (normalized.contains("e11000") || normalized.contains("duplicate key"))
        && normalized.contains("task_id")
}

fn build_mongo_task_filter(filters: &TaskListFilters) -> Document {
    let mut filter = Document::new();
    if let Some(status) = filters.status {
        filter.insert("status", task_status_to_str(status));
    }
    if let Some(keyword) = filters.keyword.as_deref() {
        let regex = doc! {
            "$regex": escape_regex_pattern(keyword),
            "$options": "i",
        };
        filter.insert(
            "$or",
            vec![
                doc! { "id": regex.clone() },
                doc! { "title": regex.clone() },
                doc! { "objective": regex.clone() },
                doc! { "description": regex.clone() },
                doc! { "result_summary": regex.clone() },
                doc! { "tags": regex },
            ],
        );
    }
    if let Some(tag) = filters.tag.as_deref() {
        filter.insert("tags", tag);
    }
    if let Some(model_config_id) = filters.model_config_id.as_deref() {
        filter.insert("default_model_config_id", model_config_id);
    }
    if filters.scheduled_only.unwrap_or(false) {
        filter.insert("schedule.mode", doc! { "$ne": "manual" });
    }
    if let Some(parent_task_id) = filters.parent_task_id.as_deref() {
        filter.insert("parent_task_id", parent_task_id);
    }
    if let Some(source_run_id) = filters.source_run_id.as_deref() {
        filter.insert("source_run_id", source_run_id);
    }
    filter
}

fn build_mongo_run_filter(filters: &RunListFilters) -> Document {
    let mut filter = Document::new();
    if let Some(task_id) = filters.task_id.as_deref() {
        filter.insert("task_id", task_id);
    }
    if let Some(status) = filters.status {
        filter.insert("status", task_run_status_to_str(status));
    }
    if let Some(model_config_id) = filters.model_config_id.as_deref() {
        filter.insert("model_config_id", model_config_id);
    }
    if let Some(keyword) = filters.keyword.as_deref() {
        let regex = doc! {
            "$regex": escape_regex_pattern(keyword),
            "$options": "i",
        };
        filter.insert(
            "$or",
            vec![
                doc! { "id": regex.clone() },
                doc! { "task_id": regex.clone() },
                doc! { "model_config_id": regex.clone() },
                doc! { "result_summary": regex.clone() },
                doc! { "error_message": regex },
            ],
        );
    }
    filter
}

fn build_mongo_prompt_filter(
    task_id: Option<&str>,
    run_id: Option<&str>,
    status: Option<UiPromptStatus>,
) -> Document {
    let mut filter = Document::new();
    if let Some(task_id) = task_id {
        filter.insert("task_id", task_id);
    }
    if let Some(run_id) = run_id {
        filter.insert("run_id", run_id);
    }
    if let Some(status) = status {
        filter.insert("status", ui_prompt_status_to_str(status));
    }
    filter
}

fn mongo_find_options(sort: Document, offset: Option<usize>, limit: Option<usize>) -> FindOptions {
    let mut options = FindOptions::default();
    options.sort = Some(sort);
    options.skip = offset.filter(|value| *value > 0).map(|value| value as u64);
    options.limit = limit.map(|value| value as i64);
    options
}

fn build_skip_stage(offset: Option<usize>) -> Document {
    match offset.filter(|value| *value > 0) {
        Some(offset) => doc! { "$skip": offset as i64 },
        None => Document::new(),
    }
}

fn build_limit_stage(limit: Option<usize>) -> Document {
    match limit {
        Some(limit) => doc! { "$limit": limit as i64 },
        None => Document::new(),
    }
}

fn escape_regex_pattern(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn bson_string_field(doc: &Document, field: &str) -> Option<String> {
    match doc.get(field) {
        Some(Bson::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn bson_usize_field(doc: &Document, field: &str) -> Option<usize> {
    match doc.get(field) {
        Some(Bson::Int32(value)) if *value >= 0 => Some(*value as usize),
        Some(Bson::Int64(value)) if *value >= 0 => Some(*value as usize),
        Some(Bson::Double(value)) if *value >= 0.0 => Some(*value as usize),
        _ => None,
    }
}

fn encode_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|err| err.to_string())
}

fn encode_json_option(value: &Option<Value>) -> Result<String, String> {
    match value {
        Some(value) => encode_json(value),
        None => Ok("null".to_string()),
    }
}

fn encode_json_optional<T: Serialize>(value: Option<&T>) -> Result<String, String> {
    match value {
        Some(value) => encode_json(value),
        None => Ok("null".to_string()),
    }
}

fn decode_json<T>(text: String) -> Result<T, String>
where
    T: DeserializeOwned,
{
    serde_json::from_str(&text).map_err(|err| err.to_string())
}

fn decode_json_option(text: String) -> Result<Option<Value>, String> {
    let value: Value = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    if value.is_null() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn decode_json_optional_typed<T>(text: String) -> Result<Option<T>, String>
where
    T: DeserializeOwned,
{
    let value: Value = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    if value.is_null() {
        Ok(None)
    } else {
        serde_json::from_value(value)
            .map(Some)
            .map_err(|err| err.to_string())
    }
}

fn bool_to_int(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn int_to_bool(value: i64) -> bool {
    value != 0
}

fn task_matches_keyword(task: &TaskRecord, keyword: &str) -> bool {
    let contains = |value: &str| value.to_ascii_lowercase().contains(keyword);
    contains(&task.title)
        || contains(&task.objective)
        || task.description.as_deref().is_some_and(contains)
        || task.result_summary.as_deref().is_some_and(contains)
        || contains(&task.id)
        || task.tags.iter().any(|tag| contains(tag))
}

fn task_status_to_str(status: TaskStatus) -> &'static str {
    match status {
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

fn task_status_from_str(value: &str) -> TaskStatus {
    match value {
        "ready" => TaskStatus::Ready,
        "running" => TaskStatus::Running,
        "succeeded" => TaskStatus::Succeeded,
        "failed" => TaskStatus::Failed,
        "blocked" => TaskStatus::Blocked,
        "cancelled" => TaskStatus::Cancelled,
        "archived" => TaskStatus::Archived,
        _ => TaskStatus::Draft,
    }
}

fn task_run_status_to_str(status: TaskRunStatus) -> &'static str {
    match status {
        TaskRunStatus::Queued => "queued",
        TaskRunStatus::Running => "running",
        TaskRunStatus::Succeeded => "succeeded",
        TaskRunStatus::Failed => "failed",
        TaskRunStatus::Cancelled => "cancelled",
        TaskRunStatus::Blocked => "blocked",
    }
}

fn task_run_status_from_str(value: &str) -> TaskRunStatus {
    match value {
        "running" => TaskRunStatus::Running,
        "succeeded" => TaskRunStatus::Succeeded,
        "failed" => TaskRunStatus::Failed,
        "cancelled" => TaskRunStatus::Cancelled,
        "blocked" => TaskRunStatus::Blocked,
        _ => TaskRunStatus::Queued,
    }
}

fn ui_prompt_status_to_str(status: UiPromptStatus) -> &'static str {
    match status {
        UiPromptStatus::Pending => "pending",
        UiPromptStatus::Submitted => "submitted",
        UiPromptStatus::Cancelled => "cancelled",
        UiPromptStatus::TimedOut => "timed_out",
        UiPromptStatus::Failed => "failed",
    }
}

fn ui_prompt_status_from_str(value: &str) -> UiPromptStatus {
    match value {
        "submitted" => UiPromptStatus::Submitted,
        "cancelled" => UiPromptStatus::Cancelled,
        "timed_out" => UiPromptStatus::TimedOut,
        "failed" => UiPromptStatus::Failed,
        _ => UiPromptStatus::Pending,
    }
}

fn empty_task_stats() -> TaskStatsResponse {
    TaskStatsResponse {
        total: 0,
        scheduled: 0,
        follow_up: 0,
        draft: 0,
        ready: 0,
        running: 0,
        succeeded: 0,
        failed: 0,
        blocked: 0,
        cancelled: 0,
        archived: 0,
    }
}

fn task_due_for_scheduler(task: &TaskRecord, now: &DateTime<Utc>) -> bool {
    if matches!(task.status, TaskStatus::Archived | TaskStatus::Running) {
        return false;
    }
    if matches!(task.schedule.mode, TaskScheduleMode::Manual) {
        return false;
    }
    task_due_at(task).is_some_and(|value| value <= now.to_owned())
}

fn task_due_at(task: &TaskRecord) -> Option<DateTime<Utc>> {
    task.schedule
        .next_run_at
        .as_deref()
        .and_then(parse_rfc3339_utc)
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|item| item.with_timezone(&Utc))
}

const DEFAULT_PAGE_LIMIT: usize = 20;

fn apply_offset_limit<T>(items: &mut Vec<T>, offset: Option<usize>, limit: Option<usize>) {
    let offset = offset.unwrap_or(0);
    if offset >= items.len() {
        items.clear();
        return;
    }
    if offset > 0 {
        items.drain(0..offset);
    }
    if let Some(limit) = limit {
        items.truncate(limit);
    }
}

fn slice_page_items<T>(items: Vec<T>, offset: usize, limit: usize) -> Vec<T> {
    items.into_iter().skip(offset).take(limit).collect()
}

fn build_page_response<T>(
    items: Vec<T>,
    total: usize,
    limit: usize,
    offset: usize,
) -> PaginatedResponse<T> {
    let has_more = offset.saturating_add(items.len()) < total;
    PaginatedResponse {
        items,
        total,
        limit,
        offset,
        has_more,
    }
}
