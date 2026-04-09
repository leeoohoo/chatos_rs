mod context;
pub(crate) mod parsing;
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
use crate::services::contact_agent_model::{
    normalize_optional_model_id, resolve_effective_contact_agent_model_config_id,
};
use crate::services::im_task_runtime_bridge::publish_task_runtime_update_best_effort;
use crate::services::memory_server_client;
use crate::services::task_manager::{list_tasks_for_context, resolve_task_scope_context};
use crate::services::task_service_client::{
    self, ConfirmTaskRequestDto, PauseTaskRequestDto, ResumeTaskRequestDto, StopTaskRequestDto,
    UpdateTaskRequestDto,
};

use self::context::ToolContext;
use self::parsing::trimmed_non_empty;
use self::review_flow::handle_create_tasks;

#[derive(Debug, Clone)]
pub struct TaskPlannerOptions {
    pub server_name: String,
    pub review_timeout_ms: u64,
}

#[derive(Clone)]
pub struct TaskPlannerService {
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

impl TaskPlannerService {
    pub fn new(opts: TaskPlannerOptions) -> Result<Self, String> {
        let mut service = Self {
            tools: HashMap::new(),
            default_session_id: format!("session_{}", Uuid::new_v4().simple()),
            default_turn_id: format!("turn_{}", Uuid::new_v4().simple()),
        };

        let add_timeout = opts.review_timeout_ms.max(10_000);
        let server_name = opts.server_name;
        service.register_list_tasks();
        service.register_create_tasks(add_timeout, server_name.as_str());
        service.register_confirm_task();
        service.register_request_pause_running_task();
        service.register_request_stop_running_task();
        service.register_resume_task();
        service.register_get_contact_builtin_mcp_grants();
        service.register_list_contact_runtime_assets();
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

    fn register_list_tasks(&mut self) {
        self.register_tool(
            "list_tasks",
            "List tasks in the current contact conversation scope.",
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

    fn register_create_tasks(&mut self, add_timeout: u64, server_name: &str) {
        let capability_tokens =
            crate::services::task_capability_registry::planning_task_capability_tokens();
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
            &format!(
                "Create one or more tasks for the current conversation turn. New tasks always start in pending_confirm. Only use the simplified fields exposed in this schema. Prefer required_builtin_capabilities and required_context_assets; the server will map them into internal task planning fields and auto-pass current turn runtime, selected commons, project_root, remote_connection_id, and related planning context when possible. For implementation/migration/documentation tasks that modify files, required_builtin_capabilities must include write; if commands/build/tests/logs are needed, also include terminal (server: {server_name})."
            ),
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
                                    "items": { "type": "string", "enum": capability_tokens.clone() }
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
                        "items": { "type": "string", "enum": capability_tokens }
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
            Arc::new(move |args, ctx| handle_create_tasks(args, ctx, add_timeout)),
        );
    }

    fn register_get_contact_builtin_mcp_grants(&mut self) {
        self.register_tool(
            "get_contact_builtin_mcp_grants",
            "Return builtin MCP ids that this contact is authorized to use for future task execution.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            Arc::new(move |_args, ctx| {
                let scope = block_on_result(resolve_task_scope_context(ctx.session_id))?;
                let contact = block_on_result(memory_server_client::resolve_memory_contact(
                    Some(scope.user_id.as_str()),
                    scope.contact_id.as_deref(),
                    Some(scope.contact_agent_id.as_str()),
                ))?;
                Ok(text_result(json!({
                    "session_id": ctx.session_id,
                    "contact_id": scope.contact_id,
                    "contact_agent_id": scope.contact_agent_id,
                    "authorized_builtin_mcp_ids": contact
                        .map(|item| item.authorized_builtin_mcp_ids)
                        .unwrap_or_default(),
                })))
            }),
        );
    }

    fn register_confirm_task(&mut self) {
        self.register_tool(
            "confirm_task",
            "Confirm a pending task in the current contact scope so it can move from pending_confirm to pending_execute.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "note": { "type": "string" }
                },
                "required": ["task_id"],
                "additionalProperties": false
            }),
            Arc::new(move |args, ctx| {
                let task_id = args
                    .get("task_id")
                    .and_then(Value::as_str)
                    .and_then(trimmed_non_empty)
                    .ok_or_else(|| "task_id is required".to_string())?;
                let note = args
                    .get("note")
                    .and_then(Value::as_str)
                    .and_then(trimmed_non_empty)
                    .map(|value| value.to_string());
                let scope = block_on_result(resolve_task_scope_context(ctx.session_id))?;
                let existing = block_on_result(task_service_client::get_task(task_id))?
                    .ok_or_else(|| format!("task not found: {}", task_id))?;

                if existing.user_id != scope.user_id
                    || existing.contact_agent_id != scope.contact_agent_id
                    || existing.project_id != scope.project_id
                {
                    return Err(format!(
                        "task {} is not in the current contact scope",
                        task_id
                    ));
                }
                if existing.status != "pending_confirm" {
                    return Err(format!(
                        "task {} status={} cannot be confirmed again; create a new task for new work",
                        task_id, existing.status
                    ));
                }

                let effective_model_id = block_on_result(
                    resolve_effective_contact_agent_model_config_id(
                        scope.contact_agent_id.as_str(),
                    ),
                )?;
                if normalize_optional_model_id(existing.model_config_id.clone()).is_none()
                    && effective_model_id.is_some()
                {
                    block_on_result(task_service_client::update_task(
                        task_id,
                        &UpdateTaskRequestDto {
                            model_config_id: Some(effective_model_id.clone()),
                            ..UpdateTaskRequestDto::default()
                        },
                    ))?;
                }

                let task = block_on_result(task_service_client::confirm_task(
                    task_id,
                    &ConfirmTaskRequestDto {
                        user_id: Some(scope.user_id.clone()),
                        note,
                    },
                ))?
                .ok_or_else(|| format!("task not found: {}", task_id))?;
                let _ = block_on_result(async {
                    publish_task_runtime_update_best_effort(&task).await;
                    Ok::<(), String>(())
                });

                Ok(text_result(json!({
                    "task_id": task.id,
                    "status": task.status,
                    "confirmed_at": task.confirmed_at,
                    "task": task,
                })))
            }),
        );
    }

    fn register_list_contact_runtime_assets(&mut self) {
        self.register_tool(
            "list_contact_runtime_assets",
            "List current contact runtime assets that can be attached to a task, including skills, plugins, and commons.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            Arc::new(move |_args, ctx| {
                let scope = block_on_result(resolve_task_scope_context(ctx.session_id))?;
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

    fn register_request_pause_running_task(&mut self) {
        self.register_tool(
            "request_pause_running_task",
            "Request the currently running task in this contact scope to pause at the next safe point. Use this when the task should continue later, not be cancelled.",
            json!({
                "type": "object",
                "properties": {
                    "reason": { "type": "string" }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args, ctx| {
                let reason = args
                    .get("reason")
                    .and_then(Value::as_str)
                    .and_then(trimmed_non_empty)
                    .map(|value| value.to_string());
                let scope = block_on_result(resolve_task_scope_context(ctx.session_id))?;
                let running_tasks = block_on_result(task_service_client::list_tasks(
                    Some(scope.user_id.as_str()),
                    Some(scope.contact_agent_id.as_str()),
                    Some(scope.project_id.as_str()),
                    None,
                    None,
                    Some("running"),
                    Some(10),
                    0,
                ))?;
                let task = running_tasks
                    .into_iter()
                    .next()
                    .ok_or_else(|| "no running task found in the current contact scope".to_string())?;
                let updated = block_on_result(task_service_client::request_pause_task(
                    task.id.as_str(),
                    &PauseTaskRequestDto {
                        user_id: Some(scope.user_id.clone()),
                        reason: reason.clone(),
                    },
                ))?
                .ok_or_else(|| "task not found".to_string())?;
                Ok(text_result(json!({
                    "requested": true,
                    "task_id": updated.id,
                    "status": updated.status,
                    "reason": reason,
                })))
            }),
        );
    }

    fn register_request_stop_running_task(&mut self) {
        self.register_tool(
            "request_stop_running_task",
            "Request the currently running task in this contact scope to stop at the next safe point. Use this when the current task should no longer continue.",
            json!({
                "type": "object",
                "properties": {
                    "reason": { "type": "string" }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args, ctx| {
                let reason = args
                    .get("reason")
                    .and_then(Value::as_str)
                    .and_then(trimmed_non_empty)
                    .map(|value| value.to_string());
                let scope = block_on_result(resolve_task_scope_context(ctx.session_id))?;
                let running_tasks = block_on_result(task_service_client::list_tasks(
                    Some(scope.user_id.as_str()),
                    Some(scope.contact_agent_id.as_str()),
                    Some(scope.project_id.as_str()),
                    None,
                    None,
                    Some("running"),
                    Some(10),
                    0,
                ))?;
                let task = running_tasks
                    .into_iter()
                    .next()
                    .ok_or_else(|| "no running task found in the current contact scope".to_string())?;
                let updated = block_on_result(task_service_client::request_stop_task(
                    task.id.as_str(),
                    &StopTaskRequestDto {
                        user_id: Some(scope.user_id.clone()),
                        reason: reason.clone(),
                    },
                ))?
                .ok_or_else(|| "task not found".to_string())?;
                Ok(text_result(json!({
                    "requested": true,
                    "task_id": updated.id,
                    "status": updated.status,
                    "reason": reason,
                })))
            }),
        );
    }

    fn register_resume_task(&mut self) {
        self.register_tool(
            "resume_task",
            "Resume a paused task in the current contact scope so it can return to pending_execute and be scheduled again.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "note": { "type": "string" }
                },
                "required": ["task_id"],
                "additionalProperties": false
            }),
            Arc::new(move |args, ctx| {
                let task_id = args
                    .get("task_id")
                    .and_then(Value::as_str)
                    .and_then(trimmed_non_empty)
                    .ok_or_else(|| "task_id is required".to_string())?;
                let note = args
                    .get("note")
                    .and_then(Value::as_str)
                    .and_then(trimmed_non_empty)
                    .map(|value| value.to_string());
                let scope = block_on_result(resolve_task_scope_context(ctx.session_id))?;
                let existing = block_on_result(task_service_client::get_task(task_id))?
                    .ok_or_else(|| format!("task not found: {}", task_id))?;
                if existing.user_id != scope.user_id
                    || existing.contact_agent_id != scope.contact_agent_id
                    || existing.project_id != scope.project_id
                {
                    return Err(format!(
                        "task {} is not in the current contact scope",
                        task_id
                    ));
                }
                let task = block_on_result(task_service_client::resume_task(
                    task_id,
                    &ResumeTaskRequestDto {
                        user_id: Some(scope.user_id.clone()),
                        note,
                    },
                ))?
                .ok_or_else(|| format!("task not found: {}", task_id))?;
                let _ = block_on_result(async {
                    publish_task_runtime_update_best_effort(&task).await;
                    Ok::<(), String>(())
                });
                Ok(text_result(json!({
                    "task_id": task.id,
                    "status": task.status,
                    "task": task,
                })))
            }),
        );
    }
}
