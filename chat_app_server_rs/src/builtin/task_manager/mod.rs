mod parsing;
mod review_flow;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::core::async_bridge::block_on_result;
use crate::core::mcp_tools::ToolStreamChunkCallback;
use crate::core::tool_io::text_result;
use crate::services::task_manager::{
    complete_task_by_id, delete_task_by_id, list_tasks_for_context, update_task_by_id,
};

use self::parsing::{parse_update_patch, required_string_arg, trimmed_non_empty};
use self::review_flow::handle_add_task;

#[derive(Debug, Clone)]
pub struct TaskManagerOptions {
    pub server_name: String,
    pub review_timeout_ms: u64,
}

#[derive(Clone)]
pub struct TaskManagerService {
    tools: HashMap<String, Tool>,
    default_session_id: String,
    default_turn_id: String,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
    handler: ToolHandler,
}

type ToolHandler = Arc<dyn Fn(Value, &ToolContext) -> Result<Value, String> + Send + Sync>;

pub(super) struct ToolContext<'a> {
    session_id: &'a str,
    conversation_turn_id: &'a str,
    on_stream_chunk: Option<ToolStreamChunkCallback>,
}

impl TaskManagerService {
    pub fn new(opts: TaskManagerOptions) -> Result<Self, String> {
        let mut service = Self {
            tools: HashMap::new(),
            default_session_id: format!("session_{}", Uuid::new_v4().simple()),
            default_turn_id: format!("turn_{}", Uuid::new_v4().simple()),
        };

        let add_timeout = opts.review_timeout_ms.max(10_000);
        let server_name = opts.server_name;

        service.register_add_task(add_timeout, server_name.as_str());
        service.register_list_tasks();
        service.register_update_task();
        service.register_complete_task();
        service.register_delete_task();

        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema
                })
            })
            .collect()
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        session_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;

        let session = session_id
            .and_then(trimmed_non_empty)
            .unwrap_or(self.default_session_id.as_str());
        let turn = conversation_turn_id
            .and_then(trimmed_non_empty)
            .unwrap_or(self.default_turn_id.as_str());

        let ctx = ToolContext {
            session_id: session,
            conversation_turn_id: turn,
            on_stream_chunk,
        };
        (tool.handler)(args, &ctx)
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

    fn register_add_task(&mut self, add_timeout: u64, server_name: &str) {
        self.register_tool(
            "add_task",
            &format!(
                "Create one or more tasks for the current conversation turn. The task list must be confirmed by the user before persistence (server: {server_name})."
            ),
            json!({
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string" },
                                "details": { "type": "string" },
                                "priority": { "type": "string", "enum": ["high", "medium", "low"] },
                                "status": { "type": "string", "enum": ["todo", "doing", "blocked", "done"] },
                                "tags": { "type": "array", "items": { "type": "string" } },
                                "due_at": { "type": "string" }
                            },
                            "required": ["title"],
                            "additionalProperties": false
                        }
                    },
                    "title": { "type": "string" },
                    "details": { "type": "string" },
                    "priority": { "type": "string", "enum": ["high", "medium", "low"] },
                    "status": { "type": "string", "enum": ["todo", "doing", "blocked", "done"] },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "due_at": { "type": "string" }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args, ctx| handle_add_task(args, ctx, add_timeout)),
        );
    }

    fn register_list_tasks(&mut self) {
        self.register_tool(
            "list_tasks",
            "List tasks in the current session. Optionally scope to the current conversation turn.",
            json!({
                "type": "object",
                "properties": {
                    "include_done": { "type": "boolean" },
                    "current_turn_only": { "type": "boolean" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args, ctx| {
                let include_done = args
                    .get("include_done")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                let current_turn_only = args
                    .get("current_turn_only")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                let limit = args
                    .get("limit")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(20)
                    .clamp(1, 200) as usize;

                let turn_scope = if current_turn_only {
                    Some(ctx.conversation_turn_id)
                } else {
                    None
                };

                let tasks = block_on_result(list_tasks_for_context(
                    ctx.session_id,
                    turn_scope,
                    include_done,
                    limit,
                ))?;

                Ok(text_result(json!({
                    "session_id": ctx.session_id,
                    "conversation_turn_id": if current_turn_only {
                        Value::String(ctx.conversation_turn_id.to_string())
                    } else {
                        Value::Null
                    },
                    "count": tasks.len(),
                    "tasks": tasks,
                })))
            }),
        );
    }

    fn register_update_task(&mut self) {
        self.register_tool(
            "update_task",
            "Update a task in current session by task_id. Provide changes as a JSON string (example: {\"status\":\"doing\"}).",
            json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "changes": {
                        "type": "string",
                        "description": "JSON object string. Allowed keys: title, details (or description), priority, status, tags, due_at (or dueAt)."
                    }
                },
                "required": ["task_id", "changes"],
                "additionalProperties": false
            }),
            Arc::new(move |args, ctx| {
                let task_id = required_string_arg(&args, "task_id")?;
                let changes = args
                    .get("changes")
                    .ok_or_else(|| "changes is required".to_string())?;
                let patch = parse_update_patch(changes)?;
                let task =
                    block_on_result(update_task_by_id(ctx.session_id, task_id.as_str(), patch))?;
                Ok(text_result(json!({
                    "updated": true,
                    "task": task,
                    "session_id": ctx.session_id,
                })))
            }),
        );
    }

    fn register_complete_task(&mut self) {
        self.register_tool(
            "complete_task",
            "Mark a task as done in current session by task_id.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" }
                },
                "required": ["task_id"],
                "additionalProperties": false
            }),
            Arc::new(move |args, ctx| {
                let task_id = required_string_arg(&args, "task_id")?;
                let task = block_on_result(complete_task_by_id(ctx.session_id, task_id.as_str()))?;
                Ok(text_result(json!({
                    "completed": true,
                    "task": task,
                    "session_id": ctx.session_id,
                })))
            }),
        );
    }

    fn register_delete_task(&mut self) {
        self.register_tool(
            "delete_task",
            "Delete a task in current session by task_id.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" }
                },
                "required": ["task_id"],
                "additionalProperties": false
            }),
            Arc::new(move |args, ctx| {
                let task_id = required_string_arg(&args, "task_id")?;
                let deleted = block_on_result(delete_task_by_id(ctx.session_id, task_id.as_str()))?;
                Ok(text_result(json!({
                    "deleted": deleted,
                    "task_id": task_id,
                    "reason": if deleted {
                        Value::Null
                    } else {
                        Value::String(crate::services::task_manager::TASK_NOT_FOUND_ERR.to_string())
                    },
                    "session_id": ctx.session_id,
                })))
            }),
        );
    }
}
