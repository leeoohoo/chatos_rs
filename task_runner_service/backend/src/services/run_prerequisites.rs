// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use crate::models::{
    StartTaskRunRequest, TaskRecord, TaskRunEventRecord, TaskRunRecord, TaskRunStatus, TaskStatus,
};

use super::prerequisite_context::{build_prerequisite_context, PrerequisiteTaskContext};
use super::status_display::TaskStatusExt;
use super::{is_terminal_run_status, save_task_if_tenant_aligned, RunService, TaskService};

mod dependency_runs;
