use async_trait::async_trait;
use serde_json::{Value, json};
use tracing::info;
use uuid::Uuid;

use chatos_builtin_tools::{
    TASK_NOT_FOUND_ERR, TaskDraft as SharedTaskDraft, TaskManagerStore,
    TaskOutcomeItem as SharedTaskOutcomeItem, TaskStreamChunkCallback,
    TaskUpdatePatch as SharedTaskUpdatePatch,
};

use crate::models::{
    TaskRecord, TaskScheduleConfig, TaskStatus, TaskToolOutcomeItem, TaskToolState, now_rfc3339,
};

use super::{
    TaskService, TaskStatusExt, normalize_strings, normalized_optional, normalized_optional_nested,
    validate_required,
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
