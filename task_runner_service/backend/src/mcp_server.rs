use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{
    CreateRemoteServerRequest, CreateTaskRequest, TaskMcpConfig, TaskRecord,
    TaskRunEventRecord, TaskRunRecord, TaskRunStatus, TaskScheduleConfig, TaskSourceContext,
    TaskStatus, UiPromptRecord, UiPromptStatus, UpdateModelConfigRequest, UpdateTaskRequest,
};
use crate::services::{McpCatalogService, ModelConfigService, RunService, TaskService};
use crate::ui_prompt_service::UiPromptService;

mod chatos_async_planner;
mod access;
mod entrypoints;
mod model_tools;
mod prompt_tools;
mod prerequisite_creation;
mod run_tools;
mod support;
mod task_tools;

use self::support::{
    agent_tool_allowed_for_profile, normalize_mcp_builtin_kind_names,
};

const TASK_RUNNER_MCP_SERVER_NAME: &str = "task_runner_service";
const TASK_RUNNER_MCP_ENDPOINT_PATH: &str = "/mcp";
const TASK_RUNNER_MCP_STDIO_COMMAND: &str = "cargo";
const TASK_RUNNER_MCP_STDIO_ARGS: &[&str] = &[
    "run",
    "-p",
    "task_runner_service_backend",
    "--bin",
    "task_runner_mcp_stdio",
];
const CHATOS_ASYNC_PLANNER_TOOL_PROFILE: &str = "chatos_async_planner";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpToolProfile {
    Default,
    ChatosAsyncPlanner,
}

#[derive(Debug, Clone, Default)]
pub struct McpRequestContext {
    pub source_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub source_user_message_id: Option<String>,
    pub workspace_dir: Option<String>,
    pub remote_server_config: Option<String>,
    pub tool_profile: Option<String>,
}

impl McpRequestContext {
    fn task_source_context(&self) -> Result<Option<TaskSourceContext>, String> {
        if self.source_session_id.is_none()
            && self.source_turn_id.is_none()
            && self.source_user_message_id.is_none()
            && self.workspace_dir.is_none()
            && self.remote_server_config.is_none()
        {
            return Ok(None);
        }
        let remote_server_config = self
            .remote_server_config
            .as_deref()
            .map(decode_remote_server_config_header)
            .transpose()?;
        Ok(Some(TaskSourceContext {
            source_session_id: self.source_session_id.clone(),
            source_turn_id: self.source_turn_id.clone(),
            source_user_message_id: self.source_user_message_id.clone(),
            workspace_dir: self.workspace_dir.clone(),
            remote_server_config,
        }))
    }

    fn tool_profile(&self) -> McpToolProfile {
        if self.tool_profile.as_deref().is_some_and(|value| {
            value
                .trim()
                .eq_ignore_ascii_case(CHATOS_ASYNC_PLANNER_TOOL_PROFILE)
        }) {
            McpToolProfile::ChatosAsyncPlanner
        } else {
            McpToolProfile::Default
        }
    }
}

#[derive(Clone)]
pub struct TaskRunnerMcpService {
    task_service: TaskService,
    model_config_service: ModelConfigService,
    run_service: RunService,
    ui_prompt_service: UiPromptService,
    mcp_catalog_service: McpCatalogService,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Default, Deserialize)]
struct ListTasksArgs {
    #[serde(default)]
    status: Option<TaskStatus>,
    #[serde(default)]
    keyword: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    model_config_id: Option<String>,
    #[serde(default)]
    scheduled_only: Option<bool>,
    #[serde(default)]
    parent_task_id: Option<String>,
    #[serde(default)]
    source_run_id: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct TaskIdArgs {
    task_id: String,
}

#[derive(Debug, Deserialize)]
struct CreateTaskArgs {
    title: String,
    #[serde(default)]
    description: Option<String>,
    objective: String,
    #[serde(default)]
    input_payload: Option<Value>,
    #[serde(default)]
    priority: Option<i32>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    default_model_config_id: Option<String>,
    #[serde(default)]
    schedule: Option<TaskScheduleConfig>,
    #[serde(default)]
    enabled_builtin_kinds: Option<Vec<String>>,
    #[serde(default)]
    prerequisite_task_ids: Option<Vec<String>>,
    #[serde(default)]
    mcp_config: Option<TaskMcpConfig>,
}

impl CreateTaskArgs {
    fn into_request(self) -> Result<CreateTaskRequest, String> {
        let mut mcp_config = self.mcp_config;
        if let Some(enabled_builtin_kinds) = self.enabled_builtin_kinds {
            let normalized = normalize_mcp_builtin_kind_names(enabled_builtin_kinds)?;
            let config = mcp_config.get_or_insert_with(TaskMcpConfig::default);
            config.enabled = true;
            config.enabled_builtin_kinds = normalized;
        }
        Ok(CreateTaskRequest {
            title: self.title,
            description: self.description,
            objective: self.objective,
            input_payload: self.input_payload,
            status: None,
            priority: self.priority,
            tags: self.tags,
            default_model_config_id: self.default_model_config_id,
            tenant_id: None,
            subject_id: None,
            schedule: self.schedule,
            mcp_config,
            prerequisite_task_ids: self.prerequisite_task_ids,
        })
    }
}

#[derive(Debug, Deserialize)]
struct UpdateTaskArgs {
    task_id: String,
    #[serde(default)]
    patch: UpdateTaskRequest,
}

#[derive(Debug, Deserialize)]
struct SetTaskPrerequisitesArgs {
    task_id: String,
    #[serde(default)]
    prerequisite_task_ids: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct CreateTasksWithPrerequisitesArgs {
    #[serde(default)]
    tasks: Vec<CreateTaskWithPrerequisitesItem>,
}

#[derive(Debug, Deserialize)]
struct CreateTaskWithPrerequisitesItem {
    client_ref: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
    objective: String,
    #[serde(default)]
    input_payload: Option<Value>,
    #[serde(default)]
    priority: Option<i32>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    default_model_config_id: Option<String>,
    #[serde(default)]
    schedule: Option<TaskScheduleConfig>,
    #[serde(default)]
    enabled_builtin_kinds: Option<Vec<String>>,
    #[serde(default)]
    prerequisite_refs: Vec<String>,
    #[serde(default)]
    prerequisite_task_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ModelConfigIdArgs {
    model_config_id: String,
}

#[derive(Debug, Deserialize)]
struct UpdateModelConfigArgs {
    model_config_id: String,
    #[serde(default)]
    patch: UpdateModelConfigRequest,
}

#[derive(Debug, Deserialize)]
struct TestModelConfigArgs {
    model_config_id: String,
    #[serde(default)]
    prompt: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ListRunsArgs {
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    status: Option<TaskRunStatus>,
    #[serde(default)]
    model_config_id: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct RunIdArgs {
    run_id: String,
}

#[derive(Debug, Deserialize)]
struct StartTaskRunArgs {
    task_id: String,
    #[serde(default)]
    model_config_id: Option<String>,
    #[serde(default)]
    prompt_override: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BatchTaskStatusUpdateArgs {
    task_ids: Vec<String>,
    status: TaskStatus,
}

#[derive(Debug, Deserialize)]
struct BatchTaskDeleteArgs {
    task_ids: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct BatchTaskRunArgs {
    task_ids: Vec<String>,
    #[serde(default)]
    model_config_id: Option<String>,
    #[serde(default)]
    prompt_override: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct GetTaskMemoryContextArgs {
    task_id: String,
    #[serde(default)]
    include_recent_records: Option<bool>,
    #[serde(default)]
    include_thread_summary: Option<bool>,
    #[serde(default)]
    include_subject_memory: Option<bool>,
    #[serde(default)]
    recent_record_limit: Option<usize>,
    #[serde(default)]
    summary_limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct ListTaskMemoryRecordsArgs {
    task_id: String,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    record_type: Option<String>,
    #[serde(default)]
    summary_status: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    offset: Option<i64>,
    #[serde(default)]
    order: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PromptIdArgs {
    prompt_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct ListPromptsArgs {
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    status: Option<UiPromptStatus>,
}

#[derive(Debug, Deserialize)]
struct SubmitPromptArgs {
    prompt_id: String,
    #[serde(default)]
    values: Option<Value>,
    #[serde(default)]
    selection: Option<Value>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CancelPromptArgs {
    prompt_id: String,
    #[serde(default)]
    reason: Option<String>,
}

impl TaskRunnerMcpService {
    pub(crate) fn new(
        task_service: TaskService,
        model_config_service: ModelConfigService,
        run_service: RunService,
        ui_prompt_service: UiPromptService,
        mcp_catalog_service: McpCatalogService,
    ) -> Self {
        Self {
            task_service,
            model_config_service,
            run_service,
            ui_prompt_service,
            mcp_catalog_service,
        }
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Value, String> {
        if !current_user.is_admin()
            && !agent_tool_allowed_for_profile(name, request_context.tool_profile())
        {
            return Err("当前 agent 无权调用该任务系统工具".to_string());
        }
        match name {
            "list_tasks"
            | "get_task"
            | "get_task_stats"
            | "create_task"
            | "list_mcp_builtin_catalog"
            | "create_tasks_with_prerequisites"
            | "update_task"
            | "set_task_prerequisites"
            | "wait_for_task_completion"
            | "get_task_dependency_graph"
            | "delete_task"
            | "batch_update_task_status"
            | "batch_delete_tasks" => {
                self.call_task_tool(name, args, current_user, request_context)
                    .await
            }
            "list_model_configs"
            | "get_model_config"
            | "create_model_config"
            | "update_model_config"
            | "delete_model_config"
            | "test_model_config" => self.call_model_tool(name, args, current_user).await,
            "list_runs"
            | "get_run"
            | "start_task_run"
            | "batch_start_task_runs"
            | "get_task_memory_context"
            | "list_task_memory_records"
            | "summarize_task_memory"
            | "cancel_run"
            | "retry_run"
            | "list_run_events" => self.call_run_tool(name, args, current_user).await,
            "list_prompts" | "get_prompt" | "submit_prompt" | "cancel_prompt" => {
                self.call_prompt_tool(name, args, current_user).await
            }
            other => Err(format!("tool not found: {other}")),
        }
    }

}

fn decode_args<T>(args: Value) -> Result<T, String>
where
    T: DeserializeOwned,
{
    serde_json::from_value(args).map_err(|err| err.to_string())
}

fn decode_remote_server_config_header(value: &str) -> Result<CreateRemoteServerRequest, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("远程服务器透传配置为空".to_string());
    }
    let json_text = if trimmed.starts_with('{') {
        trimmed.to_string()
    } else {
        let bytes = URL_SAFE_NO_PAD
            .decode(trimmed.as_bytes())
            .map_err(|err| format!("远程服务器透传配置不是有效 base64: {err}"))?;
        String::from_utf8(bytes).map_err(|err| format!("远程服务器透传配置不是 UTF-8: {err}"))?
    };
    serde_json::from_str::<CreateRemoteServerRequest>(&json_text)
        .map_err(|err| format!("远程服务器透传配置不是有效 JSON: {err}"))
}

fn text_result(payload: Value) -> Value {
    let text = if payload.is_string() {
        payload.as_str().unwrap_or("").to_string()
    } else {
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
    };
    let mut out = json!({
        "content": [
            { "type": "text", "text": text }
        ]
    });
    if !payload.is_string() && !payload.is_null() {
        out["_structured_result"] = payload;
    }
    out
}

#[allow(dead_code)]
fn _assert_types(
    _task: TaskRecord,
    _run: TaskRunRecord,
    _event: TaskRunEventRecord,
    _prompt: UiPromptRecord,
) {
}

#[cfg(test)]
mod tests {
    use super::chatos_async_planner;
    use super::support::{agent_tool_allowed, create_task_schema, task_mcp_config_schema};
    use crate::models::{
        CreateTaskRequest, TaskMcpConfig, TaskScheduleMode, TaskStatus, UpdateTaskRequest,
    };

    #[test]
    fn create_task_schema_hides_memory_scope_fields() {
        let schema = create_task_schema();
        let properties = schema
            .get("properties")
            .and_then(|value| value.as_object())
            .expect("object properties");

        assert!(!properties.contains_key("tenant_id"));
        assert!(!properties.contains_key("subject_id"));
        assert!(!properties.contains_key("status"));
        assert!(!properties.contains_key("mcp_config"));
        assert!(properties.contains_key("enabled_builtin_kinds"));

        let kind_enum = properties
            .get("enabled_builtin_kinds")
            .and_then(|value| value.get("items"))
            .and_then(|value| value.get("enum"))
            .and_then(|value| value.as_array())
            .expect("enabled_builtin_kinds enum");
        assert!(kind_enum
            .iter()
            .any(|value| value.as_str() == Some("WebTools")));
        assert!(kind_enum
            .iter()
            .any(|value| value.as_str() == Some("RemoteConnectionController")));
    }

    #[test]
    fn task_mcp_config_schema_hides_host_passthrough_fields() {
        let schema = task_mcp_config_schema();
        let properties = schema
            .get("properties")
            .and_then(|value| value.as_object())
            .expect("object properties");

        assert!(!properties.contains_key("workspace_dir"));
        assert!(!properties.contains_key("default_remote_server_id"));
        assert!(properties.contains_key("enabled_builtin_kinds"));
    }

    #[test]
    fn external_mcp_tools_hide_internal_process_recorder() {
        assert!(!agent_tool_allowed("record_task_process"));
    }

    #[test]
    fn async_planner_profile_exposes_only_planning_tools() {
        assert!(chatos_async_planner::planner_agent_tool_allowed("list_tasks"));
        assert!(chatos_async_planner::planner_agent_tool_allowed("get_task"));
        assert!(chatos_async_planner::planner_agent_tool_allowed("get_task_stats"));
        assert!(chatos_async_planner::planner_agent_tool_allowed("create_task"));
        assert!(chatos_async_planner::planner_agent_tool_allowed(
            "create_tasks_with_prerequisites"
        ));
        assert!(chatos_async_planner::planner_agent_tool_allowed("list_mcp_builtin_catalog"));
        assert!(chatos_async_planner::planner_agent_tool_allowed("update_task"));
        assert!(chatos_async_planner::planner_agent_tool_allowed("set_task_prerequisites"));
        assert!(chatos_async_planner::planner_agent_tool_allowed("get_task_dependency_graph"));
        assert!(!chatos_async_planner::planner_agent_tool_allowed("start_task_run"));
        assert!(!chatos_async_planner::planner_agent_tool_allowed("list_runs"));
        assert!(!chatos_async_planner::planner_agent_tool_allowed("get_run"));
        assert!(!chatos_async_planner::planner_agent_tool_allowed("list_run_events"));
    }

    #[test]
    fn async_planner_update_task_cannot_change_status() {
        let patch = UpdateTaskRequest {
            status: Some(TaskStatus::Ready),
            ..UpdateTaskRequest::default()
        };
        assert!(chatos_async_planner::planner_update_task_request(patch).is_err());

        let patch = UpdateTaskRequest {
            objective: Some("updated objective".to_string()),
            ..UpdateTaskRequest::default()
        };
        assert!(chatos_async_planner::planner_update_task_request(patch).is_ok());
    }

    #[test]
    fn async_planner_tasks_require_model_and_builtin_kinds() {
        let missing_model = CreateTaskRequest {
            title: "task".to_string(),
            description: None,
            objective: "objective".to_string(),
            input_payload: None,
            status: None,
            priority: None,
            tags: None,
            default_model_config_id: None,
            tenant_id: None,
            subject_id: None,
            schedule: None,
            mcp_config: Some(TaskMcpConfig {
                enabled_builtin_kinds: vec!["CodeMaintainerRead".to_string()],
                ..TaskMcpConfig::default()
            }),
            prerequisite_task_ids: None,
        };
        assert!(chatos_async_planner::ensure_planner_required_fields(&missing_model).is_err());

        let missing_builtin_kinds = CreateTaskRequest {
            default_model_config_id: Some("model-1".to_string()),
            mcp_config: Some(TaskMcpConfig {
                enabled_builtin_kinds: Vec::new(),
                ..TaskMcpConfig::default()
            }),
            ..missing_model.clone()
        };
        assert!(chatos_async_planner::ensure_planner_required_fields(&missing_builtin_kinds).is_err());

        let valid = CreateTaskRequest {
            default_model_config_id: Some("model-1".to_string()),
            mcp_config: Some(TaskMcpConfig {
                enabled_builtin_kinds: vec!["CodeMaintainerWrite".to_string()],
                ..TaskMcpConfig::default()
            }),
            ..missing_model
        };
        assert!(chatos_async_planner::ensure_planner_required_fields(&valid).is_ok());
    }

    #[test]
    fn async_planner_root_tasks_are_forced_to_contact_async_schedule() {
        let request = CreateTaskRequest {
            title: "task".to_string(),
            description: None,
            objective: "objective".to_string(),
            input_payload: None,
            status: None,
            priority: None,
            tags: None,
            default_model_config_id: Some("model-1".to_string()),
            tenant_id: None,
            subject_id: None,
            schedule: None,
            mcp_config: Some(TaskMcpConfig {
                enabled_builtin_kinds: vec!["CodeMaintainerRead".to_string()],
                ..TaskMcpConfig::default()
            }),
            prerequisite_task_ids: None,
        };
        let planned =
            chatos_async_planner::planner_root_create_request(request).expect("planner request");
        assert_eq!(
            planned.schedule.expect("schedule").mode,
            TaskScheduleMode::ContactAsync
        );
    }
}
