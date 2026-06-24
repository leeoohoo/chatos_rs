use std::collections::{BTreeMap, BTreeSet, HashSet};
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
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};
use tokio::sync::broadcast;
use tracing::warn;

use crate::config::{AppConfig, StoreMode};
use crate::models::{
    now_rfc3339, AskUserPromptRecord, AskUserPromptStatus, AskUserPromptTaskCountRecord,
    ExternalMcpConfigRecord, ModelConfigRecord, ModelConfigUsageRecord, PaginatedResponse,
    PromptListFilters, RemoteServerRecord, RunListFilters, RunSummaryRecord, RuntimeSettingsRecord,
    TaskListFilters, TaskPrerequisiteRecord, TaskProjectRecord, TaskRecord, TaskRunEventRecord,
    TaskRunRecord, TaskRunStatus, TaskScheduleMode, TaskStatsResponse, TaskStatus,
    TaskSummaryRecord, UserRecord,
};

mod app_models;
mod app_prompts;
mod app_runs;
mod app_tasks;
mod app_users;
mod codec;
mod in_memory;
mod mongo;
mod mongo_support;
mod sqlite;
mod sqlite_rows;
mod sqlite_support;
mod task_support;

use self::codec::{
    ask_user_prompt_status_to_str, bool_to_int, decode_json, encode_json, encode_json_option,
    encode_json_optional, task_run_status_to_str, task_status_to_str, user_role_to_str,
};
use self::mongo_support::{
    bson_string_field, bson_usize_field, build_limit_stage, build_mongo_prompt_filter,
    build_mongo_run_filter, build_mongo_task_filter, build_skip_stage,
    is_mongo_active_run_conflict, is_mongo_active_run_index_conflict, mongo_find_options,
};
use self::sqlite_rows::{
    ask_user_prompt_from_row, external_mcp_config_from_row, model_config_from_row,
    remote_server_from_row, run_summary_from_row, runtime_settings_from_row, task_from_row,
    task_project_from_row, task_run_event_from_row, task_run_from_row, task_summary_from_row,
    user_from_row,
};
use self::sqlite_support::ensure_sqlite_parent_dir;
use self::task_support::{
    apply_offset_limit, build_page_response, empty_task_stats, slice_page_items, task_due_at,
    task_due_for_scheduler, task_matches_keyword, DEFAULT_PAGE_LIMIT,
};

const ACTIVE_TASK_RUN_UNIQUE_INDEX_NAME: &str = "idx_task_runs_active_task_unique";
const TASK_RUNS_TASK_CREATED_INDEX_NAME: &str = "idx_task_runs_task_created_at";

#[derive(Default)]
struct StoreData {
    tasks: BTreeMap<String, TaskRecord>,
    task_projects: BTreeMap<String, TaskProjectRecord>,
    model_configs: BTreeMap<String, ModelConfigRecord>,
    runtime_settings: Option<RuntimeSettingsRecord>,
    remote_servers: BTreeMap<String, RemoteServerRecord>,
    external_mcp_configs: BTreeMap<String, ExternalMcpConfigRecord>,
    runs: BTreeMap<String, TaskRunRecord>,
    run_events: BTreeMap<String, Vec<TaskRunEventRecord>>,
    ask_user_prompts: BTreeMap<String, AskUserPromptRecord>,
    users: BTreeMap<String, UserRecord>,
    task_prerequisites: BTreeMap<String, BTreeSet<String>>,
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
    task_projects: Collection<TaskProjectRecord>,
    model_configs: Collection<ModelConfigRecord>,
    runtime_settings: Collection<RuntimeSettingsRecord>,
    remote_servers: Collection<RemoteServerRecord>,
    external_mcp_configs: Collection<ExternalMcpConfigRecord>,
    runs: Collection<TaskRunRecord>,
    run_events: Collection<TaskRunEventRecord>,
    ask_user_prompts: Collection<AskUserPromptRecord>,
    users: Collection<UserRecord>,
    task_prerequisites: Collection<TaskPrerequisiteRecord>,
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
}
