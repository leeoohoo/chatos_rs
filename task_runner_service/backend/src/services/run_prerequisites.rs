use std::time::Duration;

use serde_json::json;
use tokio::time::Instant;
use tracing::warn;
use uuid::Uuid;

use crate::models::{
    StartTaskRunRequest, TaskRecord, TaskRunEventRecord, TaskRunRecord, TaskRunStatus, TaskStatus,
    now_rfc3339,
};

use super::prerequisite_context::{
    PrerequisiteTaskContext, build_prerequisite_context, prerequisite_context_json,
};
use super::status_display::TaskStatusExt;
use super::workspace_mcp::ensure_effective_task_workspace_dir;
use super::{RunService, TaskService, is_terminal_run_status, normalized_optional};

mod completion;
mod dependency_runs;
