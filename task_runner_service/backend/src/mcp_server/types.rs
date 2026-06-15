use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::models::{
    CreateRemoteServerRequest, CreateTaskRequest, TaskMcpConfig, TaskRecord, TaskRunEventRecord,
    TaskRunRecord, TaskRunStatus, TaskScheduleConfig, TaskStatus, UiPromptRecord, UiPromptStatus,
    UpdateModelConfigRequest, UpdateTaskRequest,
};

use super::support::normalize_mcp_builtin_kind_names;

mod common;
mod jsonrpc;
mod model;
mod prompt;
mod run;
mod task;

pub(super) use self::common::{decode_args, decode_remote_server_config_header, text_result};
pub use self::jsonrpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub(super) use self::model::{ModelConfigIdArgs, TestModelConfigArgs, UpdateModelConfigArgs};
pub(super) use self::prompt::{CancelPromptArgs, ListPromptsArgs, PromptIdArgs, SubmitPromptArgs};
pub(super) use self::run::{
    GetTaskMemoryContextArgs, ListRunsArgs, ListTaskMemoryRecordsArgs, RunIdArgs, StartTaskRunArgs,
};
pub(super) use self::task::{
    BatchTaskDeleteArgs, BatchTaskRunArgs, BatchTaskStatusUpdateArgs, CreateTaskArgs,
    CreateTaskWithPrerequisitesItem, CreateTasksWithPrerequisitesArgs, ListTasksArgs,
    SetTaskPrerequisitesArgs, TaskIdArgs, UpdateTaskArgs,
};

#[allow(dead_code)]
pub(super) fn _assert_types(
    _task: TaskRecord,
    _run: TaskRunRecord,
    _event: TaskRunEventRecord,
    _prompt: UiPromptRecord,
) {
}
