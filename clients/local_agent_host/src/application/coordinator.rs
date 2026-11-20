// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    HostRequestHandler, LocalAgentScheduler, LocalAgentSchedulerError, LocalHookCoordinator,
    LocalMemorySyncError, LocalMemorySyncWorker, LocalToolScheduler, LocalToolSchedulerError,
    MemorySyncTick, SchedulerTick, ToolSchedulerTick,
};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    validate_identifier, HostCommand, HostError, HostRequestEnvelope, HostResponseEnvelope,
    HostResult, LocalAgentHostSchedulerSnapshot, LocalAgentResourcePressureLevel,
    LocalAgentRunRecord, LocalAgentRunStatus, LocalAgentToolExecutionRoute, LocalAgentToolOutcome,
};
use chatos_local_agent_runtime::{LocalAgentRuntime, LocalAgentRuntimeError};
use futures::future::join_all;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::sync::{watch, Notify, RwLock};

use super::hook_coordinator::LocalHookPromptDecision;
use super::work_activity::HostWorkActivity;

const MIN_CONCURRENT_WORK_ITEMS: usize = 1;
const MAX_CONCURRENT_WORK_ITEMS: usize = 16;
const RUN_IDLE_RECOVERY_INTERVAL: Duration = Duration::from_secs(30);
const HOOK_SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const HOOK_SESSION_REAPER_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Error)]
pub enum LocalAgentCoordinatorError {
    #[error(transparent)]
    ModelScheduler(#[from] LocalAgentSchedulerError),
    #[error(transparent)]
    ToolScheduler(#[from] LocalToolSchedulerError),
    #[error(transparent)]
    Runtime(#[from] LocalAgentRuntimeError),
    #[error(transparent)]
    MemorySync(#[from] LocalMemorySyncError),
    #[error("system clock is before the Unix epoch")]
    Clock,
}

pub struct LocalAgentHostCoordinator {
    runtime: Arc<LocalAgentRuntime>,
    owner_user_id: String,
    model_scheduler: Option<LocalAgentScheduler>,
    tool_scheduler: Option<LocalToolScheduler>,
    memory_sync_worker: Option<LocalMemorySyncWorker>,
    hook_coordinator: Option<Arc<LocalHookCoordinator>>,
    wakeup: Arc<Notify>,
    activity: watch::Sender<u64>,
    work_activity: HostWorkActivity,
    admission_activity: HostWorkActivity,
    memory_work_activity: HostWorkActivity,
    reserved_ipc_tools: Vec<String>,
    maximum_concurrent_work_items: usize,
    target_concurrent_work_items: watch::Sender<usize>,
    effective_maximum_concurrent_work_items: watch::Sender<usize>,
    pressure_level: watch::Sender<LocalAgentResourcePressureLevel>,
    accepting_new_work: watch::Sender<bool>,
    admission_barrier: Arc<RwLock<()>>,
}

include!("coordinator/setup.rs");
include!("coordinator/scheduler.rs");
include!("coordinator/runtime.rs");
include!("coordinator/request_handler.rs");

#[cfg(test)]
#[path = "coordinator_tests.rs"]
mod coordinator_tests;
