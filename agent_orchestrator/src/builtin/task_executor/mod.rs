use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::builtin::task_planner::parsing::{parse_task_drafts, trimmed_non_empty};
use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;
use crate::services::im_task_runtime_bridge::publish_task_runtime_update_best_effort;
use crate::services::memory_server_client;
use crate::services::task_manager::{
    create_tasks_for_turn, list_tasks_for_context, resolve_task_scope_context,
};
use crate::services::task_service_client::{
    self, AckPauseTaskRequestDto, AckStopTaskRequestDto, TaskRecordDto, UpdateTaskRequestDto,
};

#[derive(Debug, Clone)]
pub struct TaskExecutorOptions {
    pub server_name: String,
    pub current_task_id: String,
}

#[derive(Clone)]
pub struct TaskExecutorService {
    tools: HashMap<String, Tool>,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
    handler: ToolHandler,
}

type ToolHandler = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

impl TaskExecutorService {
    pub fn new(opts: TaskExecutorOptions) -> Result<Self, String> {
        let mut service = Self {
            tools: HashMap::new(),
        };
        service.register_list_tasks(opts.current_task_id.as_str());
        service.register_create_tasks(opts.current_task_id.as_str());
        service.register_get_contact_builtin_mcp_grants(opts.current_task_id.as_str());
        service.register_list_contact_runtime_assets(opts.current_task_id.as_str());
        service.register_get_current_task(opts.current_task_id.as_str());
        service.register_complete_current_task(
            opts.server_name.as_str(),
            opts.current_task_id.as_str(),
        );
        service
            .register_fail_current_task(opts.server_name.as_str(), opts.current_task_id.as_str());
        service.register_ack_pause_request(opts.current_task_id.as_str());
        service.register_ack_stop_request(opts.current_task_id.as_str());
        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema,
                })
            })
            .collect()
    }

    pub fn call_tool(&self, name: &str, args: Value) -> Result<Value, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args)
    }

    fn register_tool(
        &mut self,
        name: &str,
        description: &str,
        input_schema: Value,
        handler: ToolHandler,
    ) {
        self.tools.insert(
            name.to_string(),
            Tool {
                name: name.to_string(),
                description: description.to_string(),
                input_schema,
                handler,
            },
        );
    }

    fn register_list_tasks(&mut self, current_task_id: &str) {
        let bound_task_id = current_task_id.trim().to_string();
        self.register_tool(
            "list_tasks",
            "List tasks in the current contact scope while executing the current task.",
            json!({
                "type": "object",
                "properties": {
                    "include_done": { "type": "boolean" },
                    "current_turn_only": { "type": "boolean" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let include_done = args
                    .get("include_done")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let current_turn_only = args
                    .get("current_turn_only")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let limit = args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(20)
                    .clamp(1, 200) as usize;
                let task = load_task(bound_task_id.as_str())?;
                let session_id = required_task_session_id(&task)?;
                let turn_scope = if current_turn_only {
                    task.conversation_turn_id
                        .as_deref()
                        .and_then(trimmed_non_empty)
                } else {
                    None
                };
                let tasks = block_on_result(list_tasks_for_context(
                    session_id.as_str(),
                    turn_scope,
                    include_done,
                    limit,
                ))?;
                Ok(text_result(json!({
                    "task_id": task.id,
                    "session_id": session_id,
                    "count": tasks.len(),
                    "tasks": tasks,
                })))
            }),
        );
    }

    fn register_create_tasks(&mut self, current_task_id: &str) {
        let bound_task_id = current_task_id.trim().to_string();
        let execution_result_contract_schema = json!({
            "type": "object",
            "properties": {
                "result_required": { "type": "boolean" },
                "preferred_format": { "type": "string" }
            },
            "additionalProperties": false
        });
        self.register_tool(
            "create_tasks",
            "Create follow-up tasks directly during task execution. New tasks still start in pending_confirm. Only use the simplified fields exposed in this schema. Prefer required_builtin_capabilities and required_context_assets; the server will map them into internal task planning fields and auto-pass current task runtime when possible.",
            json!({
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "task_ref": { "type": "string" },
                                "task_kind": {
                                    "type": "string",
                                    "enum": ["analysis", "implementation", "verification", "documentation", "delivery", "migration", "research"]
                                },
                                "depends_on_refs": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "verification_of_refs": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "acceptance_criteria": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "title": { "type": "string" },
                                "details": { "type": "string" },
                                "priority": { "type": "string", "enum": ["high", "medium", "low"] },
                                "tags": { "type": "array", "items": { "type": "string" } },
                                "due_at": { "type": "string" },
                                "required_builtin_capabilities": {
                                    "type": "array",
                                    "items": { "type": "string", "enum": ["read", "write", "terminal", "remote", "notepad", "ui_prompter"] }
                                },
                                "required_context_assets": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "asset_type": { "type": "string", "enum": ["skill", "plugin", "common"] },
                                            "asset_ref": { "type": "string" }
                                        },
                                        "required": ["asset_type", "asset_ref"],
                                        "additionalProperties": false
                                    }
                                },
                                "execution_result_contract": execution_result_contract_schema
                            },
                            "required": ["title"],
                            "additionalProperties": false
                        }
                    },
                    "task_ref": { "type": "string" },
                    "task_kind": {
                        "type": "string",
                        "enum": ["analysis", "implementation", "verification", "documentation", "delivery", "migration", "research"]
                    },
                    "depends_on_refs": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "verification_of_refs": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "acceptance_criteria": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "title": { "type": "string" },
                    "details": { "type": "string" },
                    "priority": { "type": "string", "enum": ["high", "medium", "low"] },
                    "required_builtin_capabilities": {
                        "type": "array",
                        "items": { "type": "string", "enum": ["read", "write", "terminal", "remote", "notepad", "ui_prompter"] }
                    },
                    "required_context_assets": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "asset_type": { "type": "string", "enum": ["skill", "plugin", "common"] },
                                "asset_ref": { "type": "string" }
                            },
                            "required": ["asset_type", "asset_ref"],
                            "additionalProperties": false
                        }
                    },
                    "execution_result_contract": execution_result_contract_schema
                },
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let draft_tasks = parse_task_drafts(&args)?;
                if draft_tasks.is_empty() {
                    return Err("tasks is required".to_string());
                }

                let task = load_task(bound_task_id.as_str())?;
                let session_id = required_task_session_id(&task)?;
                let conversation_turn_id = task
                    .conversation_turn_id
                    .as_deref()
                    .and_then(trimmed_non_empty)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| format!("task-exec-{}", task.id));
                let tasks = block_on_result(create_tasks_for_turn(
                    session_id.as_str(),
                    conversation_turn_id.as_str(),
                    draft_tasks,
                ))?;
                Ok(text_result(json!({
                    "created_count": tasks.len(),
                    "tasks": tasks,
                    "session_id": session_id,
                    "conversation_turn_id": conversation_turn_id,
                    "source_task_id": task.id,
                })))
            }),
        );
    }

    fn register_get_contact_builtin_mcp_grants(&mut self, current_task_id: &str) {
        let bound_task_id = current_task_id.trim().to_string();
        self.register_tool(
            "get_contact_builtin_mcp_grants",
            "Return builtin MCP ids that this contact is authorized to use for future task execution.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            Arc::new(move |_args| {
                let scope = load_scope_for_current_task(bound_task_id.as_str())?;
                let contact = block_on_result(memory_server_client::resolve_memory_contact(
                    Some(scope.user_id.as_str()),
                    scope.contact_id.as_deref(),
                    Some(scope.contact_agent_id.as_str()),
                ))?;
                Ok(text_result(json!({
                    "contact_id": scope.contact_id,
                    "contact_agent_id": scope.contact_agent_id,
                    "authorized_builtin_mcp_ids": contact
                        .map(|item| item.authorized_builtin_mcp_ids)
                        .unwrap_or_default(),
                })))
            }),
        );
    }

    fn register_list_contact_runtime_assets(&mut self, current_task_id: &str) {
        let bound_task_id = current_task_id.trim().to_string();
        self.register_tool(
            "list_contact_runtime_assets",
            "List current contact runtime assets that can be attached to a task, including skills, plugins, and commons.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            Arc::new(move |_args| {
                let scope = load_scope_for_current_task(bound_task_id.as_str())?;
                let runtime_context = block_on_result(
                    memory_server_client::get_memory_agent_runtime_context(
                        scope.contact_agent_id.as_str(),
                    ),
                )?
                .ok_or_else(|| {
                    format!(
                        "agent runtime context not found: {}",
                        scope.contact_agent_id
                    )
                })?;

                Ok(text_result(json!({
                    "contact_agent_id": runtime_context.agent_id,
                    "skills": runtime_context.runtime_skills.into_iter().map(|item| json!({
                        "asset_type": "skill",
                        "asset_id": item.id,
                        "display_name": item.name,
                        "source_type": item.source_type,
                        "source_path": item.source_path,
                        "description": item.description,
                        "plugin_source": item.plugin_source,
                    })).collect::<Vec<_>>(),
                    "plugins": runtime_context.runtime_plugins.into_iter().map(|item| json!({
                        "asset_type": "plugin",
                        "asset_id": item.source,
                        "display_name": item.name,
                        "description": item.description,
                        "category": item.category,
                    })).collect::<Vec<_>>(),
                    "commons": runtime_context.runtime_commands.into_iter().map(|item| json!({
                        "asset_type": "common",
                        "asset_id": item.command_ref,
                        "display_name": item.name,
                        "source_type": "runtime_command",
                        "source_path": item.source_path,
                        "description": item.description,
                        "plugin_source": item.plugin_source,
                        "argument_hint": item.argument_hint,
                    })).collect::<Vec<_>>(),
                })))
            }),
        );
    }

    fn register_get_current_task(&mut self, current_task_id: &str) {
        let bound_task_id = current_task_id.trim().to_string();
        self.register_tool(
            "get_current_task",
            "Return the current task being executed.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            Arc::new(move |_args| {
                let task = load_task(bound_task_id.as_str())?;
                Ok(text_result(json!({
                    "task": task,
                })))
            }),
        );
    }

    fn register_complete_current_task(&mut self, server_name: &str, current_task_id: &str) {
        let bound_task_id = current_task_id.trim().to_string();
        self.register_tool(
            "complete_current_task",
            &format!(
                "Mark the current task as completed. A non-empty result is required (server: {}).",
                server_name
            ),
            json!({
                "type": "object",
                "properties": {
                    "result": { "type": "string" }
                },
                "required": ["result"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let result = required_string(&args, "result")?;
                let task = block_on_result(task_service_client::update_task_internal(
                    bound_task_id.as_str(),
                    &UpdateTaskRequestDto {
                        status: Some("completed".to_string()),
                        result_summary: Some(Some(result.clone())),
                        result_message_id: Some(None),
                        last_error: Some(None),
                        ..UpdateTaskRequestDto::default()
                    },
                ))?
                .ok_or_else(|| "task not found".to_string())?;
                let _ = block_on_result(async {
                    publish_task_runtime_update_best_effort(&task).await;
                    Ok::<(), String>(())
                });
                Ok(text_result(json!({
                    "task": task,
                    "result": result,
                })))
            }),
        );
    }

    fn register_fail_current_task(&mut self, server_name: &str, current_task_id: &str) {
        let bound_task_id = current_task_id.trim().to_string();
        self.register_tool(
            "fail_current_task",
            &format!(
                "Mark the current task as failed. A non-empty result is required (server: {}).",
                server_name
            ),
            json!({
                "type": "object",
                "properties": {
                    "result": { "type": "string" }
                },
                "required": ["result"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let result = required_string(&args, "result")?;
                let task = block_on_result(task_service_client::update_task_internal(
                    bound_task_id.as_str(),
                    &UpdateTaskRequestDto {
                        status: Some("failed".to_string()),
                        result_summary: Some(Some(result.clone())),
                        result_message_id: Some(None),
                        last_error: Some(Some(result.clone())),
                        ..UpdateTaskRequestDto::default()
                    },
                ))?
                .ok_or_else(|| "task not found".to_string())?;
                let _ = block_on_result(async {
                    publish_task_runtime_update_best_effort(&task).await;
                    Ok::<(), String>(())
                });
                Ok(text_result(json!({
                    "task": task,
                    "result": result,
                })))
            }),
        );
    }

    fn register_ack_pause_request(&mut self, current_task_id: &str) {
        let bound_task_id = current_task_id.trim().to_string();
        self.register_tool(
            "ack_pause_request",
            "Acknowledge that the current running task should pause now. Use this at a safe stopping point and include a checkpoint summary so the task can continue later.",
            json!({
                "type": "object",
                "properties": {
                    "checkpoint_summary": { "type": "string" },
                    "checkpoint_message_id": { "type": "string" }
                },
                "required": ["checkpoint_summary"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let checkpoint_summary = required_string(&args, "checkpoint_summary")?;
                let checkpoint_message_id = args
                    .get("checkpoint_message_id")
                    .and_then(Value::as_str)
                    .and_then(trimmed_non_empty)
                    .map(|value| value.to_string());
                let task = block_on_result(task_service_client::ack_pause_task(
                    bound_task_id.as_str(),
                    &AckPauseTaskRequestDto {
                        checkpoint_summary: Some(checkpoint_summary.clone()),
                        checkpoint_message_id,
                    },
                ))?
                .ok_or_else(|| "task not found".to_string())?;
                let _ = block_on_result(async {
                    publish_task_runtime_update_best_effort(&task).await;
                    Ok::<(), String>(())
                });
                Ok(text_result(json!({
                    "task": task,
                    "checkpoint_summary": checkpoint_summary,
                })))
            }),
        );
    }

    fn register_ack_stop_request(&mut self, current_task_id: &str) {
        let bound_task_id = current_task_id.trim().to_string();
        self.register_tool(
            "ack_stop_request",
            "Acknowledge that the current running task should stop now. Use this at a safe stopping point and return the partial result or stop reason.",
            json!({
                "type": "object",
                "properties": {
                    "result": { "type": "string" },
                    "last_error": { "type": "string" }
                },
                "required": ["result"],
                "additionalProperties": false
            }),
            Arc::new(move |args| {
                let result = required_string(&args, "result")?;
                let last_error = args
                    .get("last_error")
                    .and_then(Value::as_str)
                    .and_then(trimmed_non_empty)
                    .map(|value| value.to_string());
                let task = block_on_result(task_service_client::ack_stop_task(
                    bound_task_id.as_str(),
                    &AckStopTaskRequestDto {
                        result_summary: Some(result.clone()),
                        result_message_id: None,
                        last_error,
                    },
                ))?
                .ok_or_else(|| "task not found".to_string())?;
                let _ = block_on_result(async {
                    publish_task_runtime_update_best_effort(&task).await;
                    Ok::<(), String>(())
                });
                Ok(text_result(json!({
                    "task": task,
                    "result": result,
                })))
            }),
        );
    }
}

fn required_string(args: &Value, field: &str) -> Result<String, String> {
    let value = args
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{field} is required"))?;
    Ok(value.to_string())
}

fn load_task(task_id: &str) -> Result<TaskRecordDto, String> {
    let task_id = task_id.trim();
    if task_id.is_empty() {
        return Err("current task id is required".to_string());
    }
    block_on_result(task_service_client::get_task(task_id))?
        .ok_or_else(|| "task not found".to_string())
}

fn required_task_session_id(task: &TaskRecordDto) -> Result<String, String> {
    task.session_id
        .as_deref()
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string())
        .ok_or_else(|| format!("task {} missing session_id", task.id))
}

fn load_scope_for_current_task(
    current_task_id: &str,
) -> Result<crate::services::task_manager::TaskScopeContext, String> {
    let task = load_task(current_task_id)?;
    let session_id = required_task_session_id(&task)?;
    block_on_result(resolve_task_scope_context(session_id.as_str()))
}
