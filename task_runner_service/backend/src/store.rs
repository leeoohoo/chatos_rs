// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use mongodb::{
    bson::{self, doc, Bson, Document},
    options::{
        FindOneAndUpdateOptions, FindOneOptions, FindOptions, IndexOptions, ReplaceOptions,
        ReturnDocument,
    },
    Client, Collection, IndexModel,
};
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::warn;

use crate::config::{AppConfig, StoreMode};
use crate::models::{
    now_rfc3339, AskUserPromptPruneResult, AskUserPromptRecord, AskUserPromptStatus,
    AskUserPromptTaskCountRecord, ChatosCallbackDeliveryState, ChatosCallbackDeliveryStatus,
    ModelConfigRecord, ModelConfigUsageRecord, PaginatedResponse, PromptListFilters,
    RemoteServerRecord, RunEventPruneResult, RunExecutionStats, RunListFilters, RunSummaryRecord,
    RuntimeSettingsRecord, TaskListFilters, TaskPrerequisiteRecord, TaskProjectRecord, TaskRecord,
    TaskRunAttemptRecord, TaskRunAttemptStatus, TaskRunEventRecord, TaskRunRecord, TaskRunStatus,
    TaskScheduleConfig, TaskScheduleMode, TaskStatsResponse, TaskStatus, TaskSummaryRecord,
    UserRecord,
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
mod task_support;

use self::codec::ask_user_prompt_status_to_str;
use self::mongo_support::{
    bson_string_field, bson_usize_field, build_limit_stage, build_mongo_prompt_filter,
    build_mongo_run_filter, build_mongo_task_filter, build_skip_stage,
    is_mongo_active_run_conflict, is_mongo_active_run_index_conflict,
    is_mongo_execution_lane_conflict, mongo_find_options,
};
use self::task_support::{
    apply_offset_limit, build_page_response, empty_task_stats, slice_page_items, task_due_at,
    task_due_for_scheduler, task_matches_keyword, DEFAULT_PAGE_LIMIT,
};

const ACTIVE_TASK_RUN_UNIQUE_INDEX_NAME: &str = "idx_task_runs_active_task_unique";
const ACTIVE_EXECUTION_LANE_UNIQUE_INDEX_NAME: &str = "idx_task_runs_active_execution_lane_unique";
const TASK_RUNS_TASK_CREATED_INDEX_NAME: &str = "idx_task_runs_task_created_at";

fn task_run_status_is_terminal(status: TaskRunStatus) -> bool {
    matches!(
        status,
        TaskRunStatus::Succeeded
            | TaskRunStatus::Failed
            | TaskRunStatus::Cancelled
            | TaskRunStatus::Blocked
    )
}

fn prepare_run_for_claim_guarded_persist(mut run: TaskRunRecord) -> TaskRunRecord {
    run.dispatch_event_pending = run.status == TaskRunStatus::Queued && !run.dispatch_paused;
    if run.status != TaskRunStatus::Running || !run.cancel_requested || run.worker_id.is_none() {
        run.cancel_event_pending = false;
    }
    if task_run_status_is_terminal(run.status) {
        if let Some(attempt_status) = run_attempt_status_for_run_status(run.status) {
            let finished_at = run
                .finished_at
                .as_deref()
                .unwrap_or(run.updated_at.as_str())
                .to_string();
            run.finish_current_attempt(attempt_status, finished_at.as_str());
        }
        run.claim_token = None;
        run.claim_until = None;
        ensure_terminal_callback_pending(&mut run);
        ensure_run_post_process_pending(&mut run);
    }
    run
}

fn ensure_run_post_process_pending(run: &mut TaskRunRecord) {
    if task_run_status_is_terminal(run.status)
        && !run.post_process_completed
        && !run.post_process_dead_lettered
        && !run.post_process_event_enqueued
    {
        run.post_process_event_pending = true;
    }
}

fn merge_run_async_progress(run: &mut TaskRunRecord, current: &TaskRunRecord) {
    merge_run_attempts(&mut run.attempts, &current.attempts);
    run.post_process_completed |= current.post_process_completed;
    run.post_process_dead_lettered |= current.post_process_dead_lettered;
    run.memory_summary_processed |= current.memory_summary_processed;
    run.chatos_followup_processed |= current.chatos_followup_processed;
    if run.summary_job_run_id.is_none() {
        run.summary_job_run_id = current.summary_job_run_id.clone();
    }
    run.post_process_event_enqueued |= current.post_process_event_enqueued;
    run.post_process_attempt_count = run
        .post_process_attempt_count
        .max(current.post_process_attempt_count);
    if run.post_process_last_error.is_none() {
        run.post_process_last_error = current.post_process_last_error.clone();
    }
    if run.post_process_completed || run.post_process_dead_lettered {
        run.post_process_event_pending = false;
        run.post_process_event_enqueued = false;
    } else if run.post_process_event_enqueued {
        run.post_process_event_pending = false;
    } else {
        run.post_process_event_pending |= current.post_process_event_pending;
    }

    run.terminal_cleanup_completed |= current.terminal_cleanup_completed;
    run.terminal_cleanup_event_enqueued |= current.terminal_cleanup_event_enqueued;
    run.terminal_cleanup_attempt_count = run
        .terminal_cleanup_attempt_count
        .max(current.terminal_cleanup_attempt_count);
    if run.terminal_cleanup_last_error.is_none() {
        run.terminal_cleanup_last_error = current.terminal_cleanup_last_error.clone();
    }
    if run.terminal_cleanup_completed {
        run.terminal_cleanup_event_pending = false;
        run.terminal_cleanup_event_enqueued = false;
    } else if run.terminal_cleanup_event_enqueued {
        run.terminal_cleanup_event_pending = false;
    } else {
        run.terminal_cleanup_event_pending |= current.terminal_cleanup_event_pending;
    }
}

fn run_attempt_status_for_run_status(status: TaskRunStatus) -> Option<TaskRunAttemptStatus> {
    match status {
        TaskRunStatus::Succeeded => Some(TaskRunAttemptStatus::Succeeded),
        TaskRunStatus::Failed => Some(TaskRunAttemptStatus::Failed),
        TaskRunStatus::Cancelled => Some(TaskRunAttemptStatus::Cancelled),
        TaskRunStatus::Blocked => Some(TaskRunAttemptStatus::Blocked),
        TaskRunStatus::Queued | TaskRunStatus::Running => None,
    }
}

fn merge_run_attempts(
    attempts: &mut Vec<TaskRunAttemptRecord>,
    current_attempts: &[TaskRunAttemptRecord],
) {
    for current in current_attempts {
        let Some(incoming) = attempts
            .iter_mut()
            .find(|attempt| attempt.attempt_id == current.attempt_id)
        else {
            attempts.push(current.clone());
            continue;
        };
        if current.status != TaskRunAttemptStatus::Running {
            incoming.status = current.status;
            incoming.finished_at = current.finished_at.clone();
        }
        if incoming.recovery_reason.is_none() {
            incoming.recovery_reason = current.recovery_reason.clone();
        }
        if incoming.sandbox_id.is_none() {
            incoming.sandbox_id = current.sandbox_id.clone();
        }
        if incoming.lease_id.is_none() {
            incoming.lease_id = current.lease_id.clone();
        }
        if incoming.model_response_id.is_none() {
            incoming.model_response_id = current.model_response_id.clone();
        }
    }
    attempts.sort_by_key(|attempt| attempt.sequence);
}

fn terminal_callback_event_for_status(status: TaskRunStatus) -> Option<&'static str> {
    match status {
        TaskRunStatus::Succeeded => Some("task.completed"),
        TaskRunStatus::Failed => Some("task.failed"),
        TaskRunStatus::Cancelled => Some("task.cancelled"),
        TaskRunStatus::Blocked => Some("task.blocked"),
        TaskRunStatus::Queued | TaskRunStatus::Running => None,
    }
}

fn ensure_terminal_callback_pending(run: &mut TaskRunRecord) {
    let Some(event) = terminal_callback_event_for_status(run.status) else {
        return;
    };
    if run
        .chatos_callback_delivery
        .as_ref()
        .is_some_and(|delivery| delivery.event == event)
    {
        return;
    }
    let updated_at = run.updated_at.clone();
    run.chatos_callback_delivery = Some(ChatosCallbackDeliveryState {
        event: event.to_string(),
        status: ChatosCallbackDeliveryStatus::Pending,
        attempt_count: 0,
        next_attempt_at: Some(updated_at.clone()),
        last_error: None,
        updated_at,
    });
}

fn lost_run_claim_error(run_id: &str) -> String {
    format!("run claim lost before persisting run {run_id}")
}

#[derive(Default)]
struct StoreData {
    tasks: BTreeMap<String, TaskRecord>,
    task_projects: BTreeMap<String, TaskProjectRecord>,
    model_configs: BTreeMap<String, ModelConfigRecord>,
    runtime_settings: Option<RuntimeSettingsRecord>,
    remote_servers: BTreeMap<String, RemoteServerRecord>,
    runs: BTreeMap<String, TaskRunRecord>,
    run_events: BTreeMap<String, Vec<TaskRunEventRecord>>,
    run_terminal_subscriptions: BTreeMap<String, RunTerminalSubscriptionRecord>,
    ask_user_prompts: BTreeMap<String, AskUserPromptRecord>,
    users: BTreeMap<String, UserRecord>,
    task_prerequisites: BTreeMap<String, BTreeSet<String>>,
    cancel_requested_runs: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RunTerminalSubscriptionRecord {
    pub id: String,
    pub run_id: String,
    pub parent_run_id: String,
    pub worker_id: String,
    pub created_at: String,
}

impl RunTerminalSubscriptionRecord {
    pub(crate) fn new(run_id: &str, parent_run_id: &str, worker_id: &str) -> Self {
        Self {
            id: format!("{run_id}:{parent_run_id}:{worker_id}"),
            run_id: run_id.to_string(),
            parent_run_id: parent_run_id.to_string(),
            worker_id: worker_id.to_string(),
            created_at: now_rfc3339(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct InMemoryStore {
    inner: Arc<RwLock<StoreData>>,
    run_event_sender: broadcast::Sender<TaskRunEventRecord>,
}

#[derive(Clone)]
pub(crate) struct MongoStore {
    tasks: Collection<TaskRecord>,
    task_projects: Collection<TaskProjectRecord>,
    model_configs: Collection<ModelConfigRecord>,
    runtime_settings: Collection<RuntimeSettingsRecord>,
    remote_servers: Collection<RemoteServerRecord>,
    runs: Collection<TaskRunRecord>,
    run_events: Collection<TaskRunEventRecord>,
    run_terminal_subscriptions: Collection<RunTerminalSubscriptionRecord>,
    ask_user_prompts: Collection<AskUserPromptRecord>,
    users: Collection<UserRecord>,
    task_prerequisites: Collection<TaskPrerequisiteRecord>,
    cancel_requested_runs: Arc<RwLock<HashSet<String>>>,
    run_event_sender: broadcast::Sender<TaskRunEventRecord>,
}

#[derive(Clone)]
pub(crate) enum AppStore {
    InMemory(InMemoryStore),
    Mongo(MongoStore),
}

impl AppStore {
    pub async fn new(config: &AppConfig) -> Result<Self, String> {
        let (run_event_sender, _) = broadcast::channel(512);
        match config.store_mode {
            StoreMode::Memory => Ok(Self::InMemory(InMemoryStore::new(run_event_sender))),
            StoreMode::Mongo => Ok(Self::Mongo(
                MongoStore::connect(&config.database_url, run_event_sender).await?,
            )),
        }
    }
}
