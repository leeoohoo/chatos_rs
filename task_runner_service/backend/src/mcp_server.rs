use std::collections::{HashMap, HashSet};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::{
    mcp_builtin_kind_guide, mcp_builtin_kind_values, BatchTaskDeleteRequest, BatchTaskRunRequest,
    BatchTaskStatusUpdateRequest, CancelUiPromptRequest, CreateModelConfigRequest,
    CreateRemoteServerRequest, CreateTaskRequest, McpServerInfo, ModelConfigRecord, RunListFilters,
    StartTaskRunRequest, SubmitUiPromptRequest, TaskListFilters, TaskMcpConfig,
    TaskMemoryContextOptions, TaskMemoryRecordsOptions, TaskRecord, TaskRunEventRecord,
    TaskRunRecord, TaskRunStatus, TaskScheduleConfig, TaskScheduleMode, TaskSourceContext,
    TaskStatsResponse, TaskStatus, TestModelConfigRequest, UiPromptRecord, UiPromptStatus,
    UpdateModelConfigRequest, UpdateTaskRequest,
};
use crate::services::{McpCatalogService, ModelConfigService, RunService, TaskService};
use crate::ui_prompt_service::UiPromptService;
use chatos_mcp_runtime::builtin_kind_by_any;

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

#[derive(Debug, Clone, Default)]
pub struct McpRequestContext {
    pub source_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub workspace_dir: Option<String>,
    pub remote_server_config: Option<String>,
}

impl McpRequestContext {
    fn task_source_context(&self) -> Result<Option<TaskSourceContext>, String> {
        if self.source_session_id.is_none()
            && self.source_turn_id.is_none()
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
            workspace_dir: self.workspace_dir.clone(),
            remote_server_config,
        }))
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

    pub fn server_info(&self) -> McpServerInfo {
        McpServerInfo {
            server_name: TASK_RUNNER_MCP_SERVER_NAME.to_string(),
            transports: vec!["http-jsonrpc".to_string(), "stdio-jsonrpc".to_string()],
            http_endpoint_path: Some(TASK_RUNNER_MCP_ENDPOINT_PATH.to_string()),
            stdio_command: Some(TASK_RUNNER_MCP_STDIO_COMMAND.to_string()),
            stdio_args: TASK_RUNNER_MCP_STDIO_ARGS
                .iter()
                .map(|item| item.to_string())
                .collect(),
            tool_names: self
                .list_tools()
                .into_iter()
                .filter_map(|tool| {
                    tool.get("name")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                })
                .collect(),
        }
    }

    pub fn list_tools(&self) -> Vec<Value> {
        vec![
            tool_definition(
                "list_tasks",
                "List Task Runner tasks with optional status, keyword, tag, schedule, or parent filters.",
                json!({
                    "type": "object",
                    "properties": {
                        "status": { "type": "string", "enum": task_status_values() },
                        "keyword": { "type": "string" },
                        "tag": { "type": "string" },
                        "model_config_id": { "type": "string" },
                        "scheduled_only": { "type": "boolean" },
                        "parent_task_id": { "type": "string" },
                        "source_run_id": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                    },
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "get_task",
                "Get one Task Runner task by id.",
                required_object_schema(
                    json!({
                        "task_id": { "type": "string", "minLength": 1 }
                    }),
                    &["task_id"],
                ),
            ),
            tool_definition(
                "get_task_stats",
                "Get aggregate task counts for the Task Runner workspace.",
                empty_object_schema(),
            ),
            tool_definition(
                "create_task",
                "Create a new Task Runner task for the current authenticated agent. Ownership and memory scope are assigned automatically by Task Runner.",
                create_task_schema(),
            ),
            tool_definition(
                "list_mcp_builtin_catalog",
                "List builtin MCP capabilities that can be enabled for newly created Task Runner tasks, including use cases, capabilities, and current tool names.",
                empty_object_schema(),
            ),
            tool_definition(
                "create_tasks_with_prerequisites",
                "Create multiple Task Runner tasks in one call and connect prerequisite edges using temporary client_ref values plus existing prerequisite_task_ids. Use this when new prerequisite tasks do not have real task ids yet.",
                create_tasks_with_prerequisites_schema(),
            ),
            tool_definition(
                "update_task",
                "Update an existing Task Runner task.",
                required_object_schema(
                    json!({
                        "task_id": { "type": "string", "minLength": 1 },
                        "patch": update_task_schema()
                    }),
                    &["task_id", "patch"],
                ),
            ),
            tool_definition(
                "set_task_prerequisites",
                "Replace the direct prerequisite task ids for one existing Task Runner task.",
                required_object_schema(
                    json!({
                        "task_id": { "type": "string", "minLength": 1 },
                        "prerequisite_task_ids": prerequisite_task_ids_schema()
                    }),
                    &["task_id", "prerequisite_task_ids"],
                ),
            ),
            tool_definition(
                "get_task_dependency_graph",
                "Get direct and transitive prerequisite tasks for one Task Runner task.",
                required_object_schema(
                    json!({
                        "task_id": { "type": "string", "minLength": 1 }
                    }),
                    &["task_id"],
                ),
            ),
            tool_definition(
                "delete_task",
                "Delete a Task Runner task by id.",
                required_object_schema(
                    json!({
                        "task_id": { "type": "string", "minLength": 1 }
                    }),
                    &["task_id"],
                ),
            ),
            tool_definition(
                "batch_update_task_status",
                "Update the status of multiple Task Runner tasks in one call.",
                required_object_schema(
                    json!({
                        "task_ids": {
                            "type": "array",
                            "items": { "type": "string", "minLength": 1 },
                            "minItems": 1
                        },
                        "status": { "type": "string", "enum": task_status_values() }
                    }),
                    &["task_ids", "status"],
                ),
            ),
            tool_definition(
                "batch_delete_tasks",
                "Delete multiple Task Runner tasks by id.",
                required_object_schema(
                    json!({
                        "task_ids": {
                            "type": "array",
                            "items": { "type": "string", "minLength": 1 },
                            "minItems": 1
                        }
                    }),
                    &["task_ids"],
                ),
            ),
            tool_definition(
                "list_model_configs",
                "List enabled and disabled model configs that Task Runner can use.",
                empty_object_schema(),
            ),
            tool_definition(
                "get_model_config",
                "Get one Task Runner model config by id.",
                required_object_schema(
                    json!({
                        "model_config_id": { "type": "string", "minLength": 1 }
                    }),
                    &["model_config_id"],
                ),
            ),
            tool_definition(
                "create_model_config",
                "Create a new Task Runner model config.",
                create_model_config_schema(),
            ),
            tool_definition(
                "update_model_config",
                "Update an existing Task Runner model config.",
                required_object_schema(
                    json!({
                        "model_config_id": { "type": "string", "minLength": 1 },
                        "patch": update_model_config_schema()
                    }),
                    &["model_config_id", "patch"],
                ),
            ),
            tool_definition(
                "delete_model_config",
                "Delete a Task Runner model config by id.",
                required_object_schema(
                    json!({
                        "model_config_id": { "type": "string", "minLength": 1 }
                    }),
                    &["model_config_id"],
                ),
            ),
            tool_definition(
                "test_model_config",
                "Test whether one Task Runner model config can call its upstream model service.",
                required_object_schema(
                    json!({
                        "model_config_id": { "type": "string", "minLength": 1 },
                        "prompt": { "type": "string" }
                    }),
                    &["model_config_id"],
                ),
            ),
            tool_definition(
                "list_runs",
                "List Task Runner runs with optional task, status, or model config filters.",
                json!({
                    "type": "object",
                    "properties": {
                        "task_id": { "type": "string" },
                        "status": { "type": "string", "enum": run_status_values() },
                        "model_config_id": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                    },
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "get_run",
                "Get one Task Runner run by id.",
                required_object_schema(
                    json!({
                        "run_id": { "type": "string", "minLength": 1 }
                    }),
                    &["run_id"],
                ),
            ),
            tool_definition(
                "start_task_run",
                "Start a new run for a Task Runner task.",
                required_object_schema(
                    json!({
                        "task_id": { "type": "string", "minLength": 1 },
                        "model_config_id": { "type": "string" },
                        "prompt_override": { "type": "string" }
                    }),
                    &["task_id"],
                ),
            ),
            tool_definition(
                "batch_start_task_runs",
                "Start new runs for multiple Task Runner tasks.",
                required_object_schema(
                    json!({
                        "task_ids": {
                            "type": "array",
                            "items": { "type": "string", "minLength": 1 },
                            "minItems": 1
                        },
                        "model_config_id": { "type": "string" },
                        "prompt_override": { "type": "string" }
                    }),
                    &["task_ids"],
                ),
            ),
            tool_definition(
                "get_task_memory_context",
                "Read the composed Memory Engine context and thread summary for one task.",
                required_object_schema(
                    json!({
                        "task_id": { "type": "string", "minLength": 1 },
                        "include_recent_records": { "type": "boolean" },
                        "include_thread_summary": { "type": "boolean" },
                        "include_subject_memory": { "type": "boolean" },
                        "recent_record_limit": { "type": "integer", "minimum": 1, "maximum": 100 },
                        "summary_limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                    }),
                    &["task_id"],
                ),
            ),
            tool_definition(
                "list_task_memory_records",
                "List Memory Engine records persisted for one Task Runner task thread.",
                required_object_schema(
                    json!({
                        "task_id": { "type": "string", "minLength": 1 },
                        "role": { "type": "string" },
                        "record_type": { "type": "string" },
                        "summary_status": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 200 },
                        "offset": { "type": "integer", "minimum": 0 },
                        "order": { "type": "string", "enum": ["asc", "desc"] }
                    }),
                    &["task_id"],
                ),
            ),
            tool_definition(
                "summarize_task_memory",
                "Trigger a Memory Engine repair summary job for one task thread.",
                required_object_schema(
                    json!({
                        "task_id": { "type": "string", "minLength": 1 }
                    }),
                    &["task_id"],
                ),
            ),
            tool_definition(
                "cancel_run",
                "Request cancellation for a running or queued Task Runner run.",
                required_object_schema(
                    json!({
                        "run_id": { "type": "string", "minLength": 1 }
                    }),
                    &["run_id"],
                ),
            ),
            tool_definition(
                "retry_run",
                "Create a new retry run using the previous run's task and model config.",
                required_object_schema(
                    json!({
                        "run_id": { "type": "string", "minLength": 1 }
                    }),
                    &["run_id"],
                ),
            ),
            tool_definition(
                "list_run_events",
                "List stored execution events for one Task Runner run.",
                required_object_schema(
                    json!({
                        "run_id": { "type": "string", "minLength": 1 }
                    }),
                    &["run_id"],
                ),
            ),
            tool_definition(
                "list_prompts",
                "List ui_prompter prompts emitted during Task Runner execution.",
                json!({
                    "type": "object",
                    "properties": {
                        "task_id": { "type": "string" },
                        "run_id": { "type": "string" },
                        "status": { "type": "string", "enum": prompt_status_values() }
                    },
                    "additionalProperties": false
                }),
            ),
            tool_definition(
                "get_prompt",
                "Get one Task Runner ui prompt by id.",
                required_object_schema(
                    json!({
                        "prompt_id": { "type": "string", "minLength": 1 }
                    }),
                    &["prompt_id"],
                ),
            ),
            tool_definition(
                "submit_prompt",
                "Submit values or selections for a pending Task Runner ui prompt.",
                required_object_schema(
                    json!({
                        "prompt_id": { "type": "string", "minLength": 1 },
                        "values": { "type": "object" },
                        "selection": {},
                        "reason": { "type": "string" }
                    }),
                    &["prompt_id"],
                ),
            ),
            tool_definition(
                "cancel_prompt",
                "Cancel a pending Task Runner ui prompt if the prompt allows cancellation.",
                required_object_schema(
                    json!({
                        "prompt_id": { "type": "string", "minLength": 1 },
                        "reason": { "type": "string" }
                    }),
                    &["prompt_id"],
                ),
            ),
        ]
    }

    fn list_tools_for_user(&self, current_user: &CurrentUser) -> Vec<Value> {
        let tools = self.list_tools();
        if current_user.is_admin() {
            return tools;
        }
        tools
            .into_iter()
            .filter(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(agent_tool_allowed)
            })
            .collect()
    }

    pub async fn handle_jsonrpc(
        &self,
        request: JsonRpcRequest,
        current_user: CurrentUser,
        request_context: McpRequestContext,
    ) -> JsonRpcResponse {
        let id = request.id.unwrap_or(Value::Null);
        match request.method.as_str() {
            "tools/list" => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(json!({ "tools": self.list_tools_for_user(&current_user) })),
                error: None,
            },
            "tools/call" => match self
                .handle_tool_call(request.params, &current_user, &request_context)
                .await
            {
                Ok(result) => JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(result),
                    error: None,
                },
                Err(message) => JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32000,
                        message,
                    }),
                },
            },
            other => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("method not found: {other}"),
                }),
            },
        }
    }

    async fn handle_tool_call(
        &self,
        params: Value,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Value, String> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "tools/call.name is required".to_string())?;
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        self.call_tool(name, args, current_user, request_context)
            .await
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Value, String> {
        if !current_user.is_admin() && !agent_tool_allowed(name) {
            return Err("当前 agent 无权调用该任务系统工具".to_string());
        }
        match name {
            "list_tasks" => {
                let args: ListTasksArgs = decode_args(args)?;
                let tasks = self
                    .task_service
                    .list_tasks_filtered(TaskListFilters {
                        status: args.status,
                        keyword: args.keyword,
                        tag: args.tag,
                        model_config_id: args.model_config_id,
                        creator_user_id: task_creator_filter(current_user),
                        scheduled_only: args.scheduled_only,
                        parent_task_id: args.parent_task_id,
                        source_run_id: args.source_run_id,
                        limit: args.limit,
                        offset: None,
                    })
                    .await?;
                Ok(text_result(tasks_for_external_mcp(tasks)))
            }
            "get_task" => {
                let args: TaskIdArgs = decode_args(args)?;
                let task = self
                    .require_task_for_user(args.task_id.as_str(), current_user)
                    .await?;
                Ok(text_result(task_for_external_mcp(task)))
            }
            "get_task_stats" => {
                let _ = decode_args::<Value>(args).ok();
                let stats = self.task_stats_for_user(current_user).await?;
                Ok(text_result(json!(stats)))
            }
            "create_task" => {
                let input: CreateTaskRequest =
                    decode_args::<CreateTaskArgs>(args)?.into_request()?;
                let task = self
                    .task_service
                    .create_task(
                        input,
                        Some(current_user),
                        request_context.task_source_context()?,
                    )
                    .await?;
                Ok(text_result(task_for_external_mcp(task)))
            }
            "list_mcp_builtin_catalog" => {
                let _ = decode_args::<Value>(args).ok();
                Ok(text_result(json!(self.mcp_catalog_service.list_catalog())))
            }
            "create_tasks_with_prerequisites" => {
                let args: CreateTasksWithPrerequisitesArgs = decode_args(args)?;
                let result = self
                    .create_tasks_with_prerequisites(args, current_user, request_context)
                    .await?;
                Ok(text_result(result))
            }
            "update_task" => {
                let args: UpdateTaskArgs = decode_args(args)?;
                self.require_task_for_user(args.task_id.as_str(), current_user)
                    .await?;
                let task = self
                    .task_service
                    .update_task(args.task_id.as_str(), args.patch)
                    .await?
                    .ok_or_else(|| format!("任务不存在: {}", args.task_id))?;
                Ok(text_result(task_for_external_mcp(task)))
            }
            "set_task_prerequisites" => {
                let args: SetTaskPrerequisitesArgs = decode_args(args)?;
                self.require_task_for_user(args.task_id.as_str(), current_user)
                    .await?;
                let task = self
                    .task_service
                    .set_task_prerequisites(
                        args.task_id.as_str(),
                        args.prerequisite_task_ids,
                        Some(current_user),
                    )
                    .await?
                    .ok_or_else(|| format!("任务不存在: {}", args.task_id))?;
                Ok(text_result(task_for_external_mcp(task)))
            }
            "get_task_dependency_graph" => {
                let args: TaskIdArgs = decode_args(args)?;
                self.require_task_for_user(args.task_id.as_str(), current_user)
                    .await?;
                let graph = self
                    .task_service
                    .get_task_dependency_graph(args.task_id.as_str())
                    .await?
                    .ok_or_else(|| format!("任务不存在: {}", args.task_id))?;
                Ok(text_result(json!(graph)))
            }
            "delete_task" => {
                let args: TaskIdArgs = decode_args(args)?;
                self.require_task_for_user(args.task_id.as_str(), current_user)
                    .await?;
                let deleted = self.task_service.delete_task(args.task_id.as_str()).await?;
                if !deleted {
                    return Err(format!("任务不存在: {}", args.task_id));
                }
                Ok(text_result(json!({
                    "deleted": true,
                    "task_id": args.task_id,
                })))
            }
            "batch_update_task_status" => {
                let args: BatchTaskStatusUpdateArgs = decode_args(args)?;
                self.require_tasks_for_user(args.task_ids.as_slice(), current_user)
                    .await?;
                let result = self
                    .task_service
                    .batch_update_status(BatchTaskStatusUpdateRequest {
                        task_ids: args.task_ids,
                        status: args.status,
                    })
                    .await?;
                Ok(text_result(json!(result)))
            }
            "batch_delete_tasks" => {
                let args: BatchTaskDeleteArgs = decode_args(args)?;
                self.require_tasks_for_user(args.task_ids.as_slice(), current_user)
                    .await?;
                let result = self
                    .task_service
                    .batch_delete_tasks(BatchTaskDeleteRequest {
                        task_ids: args.task_ids,
                    })
                    .await?;
                Ok(text_result(json!(result)))
            }
            "list_model_configs" => {
                let _ = decode_args::<Value>(args).ok();
                let models = self.model_config_service.list_model_configs().await?;
                Ok(text_result(json!(model_configs_for_user(
                    models,
                    current_user
                ))))
            }
            "get_model_config" => {
                let args: ModelConfigIdArgs = decode_args(args)?;
                let model = self
                    .model_config_service
                    .get_model_config(args.model_config_id.as_str())
                    .await?
                    .ok_or_else(|| format!("模型配置不存在: {}", args.model_config_id))?;
                Ok(text_result(model_config_for_user(model, current_user)))
            }
            "create_model_config" => {
                require_admin_tool(current_user)?;
                let input: CreateModelConfigRequest = decode_args(args)?;
                let model = self.model_config_service.create_model_config(input).await?;
                Ok(text_result(json!(model)))
            }
            "update_model_config" => {
                require_admin_tool(current_user)?;
                let args: UpdateModelConfigArgs = decode_args(args)?;
                let model = self
                    .model_config_service
                    .update_model_config(args.model_config_id.as_str(), args.patch)
                    .await?
                    .ok_or_else(|| format!("模型配置不存在: {}", args.model_config_id))?;
                Ok(text_result(json!(model)))
            }
            "delete_model_config" => {
                require_admin_tool(current_user)?;
                let args: ModelConfigIdArgs = decode_args(args)?;
                let deleted = self
                    .model_config_service
                    .delete_model_config(args.model_config_id.as_str())
                    .await?;
                if !deleted {
                    return Err(format!("模型配置不存在: {}", args.model_config_id));
                }
                Ok(text_result(json!({
                    "deleted": true,
                    "model_config_id": args.model_config_id,
                })))
            }
            "test_model_config" => {
                require_admin_tool(current_user)?;
                let args: TestModelConfigArgs = decode_args(args)?;
                let result = self
                    .model_config_service
                    .test_model_config(
                        args.model_config_id.as_str(),
                        TestModelConfigRequest {
                            prompt: args.prompt,
                        },
                    )
                    .await?
                    .ok_or_else(|| format!("模型配置不存在: {}", args.model_config_id))?;
                Ok(text_result(json!(result)))
            }
            "list_runs" => {
                let args: ListRunsArgs = decode_args(args)?;
                if let Some(task_id) = args.task_id.as_deref() {
                    self.require_task_for_user(task_id, current_user).await?;
                }
                let runs = self
                    .run_service
                    .list_runs_filtered(RunListFilters {
                        task_id: args.task_id,
                        status: args.status,
                        model_config_id: args.model_config_id,
                        keyword: None,
                        limit: args.limit,
                        offset: None,
                    })
                    .await?;
                let runs = self.filter_runs_for_user(runs, current_user).await?;
                Ok(text_result(json!(runs)))
            }
            "get_run" => {
                let args: RunIdArgs = decode_args(args)?;
                let run = self
                    .require_run_for_user(args.run_id.as_str(), current_user)
                    .await?;
                Ok(text_result(json!(run)))
            }
            "start_task_run" => {
                let args: StartTaskRunArgs = decode_args(args)?;
                self.require_task_for_user(args.task_id.as_str(), current_user)
                    .await?;
                let run = self
                    .run_service
                    .start_run(
                        args.task_id.as_str(),
                        StartTaskRunRequest {
                            model_config_id: args.model_config_id,
                            prompt_override: args.prompt_override,
                        },
                    )
                    .await?;
                Ok(text_result(json!(run)))
            }
            "batch_start_task_runs" => {
                let args: BatchTaskRunArgs = decode_args(args)?;
                self.require_tasks_for_user(args.task_ids.as_slice(), current_user)
                    .await?;
                let result = self
                    .run_service
                    .batch_start_runs(BatchTaskRunRequest {
                        task_ids: args.task_ids,
                        model_config_id: args.model_config_id,
                        prompt_override: args.prompt_override,
                    })
                    .await?;
                Ok(text_result(json!(result)))
            }
            "get_task_memory_context" => {
                let args: GetTaskMemoryContextArgs = decode_args(args)?;
                self.require_task_for_user(args.task_id.as_str(), current_user)
                    .await?;
                let response = self
                    .task_service
                    .get_task_memory_context(
                        args.task_id.as_str(),
                        TaskMemoryContextOptions {
                            include_recent_records: args.include_recent_records,
                            include_thread_summary: args.include_thread_summary,
                            include_subject_memory: args.include_subject_memory,
                            recent_record_limit: args.recent_record_limit,
                            summary_limit: args.summary_limit,
                        },
                    )
                    .await?
                    .ok_or_else(|| format!("任务不存在: {}", args.task_id))?;
                Ok(text_result(json!(response)))
            }
            "list_task_memory_records" => {
                let args: ListTaskMemoryRecordsArgs = decode_args(args)?;
                self.require_task_for_user(args.task_id.as_str(), current_user)
                    .await?;
                let response = self
                    .task_service
                    .get_task_memory_records(
                        args.task_id.as_str(),
                        TaskMemoryRecordsOptions {
                            role: args.role,
                            record_type: args.record_type,
                            summary_status: args.summary_status,
                            limit: args.limit,
                            offset: args.offset,
                            order: args.order,
                        },
                    )
                    .await?
                    .ok_or_else(|| format!("任务不存在: {}", args.task_id))?;
                Ok(text_result(json!(response)))
            }
            "summarize_task_memory" => {
                let args: TaskIdArgs = decode_args(args)?;
                self.require_task_for_user(args.task_id.as_str(), current_user)
                    .await?;
                let response = self
                    .task_service
                    .summarize_task_memory(args.task_id.as_str())
                    .await?
                    .ok_or_else(|| format!("任务不存在: {}", args.task_id))?;
                Ok(text_result(json!(response)))
            }
            "cancel_run" => {
                let args: RunIdArgs = decode_args(args)?;
                self.require_run_for_user(args.run_id.as_str(), current_user)
                    .await?;
                let run = self
                    .run_service
                    .cancel_run(args.run_id.as_str())
                    .await?
                    .ok_or_else(|| format!("运行记录不存在: {}", args.run_id))?;
                Ok(text_result(json!(run)))
            }
            "retry_run" => {
                let args: RunIdArgs = decode_args(args)?;
                self.require_run_for_user(args.run_id.as_str(), current_user)
                    .await?;
                let run = self
                    .run_service
                    .retry_run(args.run_id.as_str())
                    .await?
                    .ok_or_else(|| format!("运行记录不存在: {}", args.run_id))?;
                Ok(text_result(json!(run)))
            }
            "list_run_events" => {
                let args: RunIdArgs = decode_args(args)?;
                self.require_run_for_user(args.run_id.as_str(), current_user)
                    .await?;
                let events = self
                    .run_service
                    .list_run_events(args.run_id.as_str())
                    .await?;
                Ok(text_result(json!(events)))
            }
            "list_prompts" => {
                let args: ListPromptsArgs = decode_args(args)?;
                if let Some(task_id) = args.task_id.as_deref() {
                    self.require_task_for_user(task_id, current_user).await?;
                }
                if let Some(run_id) = args.run_id.as_deref() {
                    self.require_run_for_user(run_id, current_user).await?;
                }
                let prompts = self
                    .ui_prompt_service
                    .list_prompts(args.task_id.as_deref(), args.run_id.as_deref(), args.status)
                    .await?;
                let prompts = self.filter_prompts_for_user(prompts, current_user).await?;
                Ok(text_result(json!(prompts)))
            }
            "get_prompt" => {
                let args: PromptIdArgs = decode_args(args)?;
                let prompt = self
                    .ui_prompt_service
                    .get_prompt(args.prompt_id.as_str())
                    .await?
                    .ok_or_else(|| format!("提示不存在: {}", args.prompt_id))?;
                self.require_prompt_for_user(&prompt, current_user).await?;
                Ok(text_result(json!(prompt)))
            }
            "submit_prompt" => {
                let args: SubmitPromptArgs = decode_args(args)?;
                let prompt = self
                    .ui_prompt_service
                    .get_prompt(args.prompt_id.as_str())
                    .await?
                    .ok_or_else(|| format!("提示不存在: {}", args.prompt_id))?;
                self.require_prompt_for_user(&prompt, current_user).await?;
                let prompt = self
                    .ui_prompt_service
                    .submit_prompt(
                        args.prompt_id.as_str(),
                        SubmitUiPromptRequest {
                            values: args.values,
                            selection: args.selection,
                            reason: args.reason,
                        },
                    )
                    .await?
                    .ok_or_else(|| format!("提示不存在: {}", args.prompt_id))?;
                Ok(text_result(json!(prompt)))
            }
            "cancel_prompt" => {
                let args: CancelPromptArgs = decode_args(args)?;
                let prompt = self
                    .ui_prompt_service
                    .get_prompt(args.prompt_id.as_str())
                    .await?
                    .ok_or_else(|| format!("提示不存在: {}", args.prompt_id))?;
                self.require_prompt_for_user(&prompt, current_user).await?;
                let prompt = self
                    .ui_prompt_service
                    .cancel_prompt(
                        args.prompt_id.as_str(),
                        CancelUiPromptRequest {
                            reason: args.reason,
                        },
                    )
                    .await?
                    .ok_or_else(|| format!("提示不存在: {}", args.prompt_id))?;
                Ok(text_result(json!(prompt)))
            }
            other => Err(format!("tool not found: {other}")),
        }
    }

    async fn task_stats_for_user(
        &self,
        current_user: &CurrentUser,
    ) -> Result<TaskStatsResponse, String> {
        if current_user.is_admin() {
            return self.task_service.task_stats().await;
        }
        let tasks = self
            .task_service
            .list_tasks_filtered(TaskListFilters {
                creator_user_id: Some(current_user.id.clone()),
                ..TaskListFilters::default()
            })
            .await?;
        let mut stats = TaskStatsResponse {
            total: 0,
            scheduled: 0,
            follow_up: 0,
            draft: 0,
            ready: 0,
            running: 0,
            succeeded: 0,
            failed: 0,
            blocked: 0,
            cancelled: 0,
            archived: 0,
        };
        for task in tasks {
            stats.total += 1;
            if !matches!(task.schedule.mode, TaskScheduleMode::Manual) {
                stats.scheduled += 1;
            }
            if task.parent_task_id.is_some() {
                stats.follow_up += 1;
            }
            match task.status {
                TaskStatus::Draft => stats.draft += 1,
                TaskStatus::Ready => stats.ready += 1,
                TaskStatus::Running => stats.running += 1,
                TaskStatus::Succeeded => stats.succeeded += 1,
                TaskStatus::Failed => stats.failed += 1,
                TaskStatus::Blocked => stats.blocked += 1,
                TaskStatus::Cancelled => stats.cancelled += 1,
                TaskStatus::Archived => stats.archived += 1,
            }
        }
        Ok(stats)
    }

    async fn require_tasks_for_user(
        &self,
        task_ids: &[String],
        current_user: &CurrentUser,
    ) -> Result<(), String> {
        for task_id in task_ids {
            self.require_task_for_user(task_id.as_str(), current_user)
                .await?;
        }
        Ok(())
    }

    async fn create_tasks_with_prerequisites(
        &self,
        args: CreateTasksWithPrerequisitesArgs,
        current_user: &CurrentUser,
        request_context: &McpRequestContext,
    ) -> Result<Value, String> {
        if args.tasks.is_empty() {
            return Err("tasks 不能为空".to_string());
        }
        if args.tasks.len() > 50 {
            return Err("一次最多创建 50 个任务".to_string());
        }

        let mut refs = HashSet::new();
        for task in &args.tasks {
            let client_ref = task.client_ref.trim();
            if client_ref.is_empty() {
                return Err("client_ref 不能为空".to_string());
            }
            if !refs.insert(client_ref.to_string()) {
                return Err(format!("client_ref 重复: {client_ref}"));
            }
        }

        for task in &args.tasks {
            for prerequisite_ref in &task.prerequisite_refs {
                let prerequisite_ref = prerequisite_ref.trim();
                if !refs.contains(prerequisite_ref) {
                    return Err(format!("未知 prerequisite_ref: {prerequisite_ref}"));
                }
                if prerequisite_ref == task.client_ref.trim() {
                    return Err(format!("任务不能依赖自身: {prerequisite_ref}"));
                }
            }
            for prerequisite_task_id in &task.prerequisite_task_ids {
                self.require_task_for_user(prerequisite_task_id, current_user)
                    .await?;
            }
        }
        ensure_client_ref_graph_acyclic(&args.tasks)?;

        let mut ref_to_task_id = HashMap::new();
        let mut created_tasks = Vec::new();
        let mut pending_edges = Vec::<(String, Vec<String>, Vec<String>)>::new();

        for item in args.tasks {
            let client_ref = item.client_ref.trim().to_string();
            let mut mcp_config = None;
            if let Some(enabled_builtin_kinds) = item.enabled_builtin_kinds {
                let normalized = normalize_mcp_builtin_kind_names(enabled_builtin_kinds)?;
                let config = mcp_config.get_or_insert_with(TaskMcpConfig::default);
                config.enabled = true;
                config.enabled_builtin_kinds = normalized;
            }
            let task = self
                .task_service
                .create_task(
                    CreateTaskRequest {
                        title: item.title,
                        description: item.description,
                        objective: item.objective,
                        input_payload: item.input_payload,
                        status: None,
                        priority: item.priority,
                        tags: item.tags,
                        default_model_config_id: item.default_model_config_id,
                        tenant_id: None,
                        subject_id: None,
                        schedule: item.schedule,
                        mcp_config,
                        prerequisite_task_ids: Some(item.prerequisite_task_ids.clone()),
                    },
                    Some(current_user),
                    request_context.task_source_context()?,
                )
                .await?;
            ref_to_task_id.insert(client_ref.clone(), task.id.clone());
            pending_edges.push((
                task.id.clone(),
                item.prerequisite_refs,
                item.prerequisite_task_ids,
            ));
            created_tasks.push(json!({
                "client_ref": client_ref,
                "task_id": task.id,
                "title": task.title,
                "status": task.status,
            }));
        }

        let mut dependency_edges = Vec::new();
        for (task_id, prerequisite_refs, existing_prerequisite_ids) in pending_edges {
            let mut prerequisite_ids = existing_prerequisite_ids;
            for prerequisite_ref in prerequisite_refs {
                let Some(prerequisite_task_id) = ref_to_task_id.get(prerequisite_ref.trim()) else {
                    return Err(format!("未知 prerequisite_ref: {prerequisite_ref}"));
                };
                prerequisite_ids.push(prerequisite_task_id.clone());
            }
            let task = self
                .task_service
                .set_task_prerequisites(&task_id, prerequisite_ids, Some(current_user))
                .await?
                .ok_or_else(|| format!("任务不存在: {task_id}"))?;
            for prerequisite_task_id in task.prerequisite_task_ids {
                dependency_edges.push(json!({
                    "task_id": task.id,
                    "prerequisite_task_id": prerequisite_task_id,
                }));
            }
        }

        Ok(json!({
            "created_tasks": created_tasks,
            "dependency_edges": dependency_edges,
        }))
    }

    async fn require_task_for_user(
        &self,
        task_id: &str,
        current_user: &CurrentUser,
    ) -> Result<TaskRecord, String> {
        let task = self
            .task_service
            .get_task(task_id)
            .await?
            .ok_or_else(|| format!("任务不存在: {task_id}"))?;
        ensure_task_owner(&task, current_user)?;
        Ok(task)
    }

    async fn require_run_for_user(
        &self,
        run_id: &str,
        current_user: &CurrentUser,
    ) -> Result<TaskRunRecord, String> {
        let run = self
            .run_service
            .get_run(run_id)
            .await?
            .ok_or_else(|| format!("运行记录不存在: {run_id}"))?;
        self.require_task_for_user(run.task_id.as_str(), current_user)
            .await?;
        Ok(run)
    }

    async fn require_prompt_for_user(
        &self,
        prompt: &UiPromptRecord,
        current_user: &CurrentUser,
    ) -> Result<(), String> {
        if current_user.is_admin() {
            return Ok(());
        }
        if let Some(task_id) = prompt.task_id.as_deref() {
            self.require_task_for_user(task_id, current_user).await?;
            return Ok(());
        }
        if let Some(run_id) = prompt.run_id.as_deref() {
            self.require_run_for_user(run_id, current_user).await?;
            return Ok(());
        }
        Err("当前 agent 无权访问该提示".to_string())
    }

    async fn filter_runs_for_user(
        &self,
        runs: Vec<TaskRunRecord>,
        current_user: &CurrentUser,
    ) -> Result<Vec<TaskRunRecord>, String> {
        if current_user.is_admin() {
            return Ok(runs);
        }
        let mut out = Vec::new();
        for run in runs {
            if self
                .require_task_for_user(run.task_id.as_str(), current_user)
                .await
                .is_ok()
            {
                out.push(run);
            }
        }
        Ok(out)
    }

    async fn filter_prompts_for_user(
        &self,
        prompts: Vec<UiPromptRecord>,
        current_user: &CurrentUser,
    ) -> Result<Vec<UiPromptRecord>, String> {
        if current_user.is_admin() {
            return Ok(prompts);
        }
        let mut out = Vec::new();
        for prompt in prompts {
            if self
                .require_prompt_for_user(&prompt, current_user)
                .await
                .is_ok()
            {
                out.push(prompt);
            }
        }
        Ok(out)
    }
}

fn agent_tool_allowed(name: &str) -> bool {
    matches!(
        name,
        "list_tasks"
            | "get_task"
            | "get_task_stats"
            | "create_task"
            | "list_mcp_builtin_catalog"
            | "create_tasks_with_prerequisites"
            | "update_task"
            | "set_task_prerequisites"
            | "get_task_dependency_graph"
            | "delete_task"
            | "batch_update_task_status"
            | "batch_delete_tasks"
            | "list_model_configs"
            | "get_model_config"
            | "list_runs"
            | "get_run"
            | "start_task_run"
            | "batch_start_task_runs"
            | "get_task_memory_context"
            | "list_task_memory_records"
            | "summarize_task_memory"
            | "cancel_run"
            | "retry_run"
            | "list_run_events"
            | "list_prompts"
            | "get_prompt"
            | "submit_prompt"
            | "cancel_prompt"
    )
}

fn task_creator_filter(current_user: &CurrentUser) -> Option<String> {
    (!current_user.is_admin()).then(|| current_user.id.clone())
}

fn ensure_task_owner(task: &TaskRecord, current_user: &CurrentUser) -> Result<(), String> {
    if current_user.is_admin() {
        return Ok(());
    }
    if task.creator_user_id.as_deref() == Some(current_user.id.as_str()) {
        return Ok(());
    }
    Err("当前 agent 无权访问该任务".to_string())
}

fn require_admin_tool(current_user: &CurrentUser) -> Result<(), String> {
    if current_user.is_admin() {
        Ok(())
    } else {
        Err("当前 agent 无权调用管理员工具".to_string())
    }
}

fn tasks_for_external_mcp(tasks: Vec<TaskRecord>) -> Value {
    Value::Array(tasks.into_iter().map(task_for_external_mcp).collect())
}

fn task_for_external_mcp(task: TaskRecord) -> Value {
    let mut value = json!(task);
    remove_process_log_field(&mut value);
    value
}

fn remove_process_log_field(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("process_log");
    }
}

fn model_configs_for_user(
    models: Vec<ModelConfigRecord>,
    current_user: &CurrentUser,
) -> Vec<Value> {
    models
        .into_iter()
        .map(|model| model_config_for_user(model, current_user))
        .collect()
}

fn model_config_for_user(model: ModelConfigRecord, current_user: &CurrentUser) -> Value {
    if current_user.is_admin() {
        return json!(model);
    }
    let mut value = json!(model);
    if let Some(object) = value.as_object_mut() {
        object.insert("api_key".to_string(), Value::String(String::new()));
    }
    value
}

fn tool_definition(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
    })
}

fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn required_object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn create_task_schema() -> Value {
    let enabled_builtin_kinds_description = builtin_mcp_kind_schema_description();
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "minLength": 1, "description": "任务标题。" },
            "description": { "type": "string", "description": "任务背景、上下文或补充说明。" },
            "objective": { "type": "string", "minLength": 1, "description": "任务执行目标，说明任务完成时应达成什么结果。" },
            "input_payload": { "description": "任务输入数据。可以放结构化 JSON、引用信息或执行所需材料。" },
            "priority": { "type": "integer", "description": "任务优先级，数字越大优先级越高。" },
            "tags": { "type": "array", "items": { "type": "string" }, "description": "任务标签。" },
            "default_model_config_id": { "type": "string", "description": "指定任务默认使用的模型配置 ID；不确定时不要传。" },
            "schedule": { "type": "object", "description": "任务调度配置；不需要定时或延迟执行时不要传。" },
            "prerequisite_task_ids": prerequisite_task_ids_schema(),
            "enabled_builtin_kinds": {
                "type": "array",
                "items": builtin_mcp_kind_item_schema(),
                "uniqueItems": true,
                "description": enabled_builtin_kinds_description
            }
        },
        "required": ["title", "objective"],
        "additionalProperties": false
    })
}

fn update_task_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "description": { "type": "string" },
            "objective": { "type": "string" },
            "input_payload": {},
            "status": { "type": "string", "enum": task_status_values() },
            "priority": { "type": "integer" },
            "tags": { "type": "array", "items": { "type": "string" } },
            "default_model_config_id": { "type": "string" },
            "schedule": { "type": "object" },
            "prerequisite_task_ids": prerequisite_task_ids_schema(),
            "mcp_config": task_mcp_config_schema()
        },
        "additionalProperties": false
    })
}

fn prerequisite_task_ids_schema() -> Value {
    json!({
        "type": "array",
        "items": { "type": "string", "minLength": 1 },
        "uniqueItems": true,
        "description": "当前任务执行前必须先成功完成的真实任务 ID 列表。只能填写 list_tasks/get_task/create_task/create_tasks_with_prerequisites 返回过的真实 task_id，不能自己编造 ID；如果要同时创建新的前置任务，请使用 create_tasks_with_prerequisites 的 client_ref/prerequisite_refs。"
    })
}

fn create_tasks_with_prerequisites_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tasks": {
                "type": "array",
                "minItems": 1,
                "maxItems": 50,
                "items": {
                    "type": "object",
                    "properties": {
                        "client_ref": {
                            "type": "string",
                            "minLength": 1,
                            "description": "本次工具调用内的临时任务引用，例如 collect_logs。只在本次请求内有效，后端会返回真实 task_id。"
                        },
                        "title": { "type": "string", "minLength": 1 },
                        "description": { "type": "string" },
                        "objective": { "type": "string", "minLength": 1 },
                        "input_payload": {},
                        "priority": { "type": "integer" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "default_model_config_id": { "type": "string" },
                        "schedule": { "type": "object" },
                        "enabled_builtin_kinds": {
                            "type": "array",
                            "items": builtin_mcp_kind_item_schema(),
                            "uniqueItems": true,
                            "description": builtin_mcp_kind_schema_description()
                        },
                        "prerequisite_refs": {
                            "type": "array",
                            "items": { "type": "string", "minLength": 1 },
                            "uniqueItems": true,
                            "description": "引用同一次 create_tasks_with_prerequisites 请求中其它任务的 client_ref。用于新建任务之间的前置依赖，不能引用自己，不能成环。"
                        },
                        "prerequisite_task_ids": prerequisite_task_ids_schema()
                    },
                    "required": ["client_ref", "title", "objective"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["tasks"],
        "additionalProperties": false
    })
}

fn task_mcp_config_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "enabled": { "type": "boolean", "description": "是否启用 MCP。通常保持 true。" },
            "init_mode": {
                "type": "string",
                "enum": ["builtin_only", "full", "disabled"],
                "description": "MCP 初始化方式。任务系统通常使用 builtin_only。"
            },
            "builtin_prompt_mode": {
                "type": "string",
                "enum": ["effective", "configured"],
                "description": "MCP prompt 生成方式。通常使用 effective。"
            },
            "builtin_prompt_locale": {
                "type": "string",
                "enum": ["zh-CN", "en-US"],
                "description": "MCP prompt 语言。"
            },
            "enabled_builtin_kinds": {
                "type": "array",
                "items": builtin_mcp_kind_item_schema(),
                "uniqueItems": true,
                "description": builtin_mcp_kind_schema_description()
            }
        },
        "additionalProperties": false
    })
}

fn builtin_mcp_kind_item_schema() -> Value {
    json!({
        "type": "string",
        "enum": mcp_builtin_kind_values()
    })
}

fn builtin_mcp_kind_schema_description() -> String {
    let mut lines = vec![
        "可选的 builtin MCP 多选列表。只在任务执行确实需要对应能力时选择；不确定时可先调用 list_mcp_builtin_catalog 查看当前目录。可选值："
            .to_string(),
    ];
    for value in mcp_builtin_kind_values() {
        if let Some(kind) = builtin_kind_by_any(value.as_str()) {
            let guide = mcp_builtin_kind_guide(kind);
            lines.push(format!(
                "- {}: {} 使用场景：{}。能力：{}。",
                value,
                guide.description,
                guide.use_cases.join("、"),
                guide.capabilities.join("、")
            ));
        }
    }
    lines.join("\n")
}

fn normalize_mcp_builtin_kind_names(values: Vec<String>) -> Result<Vec<String>, String> {
    let allowed = mcp_builtin_kind_values();
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let kind = builtin_kind_by_any(trimmed).ok_or_else(|| {
            format!(
                "未知 builtin MCP kind: {trimmed}. 可选值: {}",
                allowed.join(", ")
            )
        })?;
        let normalized = kind.kind_name().to_string();
        if !out.iter().any(|item| item == &normalized) {
            out.push(normalized);
        }
    }
    Ok(out)
}

fn ensure_client_ref_graph_acyclic(
    tasks: &[CreateTaskWithPrerequisitesItem],
) -> Result<(), String> {
    let mut graph = HashMap::<String, Vec<String>>::new();
    for task in tasks {
        graph.insert(
            task.client_ref.trim().to_string(),
            task.prerequisite_refs
                .iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect(),
        );
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for root in graph.keys() {
        let mut stack = vec![(root.clone(), false)];
        while let Some((current, expanded)) = stack.pop() {
            if expanded {
                visiting.remove(&current);
                visited.insert(current);
                continue;
            }
            if visited.contains(&current) {
                continue;
            }
            if !visiting.insert(current.clone()) {
                return Err(format!("前置任务不能形成循环依赖: {current}"));
            }
            stack.push((current.clone(), true));
            for prerequisite_ref in graph.get(&current).into_iter().flatten() {
                if visiting.contains(prerequisite_ref) {
                    return Err(format!(
                        "前置任务不能形成循环依赖: {} -> {}",
                        current, prerequisite_ref
                    ));
                }
                stack.push((prerequisite_ref.clone(), false));
            }
        }
    }
    Ok(())
}

fn create_model_config_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "minLength": 1 },
            "provider": { "type": "string", "minLength": 1 },
            "base_url": { "type": "string", "minLength": 1 },
            "api_key": { "type": "string" },
            "model": { "type": "string", "minLength": 1 },
            "temperature": { "type": "number" },
            "max_output_tokens": { "type": "integer" },
            "thinking_level": { "type": "string" },
            "supports_responses": { "type": "boolean" },
            "instructions": { "type": "string" },
            "request_cwd": { "type": "string" },
            "include_prompt_cache_retention": { "type": "boolean" },
            "request_body_limit_bytes": { "type": "integer", "minimum": 1 },
            "enabled": { "type": "boolean" }
        },
        "required": ["name", "provider", "base_url", "model"],
        "additionalProperties": false
    })
}

fn update_model_config_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "provider": { "type": "string" },
            "base_url": { "type": "string" },
            "api_key": { "type": "string" },
            "model": { "type": "string" },
            "temperature": { "type": "number" },
            "max_output_tokens": { "type": "integer" },
            "thinking_level": { "type": "string" },
            "supports_responses": { "type": "boolean" },
            "instructions": { "type": "string" },
            "request_cwd": { "type": "string" },
            "include_prompt_cache_retention": { "type": "boolean" },
            "request_body_limit_bytes": { "type": "integer", "minimum": 1 },
            "enabled": { "type": "boolean" }
        },
        "additionalProperties": false
    })
}

fn task_status_values() -> Vec<&'static str> {
    vec![
        "draft",
        "ready",
        "running",
        "succeeded",
        "failed",
        "blocked",
        "cancelled",
        "archived",
    ]
}

fn run_status_values() -> Vec<&'static str> {
    vec![
        "queued",
        "running",
        "succeeded",
        "failed",
        "cancelled",
        "blocked",
    ]
}

fn prompt_status_values() -> Vec<&'static str> {
    vec!["pending", "submitted", "cancelled", "timed_out", "failed"]
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
    use super::{agent_tool_allowed, create_task_schema, task_mcp_config_schema};

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
}
