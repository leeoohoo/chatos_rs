use std::time::Duration;

use serde_json::json;
use tokio::time::Instant;
use tracing::warn;
use uuid::Uuid;

use crate::models::{
    now_rfc3339, StartTaskRunRequest, TaskRecord, TaskRunEventRecord, TaskRunRecord, TaskRunStatus,
    TaskStatus,
};

use super::prerequisite_context::{
    build_prerequisite_context, prerequisite_context_json, PrerequisiteTaskContext,
};
use super::status_display::TaskStatusExt;
use super::workspace_mcp::ensure_effective_task_workspace_dir;
use super::{is_terminal_run_status, normalized_optional, RunService, TaskService};

mod completion;
mod dependency_runs;
