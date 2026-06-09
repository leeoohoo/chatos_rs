use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::models::{
    BatchTaskDeleteRequest, BatchTaskRunRequest, BatchTaskStatusUpdateRequest,
    CancelUiPromptRequest, CreateModelConfigRequest, CreateTaskRequest, McpServerInfo,
    RunListFilters, StartTaskRunRequest, SubmitUiPromptRequest, TaskListFilters,
    TaskMemoryContextOptions, TaskMemoryRecordsOptions, TaskRecord, TaskRunEventRecord,
    TaskRunRecord, TaskRunStatus, TaskStatus, TestModelConfigRequest, UiPromptRecord,
    UiPromptStatus, UpdateModelConfigRequest, UpdateTaskRequest,
};
use crate::services::{ModelConfigService, RunService, TaskService};
use crate::ui_prompt_service::UiPromptService;

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

#[derive(Clone)]
pub struct TaskRunnerMcpService {
    task_service: TaskService,
    model_config_service: ModelConfigService,
    run_service: RunService,
    ui_prompt_service: UiPromptService,
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
struct UpdateTaskArgs {
    task_id: String,
    #[serde(default)]
    patch: UpdateTaskRequest,
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
    ) -> Self {
        Self {
            task_service,
            model_config_service,
            run_service,
            ui_prompt_service,
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
                "Create a new Task Runner task.",
                create_task_schema(),
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

    pub async fn handle_jsonrpc(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let id = request.id.unwrap_or(Value::Null);
        match request.method.as_str() {
            "tools/list" => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(json!({ "tools": self.list_tools() })),
                error: None,
            },
            "tools/call" => match self.handle_tool_call(request.params).await {
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

    async fn handle_tool_call(&self, params: Value) -> Result<Value, String> {
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
        self.call_tool(name, args).await
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
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
                        scheduled_only: args.scheduled_only,
                        parent_task_id: args.parent_task_id,
                        source_run_id: args.source_run_id,
                        limit: args.limit,
                        offset: None,
                    })
                    .await?;
                Ok(text_result(json!(tasks)))
            }
            "get_task" => {
                let args: TaskIdArgs = decode_args(args)?;
                let task = self
                    .task_service
                    .get_task(args.task_id.as_str())
                    .await?
                    .ok_or_else(|| format!("任务不存在: {}", args.task_id))?;
                Ok(text_result(json!(task)))
            }
            "get_task_stats" => {
                let _ = decode_args::<Value>(args).ok();
                let stats = self.task_service.task_stats().await?;
                Ok(text_result(json!(stats)))
            }
            "create_task" => {
                let input: CreateTaskRequest = decode_args(args)?;
                let task = self.task_service.create_task(input, None).await?;
                Ok(text_result(json!(task)))
            }
            "update_task" => {
                let args: UpdateTaskArgs = decode_args(args)?;
                let task = self
                    .task_service
                    .update_task(args.task_id.as_str(), args.patch)
                    .await?
                    .ok_or_else(|| format!("任务不存在: {}", args.task_id))?;
                Ok(text_result(json!(task)))
            }
            "delete_task" => {
                let args: TaskIdArgs = decode_args(args)?;
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
                Ok(text_result(json!(models)))
            }
            "get_model_config" => {
                let args: ModelConfigIdArgs = decode_args(args)?;
                let model = self
                    .model_config_service
                    .get_model_config(args.model_config_id.as_str())
                    .await?
                    .ok_or_else(|| format!("模型配置不存在: {}", args.model_config_id))?;
                Ok(text_result(json!(model)))
            }
            "create_model_config" => {
                let input: CreateModelConfigRequest = decode_args(args)?;
                let model = self.model_config_service.create_model_config(input).await?;
                Ok(text_result(json!(model)))
            }
            "update_model_config" => {
                let args: UpdateModelConfigArgs = decode_args(args)?;
                let model = self
                    .model_config_service
                    .update_model_config(args.model_config_id.as_str(), args.patch)
                    .await?
                    .ok_or_else(|| format!("模型配置不存在: {}", args.model_config_id))?;
                Ok(text_result(json!(model)))
            }
            "delete_model_config" => {
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
                Ok(text_result(json!(runs)))
            }
            "get_run" => {
                let args: RunIdArgs = decode_args(args)?;
                let run = self
                    .run_service
                    .get_run(args.run_id.as_str())
                    .await?
                    .ok_or_else(|| format!("运行记录不存在: {}", args.run_id))?;
                Ok(text_result(json!(run)))
            }
            "start_task_run" => {
                let args: StartTaskRunArgs = decode_args(args)?;
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
                let response = self
                    .task_service
                    .summarize_task_memory(args.task_id.as_str())
                    .await?
                    .ok_or_else(|| format!("任务不存在: {}", args.task_id))?;
                Ok(text_result(json!(response)))
            }
            "cancel_run" => {
                let args: RunIdArgs = decode_args(args)?;
                let run = self
                    .run_service
                    .cancel_run(args.run_id.as_str())
                    .await?
                    .ok_or_else(|| format!("运行记录不存在: {}", args.run_id))?;
                Ok(text_result(json!(run)))
            }
            "retry_run" => {
                let args: RunIdArgs = decode_args(args)?;
                let run = self
                    .run_service
                    .retry_run(args.run_id.as_str())
                    .await?
                    .ok_or_else(|| format!("运行记录不存在: {}", args.run_id))?;
                Ok(text_result(json!(run)))
            }
            "list_run_events" => {
                let args: RunIdArgs = decode_args(args)?;
                let events = self
                    .run_service
                    .list_run_events(args.run_id.as_str())
                    .await?;
                Ok(text_result(json!(events)))
            }
            "list_prompts" => {
                let args: ListPromptsArgs = decode_args(args)?;
                let prompts = self
                    .ui_prompt_service
                    .list_prompts(args.task_id.as_deref(), args.run_id.as_deref(), args.status)
                    .await?;
                Ok(text_result(json!(prompts)))
            }
            "get_prompt" => {
                let args: PromptIdArgs = decode_args(args)?;
                let prompt = self
                    .ui_prompt_service
                    .get_prompt(args.prompt_id.as_str())
                    .await?
                    .ok_or_else(|| format!("提示不存在: {}", args.prompt_id))?;
                Ok(text_result(json!(prompt)))
            }
            "submit_prompt" => {
                let args: SubmitPromptArgs = decode_args(args)?;
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
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "minLength": 1 },
            "description": { "type": "string" },
            "objective": { "type": "string", "minLength": 1 },
            "input_payload": {},
            "status": { "type": "string", "enum": task_status_values() },
            "priority": { "type": "integer" },
            "tags": { "type": "array", "items": { "type": "string" } },
            "default_model_config_id": { "type": "string" },
            "tenant_id": { "type": "string" },
            "subject_id": { "type": "string" },
            "schedule": { "type": "object" },
            "mcp_config": { "type": "object" }
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
            "mcp_config": { "type": "object" }
        },
        "additionalProperties": false
    })
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
