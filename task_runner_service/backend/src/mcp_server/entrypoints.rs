use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::McpServerInfo;

use super::chatos_async_planner::enrich_tool_schemas_for_async_planner;
use super::support::{
    agent_tool_allowed_for_profile, create_model_config_schema, create_task_schema,
    create_tasks_with_prerequisites_schema, empty_object_schema,
    enrich_tool_schemas_with_model_configs, generic_run_model_config_description,
    prerequisite_task_ids_schema, prompt_status_values, required_object_schema, run_status_values,
    task_status_values, tool_definition, update_model_config_schema, update_task_schema,
};
use super::{
    JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpRequestContext, McpToolProfile,
    TaskRunnerMcpService, TASK_RUNNER_MCP_ENDPOINT_PATH, TASK_RUNNER_MCP_SERVER_NAME,
    TASK_RUNNER_MCP_STDIO_ARGS, TASK_RUNNER_MCP_STDIO_COMMAND,
};

impl TaskRunnerMcpService {
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
                "wait_for_task_completion",
                "Use after the requested Task Runner tasks have been created or adjusted. It confirms that the arranged tasks should continue through Task Runner's normal background execution flow.",
                empty_object_schema(),
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
                        "model_config_id": {
                            "type": "string",
                            "description": generic_run_model_config_description()
                        },
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
                        "model_config_id": {
                            "type": "string",
                            "description": generic_run_model_config_description()
                        },
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

    async fn list_tools_for_user(
        &self,
        current_user: &CurrentUser,
        tool_profile: McpToolProfile,
    ) -> Vec<Value> {
        let mut tools = self.list_tools();
        if let Ok(model_configs) = self.model_config_service.list_model_configs().await {
            enrich_tool_schemas_with_model_configs(&mut tools, &model_configs);
            if tool_profile == McpToolProfile::ChatosAsyncPlanner {
                enrich_tool_schemas_for_async_planner(&mut tools, &model_configs);
            }
        } else if tool_profile == McpToolProfile::ChatosAsyncPlanner {
            enrich_tool_schemas_for_async_planner(&mut tools, &[]);
        }
        if current_user.is_admin() {
            return tools;
        }
        tools
            .into_iter()
            .filter(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| agent_tool_allowed_for_profile(name, tool_profile))
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
                result: Some(json!({
                    "tools": self
                        .list_tools_for_user(&current_user, request_context.tool_profile())
                        .await
                })),
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
}
