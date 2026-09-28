// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashSet};
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::warn;

use crate::config::{AppConfig, StoreMode};
use crate::models::{
    now_rfc3339, AskUserPromptPruneResult, AskUserPromptRecord, AskUserPromptStatus,
    AskUserPromptTaskCountRecord, ChatosCallbackDeliveryState, ChatosCallbackDeliveryStatus,
    ModelConfigRecord, PaginatedResponse, PromptListFilters, RunEventPruneResult,
    RunExecutionStats, RunListFilters, RunSummaryRecord, RuntimeSettingsRecord, TaskListFilters,
    TaskPrerequisiteRecord, TaskProjectScopeFilter, TaskRecord, TaskRunAttemptRecord,
    TaskRunAttemptStatus, TaskRunEventRecord, TaskRunRecord, TaskRunStatus, TaskScheduleConfig,
    TaskScheduleMode, TaskStatsResponse, TaskStatus, TaskSummaryRecord, UserRecord,
};

mod app_models;
mod app_prompts;
mod app_runs;
mod app_tasks;
mod app_users;
pub(crate) mod cloud_agent;
mod in_memory;
mod postgres;
mod task_support;

use self::task_support::{
    apply_offset_limit, build_page_response, empty_task_stats, slice_page_items, task_due_at,
    task_due_for_scheduler, task_matches_keyword, DEFAULT_PAGE_LIMIT,
};

pub(crate) const EXECUTION_LANE_BUSY_ERROR: &str = "当前执行通道已有正在执行的运行";

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
    if run.status == TaskRunStatus::Running && run.started_at.is_some() {
        ensure_started_callback_pending(&mut run);
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
    }
    ensure_run_post_process_pending(&mut run);
    run
}

fn ensure_run_post_process_pending(run: &mut TaskRunRecord) {
    if run.requires_post_process()
        && !run.post_process_completed
        && !run.post_process_dead_lettered
        && !run.post_process_event_enqueued
    {
        run.post_process_event_pending = true;
    }
}

fn merge_run_async_progress(run: &mut TaskRunRecord, current: &TaskRunRecord) {
    merge_run_attempts(&mut run.attempts, &current.attempts);
    merge_callback_delivery(
        &mut run.chatos_started_callback_delivery,
        current.chatos_started_callback_delivery.as_ref(),
    );
    merge_callback_delivery(
        &mut run.chatos_callback_delivery,
        current.chatos_callback_delivery.as_ref(),
    );
    run.post_process_completed |= current.post_process_completed;
    run.post_process_dead_lettered |= current.post_process_dead_lettered;
    run.chatos_followup_processed |= current.chatos_followup_processed;
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
}

fn merge_callback_delivery(
    incoming: &mut Option<ChatosCallbackDeliveryState>,
    current: Option<&ChatosCallbackDeliveryState>,
) {
    let Some(current) = current else {
        return;
    };
    if incoming.as_ref().is_none_or(|incoming| {
        incoming.event == current.event && incoming.updated_at < current.updated_at
    }) {
        *incoming = Some(current.clone());
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

fn ensure_started_callback_pending(run: &mut TaskRunRecord) {
    const EVENT: &str = "task.run.started";
    if run
        .chatos_started_callback_delivery
        .as_ref()
        .is_some_and(|delivery| delivery.event == EVENT)
    {
        return;
    }
    let updated_at = run
        .started_at
        .clone()
        .unwrap_or_else(|| run.updated_at.clone());
    run.chatos_started_callback_delivery = Some(ChatosCallbackDeliveryState {
        event: EVENT.to_string(),
        status: ChatosCallbackDeliveryStatus::Pending,
        attempt_count: 0,
        next_attempt_at: Some(updated_at.clone()),
        last_error: None,
        updated_at,
    });
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
    model_configs: BTreeMap<String, ModelConfigRecord>,
    runtime_settings: Option<RuntimeSettingsRecord>,
    runs: BTreeMap<String, TaskRunRecord>,
    run_events: BTreeMap<String, Vec<TaskRunEventRecord>>,
    run_terminal_subscriptions: BTreeMap<String, RunTerminalSubscriptionRecord>,
    ask_user_prompts: BTreeMap<String, AskUserPromptRecord>,
    users: BTreeMap<String, UserRecord>,
    task_prerequisites: BTreeMap<String, BTreeSet<String>>,
    dependency_graph_revision: i64,
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
    #[cfg(test)]
    pub(crate) fn new(run_id: &str, parent_run_id: &str, worker_id: &str) -> Self {
        Self {
            id: format!("{run_id}:{parent_run_id}:{worker_id}"),
            run_id: run_id.to_string(),
            parent_run_id: parent_run_id.to_string(),
            worker_id: worker_id.to_string(),
            created_at: now_rfc3339(),
        }
    }

    pub(crate) fn cloud_agent(run_id: &str, parent_run_id: &str) -> Self {
        Self {
            id: format!("{run_id}:{parent_run_id}:cloud-agent"),
            run_id: run_id.to_string(),
            parent_run_id: parent_run_id.to_string(),
            worker_id: "cloud-agent".to_string(),
            created_at: now_rfc3339(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct InMemoryStore {
    inner: Arc<RwLock<StoreData>>,
    run_event_sender: broadcast::Sender<TaskRunEventRecord>,
    #[cfg(test)]
    run_lookup_query_counts: Arc<RunLookupQueryCounts>,
}

#[cfg(test)]
#[derive(Default)]
struct RunLookupQueryCounts {
    full_lists: AtomicUsize,
    latest: AtomicUsize,
}

#[derive(Clone)]
pub(crate) struct UserServiceModelSource {
    base_url: String,
    http_client: reqwest::Client,
    signing_secret: String,
}

#[derive(Clone)]
pub(crate) enum AppStore {
    #[cfg_attr(not(test), allow(dead_code))]
    InMemory(InMemoryStore),
    Postgres(postgres::PostgresStore),
}

impl AppStore {
    pub async fn new(config: &AppConfig) -> Result<Self, String> {
        let (run_event_sender, _) = broadcast::channel(512);
        match config.store_mode {
            StoreMode::Memory => {
                #[cfg(test)]
                {
                    Ok(Self::InMemory(InMemoryStore::new(run_event_sender)))
                }
                #[cfg(not(test))]
                {
                    let _ = run_event_sender;
                    Err(
                        "TASK_RUNNER_STORE_MODE=memory is test-only; production model configuration is read from User Service"
                            .to_string(),
                    )
                }
            }
            StoreMode::Postgres => Ok(Self::Postgres(
                postgres::PostgresStore::connect(config, run_event_sender).await?,
            )),
        }
    }

    pub(crate) async fn try_acquire_maintenance_lease(
        &self,
        lease_name: &str,
        owner_id: &str,
        lease_ttl: std::time::Duration,
    ) -> Result<bool, String> {
        if lease_name.trim().is_empty() || owner_id.trim().is_empty() {
            return Err("maintenance lease name and owner must not be empty".to_string());
        }
        let lease_ttl_seconds = i64::try_from(lease_ttl.as_secs())
            .map_err(|_| "maintenance lease TTL is too large".to_string())?;
        if lease_ttl_seconds == 0 {
            return Err("maintenance lease TTL must be at least one second".to_string());
        }
        match self {
            Self::InMemory(_) => Ok(true),
            Self::Postgres(store) => {
                sqlx::query_scalar::<_, String>(
                    "INSERT INTO task_runner_maintenance_leases(lease_name,owner_id,lease_until,updated_at) \
                     VALUES($1,$2,now()+($3::bigint * interval '1 second'),now()) \
                     ON CONFLICT(lease_name) DO UPDATE SET \
                     owner_id=EXCLUDED.owner_id,lease_until=EXCLUDED.lease_until,updated_at=now() \
                     WHERE task_runner_maintenance_leases.owner_id=EXCLUDED.owner_id \
                        OR task_runner_maintenance_leases.lease_until<=now() \
                     RETURNING owner_id",
                )
                .bind(lease_name)
                .bind(owner_id)
                .bind(lease_ttl_seconds)
                .fetch_optional(store.pool())
                .await
                .map(|claimed| claimed.as_deref() == Some(owner_id))
                .map_err(|error| error.to_string())
            }
        }
    }
}
