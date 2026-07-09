// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub description: Option<String>,
    pub objective: String,
    pub input_payload: Option<Value>,
    pub status: Option<TaskStatus>,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub default_model_config_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub task_profile: Option<String>,
    pub tenant_id: Option<String>,
    pub subject_id: Option<String>,
    pub schedule: Option<TaskScheduleConfig>,
    pub mcp_config: Option<TaskMcpConfig>,
    #[serde(default)]
    pub prerequisite_task_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskSourceContext {
    pub project_id: Option<String>,
    pub parent_task_id: Option<String>,
    pub source_run_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub workspace_dir: Option<String>,
    pub remote_server_config: Option<CreateRemoteServerRequest>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub objective: Option<String>,
    pub input_payload: Option<Value>,
    pub status: Option<TaskStatus>,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub default_model_config_id: Option<String>,
    #[serde(default)]
    pub task_profile: Option<String>,
    pub schedule: Option<TaskScheduleConfig>,
    pub mcp_config: Option<TaskMcpConfig>,
    #[serde(default)]
    pub prerequisite_task_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetTaskPrerequisitesRequest {
    #[serde(default)]
    pub prerequisite_task_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CancelTaskRequest {
    pub reason: String,
    #[serde(default)]
    pub replacement_task_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTaskResponse {
    pub cancelled: bool,
    pub task_id: String,
    pub status: TaskStatus,
    pub reason: String,
    #[serde(default)]
    pub active_run_ids: Vec<String>,
    #[serde(default)]
    pub cascade_cancelled_task_ids: Vec<String>,
    pub callback_event: String,
    pub task: TaskRecord,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecordTaskProcessRequest {
    #[serde(default)]
    pub operation: TaskProcessLogOperation,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub heading: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateTaskMcpRequest {
    pub enabled: Option<bool>,
    pub init_mode: Option<TaskMcpInitMode>,
    pub builtin_prompt_mode: Option<TaskBuiltinMcpPromptMode>,
    pub builtin_prompt_locale: Option<String>,
    pub enabled_builtin_kinds: Option<Vec<String>>,
    pub workspace_dir: Option<String>,
    pub default_remote_server_id: Option<String>,
    pub external_mcp_config_ids: Option<Vec<String>>,
    pub skill_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMcpResolutionResponse {
    pub requested_builtin_kinds: Vec<String>,
    pub required_builtin_kinds: Vec<TaskMcpRequiredBuiltinCapability>,
    pub hosted_builtin_routes: Vec<TaskMcpHostedBuiltinRoute>,
    pub server_local_builtin_kinds: Vec<String>,
    pub external_mcp_config_ids: Vec<String>,
    pub skill_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMcpRequiredBuiltinCapability {
    pub kind: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMcpHostedBuiltinRoute {
    pub host: String,
    pub server_name: String,
    pub builtin_kinds: Vec<String>,
    pub public_server_names: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskListFilters {
    pub status: Option<TaskStatus>,
    pub keyword: Option<String>,
    pub tag: Option<String>,
    pub model_config_id: Option<String>,
    pub project_id: Option<String>,
    pub creator_user_id: Option<String>,
    pub scheduled_only: Option<bool>,
    pub parent_task_id: Option<String>,
    pub include_subtasks: Option<bool>,
    pub source_run_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source_user_message_ids: Vec<String>,
    pub source_turn_ids: Vec<String>,
    pub task_profile: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchTaskStatusUpdateRequest {
    pub task_ids: Vec<String>,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTaskDeleteRequest {
    pub task_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchTaskRunRequest {
    pub task_ids: Vec<String>,
    pub model_config_id: Option<String>,
    pub prompt_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTaskOperationItem {
    pub task_id: String,
    pub ok: bool,
    pub message: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTaskOperationResponse {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub results: Vec<BatchTaskOperationItem>,
}
