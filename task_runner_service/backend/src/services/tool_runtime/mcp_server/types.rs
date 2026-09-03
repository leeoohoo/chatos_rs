// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::models::{
    AskUserPromptStatus, CancelTaskRequest, CreateTaskRequest, TaskMcpConfig, TaskMcpRequestConfig,
    TaskRunStatus, TaskScheduleConfig, TaskStatus, UpdateTaskRequest,
};

#[path = "types/common.rs"]
mod common;
#[path = "types/jsonrpc.rs"]
mod jsonrpc;
#[path = "types/prompt.rs"]
mod prompt;
#[path = "types/run.rs"]
mod run;
#[path = "types/task.rs"]
mod task;

pub(super) use self::common::{decode_args, text_result};
pub use self::jsonrpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub(super) use self::prompt::{CancelPromptArgs, ListPromptsArgs, PromptIdArgs, SubmitPromptArgs};
pub(super) use self::run::{
    GetTaskMemoryContextArgs, ListRunsArgs, ListTaskMemoryRecordsArgs, RunIdArgs, StartTaskRunArgs,
};
pub(super) use self::task::{
    reject_ai_runtime_config, BatchTaskDeleteArgs, BatchTaskRunArgs, BatchTaskStatusUpdateArgs,
    CancelTaskArgs, CreateProjectExecutionTaskItem, CreateProjectExecutionTasksArgs,
    CreateTaskArgs, CreateTaskWithPrerequisitesItem, CreateTasksWithPrerequisitesArgs,
    ListTasksArgs, SetTaskPrerequisitesArgs, TaskIdArgs, UpdateTaskArgs,
};
