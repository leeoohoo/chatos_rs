// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::{info, warn};
use uuid::Uuid;

use chatos_builtin_tools::{
    TaskDraft as SharedTaskDraft, TaskManagerStore, TaskOutcomeItem as SharedTaskOutcomeItem,
    TaskStreamChunkCallback, TaskUpdatePatch as SharedTaskUpdatePatch, TASK_NOT_FOUND_ERR,
};

use crate::models::{
    now_rfc3339, TaskMcpConfig, TaskRecord, TaskScheduleConfig, TaskStatus, TaskToolOutcomeItem,
    TaskToolState,
};

use super::{
    align_task_tenant_to_owner, ensure_subtask_can_be_marked_unfinished,
    ensure_task_has_no_unfinished_subtasks, normalize_prerequisite_task_ids, normalize_strings,
    normalized_optional, normalized_optional_nested, save_task_if_tenant_aligned,
    validate_required, TaskService, TaskStatusExt,
};

mod store_adapter;
mod support;
mod task_ops;

pub(super) struct TaskRunnerTaskManagerStore {
    task_service: TaskService,
    root_task_id: Option<String>,
}

impl TaskRunnerTaskManagerStore {
    pub(super) fn new(task_service: TaskService, root_task_id: Option<String>) -> Self {
        Self {
            task_service,
            root_task_id: normalized_optional(root_task_id),
        }
    }

    pub(super) fn root_task_id<'a>(&'a self, conversation_id: &'a str) -> &'a str {
        self.root_task_id.as_deref().unwrap_or(conversation_id)
    }
}
