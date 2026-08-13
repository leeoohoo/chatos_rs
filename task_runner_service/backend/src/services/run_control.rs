// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use tracing::{info, warn};
use uuid::Uuid;

use crate::models::{
    now_rfc3339, StartTaskRunRequest, TaskRecord, TaskRunEventRecord, TaskRunRecord, TaskRunStatus,
    TaskScheduleMode, TaskStatus,
};

use super::task_threads::ensure_task_thread_for_config;
use super::workspace_mcp::ensure_effective_task_workspace_dir;
use super::{
    normalized_optional, save_task_if_tenant_aligned, KeyedAsyncLockHandle, RunService,
    RunTriggerSource, TaskScheduleModeExt, TaskStatusExt,
};

mod cancellation;
mod execution;
pub(crate) use execution::cloud_agent_profile;
mod start;
