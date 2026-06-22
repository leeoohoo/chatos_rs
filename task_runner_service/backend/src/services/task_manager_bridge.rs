use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::info;
use uuid::Uuid;

use chatos_builtin_tools::{
    TaskDraft as SharedTaskDraft, TaskManagerStore, TaskOutcomeItem as SharedTaskOutcomeItem,
    TaskStreamChunkCallback, TaskUpdatePatch as SharedTaskUpdatePatch, TASK_NOT_FOUND_ERR,
};

use crate::models::{
    now_rfc3339, TaskRecord, TaskScheduleConfig, TaskStatus, TaskToolOutcomeItem, TaskToolState,
};

use super::{
    align_task_tenant_to_owner, normalize_strings, normalized_optional, normalized_optional_nested,
    save_task_if_tenant_aligned, validate_required, TaskService, TaskStatusExt,
};

mod store_adapter;
mod support;
mod task_ops;

pub(super) struct TaskRunnerTaskManagerStore {
    task_service: TaskService,
}

impl TaskRunnerTaskManagerStore {
    pub(super) fn new(task_service: TaskService) -> Self {
        Self { task_service }
    }
}
