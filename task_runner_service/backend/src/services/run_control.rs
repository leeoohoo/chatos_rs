use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::Mutex as AsyncMutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::models::{
    ModelConfigRecord, StartTaskRunRequest, TaskRecord, TaskRunEventRecord, TaskRunRecord,
    TaskRunStatus, TaskScheduleMode, TaskStatus, now_rfc3339,
};

use super::task_threads::ensure_task_thread_for_config;
use super::workspace_mcp::ensure_effective_task_workspace_dir;
use super::{
    RunService, RunTriggerSource, TaskScheduleModeExt, TaskStatusExt, normalized_optional,
};

mod cancellation;
mod execution;
mod start;
