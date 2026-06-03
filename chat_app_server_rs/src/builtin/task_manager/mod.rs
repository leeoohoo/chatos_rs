mod parsing;
mod review_flow;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use crate::core::async_bridge::{block_on_option, block_on_result};
use crate::core::mcp_tools::ToolStreamChunkCallback;
use crate::core::tool_io::text_result;
use crate::core::tool_registry::ToolRegistry;
use crate::modules::conversation_runtime::task_board::refresh_task_board_runtime_outcome;
use crate::services::task_manager::{
    complete_task_by_id, delete_task_by_id, list_tasks_for_context, update_task_by_id,
};
use serde_json::{json, Value};

use self::parsing::{parse_update_patch, required_string_arg, trimmed_non_empty};
use self::review_flow::handle_add_task;

#[derive(Debug, Clone)]
pub struct TaskManagerOptions {
    pub server_name: String,
    pub review_timeout_ms: u64,
    pub auto_create_task: bool,
}

#[derive(Clone)]
pub struct TaskManagerService {
    registry: ToolRegistry<ToolHandler>,
    auto_create_task: bool,
}

type ToolHandler = Arc<dyn Fn(Value, &ToolContext) -> Result<Value, String> + Send + Sync>;

pub(super) struct ToolContext<'a> {
    conversation_id: &'a str,
    conversation_turn_id: &'a str,
    auto_create_task: bool,
    on_stream_chunk: Option<ToolStreamChunkCallback>,
}

impl TaskManagerService {
    pub fn new(opts: TaskManagerOptions) -> Result<Self, String> {
        let mut service = Self {
            registry: ToolRegistry::new(),
            auto_create_task: opts.auto_create_task,
        };

        let add_timeout = opts.review_timeout_ms.max(10_000);
        let auto_create_task = opts.auto_create_task;
        let server_name = opts.server_name;

        service.register_add_task(add_timeout, server_name.as_str(), auto_create_task);
        service.register_list_tasks();
        service.register_update_task();
        service.register_complete_task();
        service.register_delete_task();

        Ok(service)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        self.registry.list_tools()
    }

    pub fn call_tool(
        &self,
        name: &str,
        args: Value,
        conversation_id: Option<&str>,
        conversation_turn_id: Option<&str>,
        on_stream_chunk: Option<ToolStreamChunkCallback>,
    ) -> Result<Value, String> {
        let tool = self
            .registry
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;

        let conversation = conversation_id
            .and_then(trimmed_non_empty)
            .ok_or_else(|| "task_manager requires an active conversation_id".to_string())?;
        let turn = conversation_turn_id
            .and_then(trimmed_non_empty)
            .ok_or_else(|| "task_manager requires an active conversation_turn_id".to_string())?;

        let ctx = ToolContext {
            conversation_id: conversation,
            conversation_turn_id: turn,
            auto_create_task: self.auto_create_task,
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
        self.registry
            .register_tool(name, description, input_schema, handler);
    }

    fn register_add_task(&mut self, add_timeout: u64, server_name: &str, auto_create_task: bool) {
        let description = if auto_create_task {
            format!(
                "Create one or more tasks for the current conversation turn. Task drafts will be persisted automatically without user confirmation (server: {server_name})."
            )
        } else {
            format!(
                "Create one or more tasks for the current conversation turn. The task list must be confirmed by the user before persistence (server: {server_name})."
            )
        };
        self.register_tool(
            "add_task",
            &description,
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
                                "due_at": { "type": "string" },
                                "outcome_summary": { "type": "string" },
                                "outcome_items": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "kind": { "type": "string" },
                                            "text": { "type": "string" },
                                            "importance": { "type": "string", "enum": ["high", "medium", "low"] },
                                            "refs": { "type": "array", "items": { "type": "string" } }
                                        },
                                        "required": ["text"],
                                        "additionalProperties": false
                                    }
                                },
                                "resume_hint": { "type": "string" },
                                "blocker_reason": { "type": "string" },
                                "blocker_needs": { "type": "array", "items": { "type": "string" } },
                                "blocker_kind": {
                                    "type": "string",
                                    "enum": ["external_dependency", "permission", "missing_information", "design_decision", "environment_failure", "upstream_bug", "unknown"]
                                }
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
                    "due_at": { "type": "string" },
                    "outcome_summary": { "type": "string" },
                    "outcome_items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "kind": { "type": "string" },
                                "text": { "type": "string" },
                                "importance": { "type": "string", "enum": ["high", "medium", "low"] },
                                "refs": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["text"],
                            "additionalProperties": false
                        }
                    },
                    "resume_hint": { "type": "string" },
                    "blocker_reason": { "type": "string" },
                    "blocker_needs": { "type": "array", "items": { "type": "string" } },
                    "blocker_kind": {
                        "type": "string",
                        "enum": ["external_dependency", "permission", "missing_information", "design_decision", "environment_failure", "upstream_bug", "unknown"]
                    }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args, ctx| handle_add_task(args, ctx, add_timeout)),
        );
    }

    fn register_list_tasks(&mut self) {
        self.register_tool(
            "list_tasks",
            "List tasks in the current conversation. Optionally scope to the current conversation turn.",
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
                    ctx.conversation_id,
                    turn_scope,
                    include_done,
                    limit,
                ))?;

                Ok(text_result(json!({
                    "conversation_id": ctx.conversation_id,
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
            "Update a task in current conversation by task_id. Provide changes as a JSON string (example: {\"status\":\"doing\"}). When setting status=blocked, include outcome_summary and blocker_reason whenever possible.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "changes": {
                        "type": "string",
                        "description": "JSON object string. Allowed keys: title, details (or description), priority, status, tags, due_at (or dueAt), outcome_summary, outcome_items, resume_hint, blocker_reason, blocker_needs, blocker_kind, completed_at, last_outcome_at."
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
                    block_on_result(update_task_by_id(ctx.conversation_id, task_id.as_str(), patch))?;
                emit_task_board_refresh(ctx);
                Ok(text_result(json!({
                    "updated": true,
                    "task": task,
                    "conversation_id": ctx.conversation_id,
                })))
            }),
        );
    }

    fn register_complete_task(&mut self) {
        self.register_tool(
            "complete_task",
            "Mark a task as done in current conversation by task_id. Prefer providing outcome_summary and key findings so later tasks can reuse them.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "outcome_summary": { "type": "string" },
                    "outcome_items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "kind": { "type": "string" },
                                "text": { "type": "string" },
                                "importance": { "type": "string", "enum": ["high", "medium", "low"] },
                                "refs": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["text"],
                            "additionalProperties": false
                        }
                    },
                    "resume_hint": { "type": "string" }
                },
                "required": ["task_id"],
                "additionalProperties": false
            }),
            Arc::new(move |args, ctx| {
                let task_id = required_string_arg(&args, "task_id")?;
                let mut patch_args = args
                    .as_object()
                    .cloned()
                    .ok_or_else(|| "complete_task payload must be an object".to_string())?;
                patch_args.remove("task_id");
                let patch = if patch_args.is_empty() {
                    None
                } else {
                    Some(parse_update_patch(&Value::Object(patch_args))?)
                };
                let task =
                    block_on_result(complete_task_by_id(ctx.conversation_id, task_id.as_str(), patch))?;
                emit_task_board_refresh(ctx);
                Ok(text_result(json!({
                    "completed": true,
                    "task": task,
                    "conversation_id": ctx.conversation_id,
                })))
            }),
        );
    }

    fn register_delete_task(&mut self) {
        self.register_tool(
            "delete_task",
            "Delete a task in current conversation by task_id.",
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
                let deleted =
                    block_on_result(delete_task_by_id(ctx.conversation_id, task_id.as_str()))?;
                Ok(text_result(json!({
                    "deleted": deleted,
                    "task_id": task_id,
                    "reason": if deleted {
                        Value::Null
                    } else {
                        Value::String(crate::services::task_manager::TASK_NOT_FOUND_ERR.to_string())
                    },
                    "conversation_id": ctx.conversation_id,
                })))
            }),
        );
    }
}

fn emit_task_board_refresh(ctx: &ToolContext<'_>) {
    let Some(outcome) = block_on_option(refresh_task_board_runtime_outcome(
        ctx.conversation_id,
        ctx.conversation_turn_id,
    )) else {
        return;
    };

    let Some(callback) = ctx.on_stream_chunk.as_ref() else {
        return;
    };
    if let Ok(serialized) = serde_json::to_string(&outcome.updated_event) {
        callback(serialized);
    }
}
