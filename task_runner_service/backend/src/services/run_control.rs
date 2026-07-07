// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::Mutex as AsyncMutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::models::{
    now_rfc3339, ModelConfigRecord, StartTaskRunRequest, TaskRecord, TaskRunEventRecord,
    TaskRunRecord, TaskRunStatus, TaskScheduleMode, TaskStatus,
};

use super::task_threads::ensure_task_thread_for_config;
use super::workspace_mcp::{
    apply_local_connector_routing_to_task, ensure_effective_task_workspace_dir,
    resolve_project_root_for_task,
};
use super::{
    normalized_optional, save_task_if_tenant_aligned, RunService, RunTriggerSource,
    TaskScheduleModeExt, TaskStatusExt,
};

mod cancellation;
mod execution;
mod start;
