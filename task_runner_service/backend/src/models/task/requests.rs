// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateTaskPluginHint {
    pub plugin_key: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskMcpRequestConfig {
    #[serde(default)]
    pub requires_execution: Option<bool>,
    #[serde(default)]
    pub workspace_changes_required: Option<bool>,
    #[serde(default)]
    pub enabled_builtin_kinds: Vec<String>,
    #[serde(default)]
    pub external_mcp_config_ids: Vec<String>,
}

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
    #[serde(default)]
    pub plugin_config: TaskPluginConfig,
    pub mcp_config: Option<TaskMcpRequestConfig>,
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
    pub remote_connection_id: Option<String>,
    pub workspace_dir: Option<String>,
    pub builtin_prompt_locale: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskProjectScopeFilter {
    UserConversation,
    Project,
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
    pub project_id: Option<String>,
    #[serde(default)]
    pub task_profile: Option<String>,
    pub schedule: Option<TaskScheduleConfig>,
    #[serde(default)]
    pub plugin_config: Option<TaskPluginConfig>,
    pub mcp_config: Option<TaskMcpRequestConfig>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMcpResolutionResponse {
    pub requested_builtin_kinds: Vec<String>,
    pub required_builtin_kinds: Vec<TaskMcpRequiredBuiltinCapability>,
    pub hosted_builtin_routes: Vec<TaskMcpHostedBuiltinRoute>,
    pub server_local_builtin_kinds: Vec<String>,
    pub external_mcp_config_ids: Vec<String>,
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
    pub project_scope: Option<TaskProjectScopeFilter>,
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

#[cfg(test)]
mod task_mcp_request_config_tests {
    use super::*;

    #[test]
    fn request_accepts_execution_intent_and_agent_mcp_selection() {
        let request = serde_json::from_value::<TaskMcpRequestConfig>(serde_json::json!({
            "requires_execution": false,
            "enabled_builtin_kinds": ["CodeMaintainerRead"],
            "external_mcp_config_ids": ["postgres-mcp"]
        }))
        .expect("task MCP selection");

        assert_eq!(request.requires_execution, Some(false));
        assert_eq!(
            request.enabled_builtin_kinds,
            vec!["CodeMaintainerRead".to_string()]
        );
        assert_eq!(
            request.external_mcp_config_ids,
            vec!["postgres-mcp".to_string()]
        );
    }

    #[test]
    fn request_rejects_program_managed_mcp_fields() {
        for field in [
            "enabled",
            "init_mode",
            "builtin_prompt_mode",
            "builtin_prompt_locale",
            "workspace_dir",
            "execution_service_id",
            "default_remote_server_id",
            "selected_skill_ids",
            "skill_policy_revision",
            "ephemeral_http_servers",
        ] {
            let value = serde_json::Value::Object(
                [(field.to_string(), serde_json::Value::Null)]
                    .into_iter()
                    .collect(),
            );
            let error = serde_json::from_value::<TaskMcpRequestConfig>(value)
                .expect_err("program-managed MCP field must be rejected");

            assert!(
                error.to_string().contains("unknown field"),
                "unexpected error for {field}: {error}"
            );
        }
    }
}
