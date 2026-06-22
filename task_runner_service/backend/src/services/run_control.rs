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
use super::workspace_mcp::ensure_effective_task_workspace_dir;
use super::{
    normalized_optional, save_task_if_tenant_aligned, RunService, RunTriggerSource,
    TaskScheduleModeExt, TaskStatusExt,
};

mod cancellation;
mod execution;
mod start;
