use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tool_registry::{block_on_option, block_on_result, text_result, ToolRegistry};

mod parsing;
mod schema;

use self::parsing::required_string_arg;
pub use self::parsing::{parse_task_drafts, parse_update_patch, trimmed_non_empty};
use self::schema::{outcome_item_schema, task_payload_schema};

pub const REVIEW_TIMEOUT_MS_DEFAULT: u64 = 86_400_000;
pub const TASK_NOT_FOUND_ERR: &str = "task_not_found";

pub type TaskStreamChunkCallback = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskOutcomeItem {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub importance: Option<String>,
    #[serde(default)]
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDraft {
    pub title: String,
    #[serde(default)]
    pub details: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub outcome_summary: String,
    #[serde(default)]
    pub outcome_items: Vec<TaskOutcomeItem>,
    #[serde(default)]
    pub resume_hint: String,
    #[serde(default)]
    pub blocker_reason: String,
    #[serde(default)]
    pub blocker_needs: Vec<String>,
    #[serde(default)]
    pub blocker_kind: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskUpdatePatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub due_at: Option<Option<String>>,
    #[serde(default)]
    pub outcome_summary: Option<String>,
    #[serde(default)]
    pub outcome_items: Option<Vec<TaskOutcomeItem>>,
    #[serde(default)]
    pub resume_hint: Option<String>,
    #[serde(default)]
    pub blocker_reason: Option<String>,
    #[serde(default)]
    pub blocker_needs: Option<Vec<String>>,
    #[serde(default)]
    pub blocker_kind: Option<String>,
    #[serde(default)]
    pub completed_at: Option<Option<String>>,
    #[serde(default)]
    pub last_outcome_at: Option<Option<String>>,
}

fn default_priority() -> String {
    "medium".to_string()
}

fn default_status() -> String {
    "todo".to_string()
}

#[async_trait]
pub trait TaskManagerStore: Send + Sync {
    async fn create_tasks_for_turn(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
        draft_tasks: Vec<TaskDraft>,
    ) -> Result<Vec<Value>, String>;

    async fn review_and_create_tasks(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
        draft_tasks: Vec<TaskDraft>,
        timeout_ms: u64,
        on_stream_chunk: Option<TaskStreamChunkCallback>,
    ) -> Result<Value, String>;

    async fn list_tasks_for_context(
        &self,
        conversation_id: &str,
        conversation_turn_id: Option<&str>,
        include_done: bool,
        limit: usize,
    ) -> Result<Vec<Value>, String>;

    async fn update_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
        patch: TaskUpdatePatch,
    ) -> Result<Value, String>;

    async fn complete_task_by_id(
        &self,
        conversation_id: &str,
        task_id: &str,
        patch: Option<TaskUpdatePatch>,
    ) -> Result<Value, String>;

    async fn delete_task_by_id(&self, conversation_id: &str, task_id: &str)
        -> Result<bool, String>;

    async fn task_board_updated_event(
        &self,
        conversation_id: &str,
        conversation_turn_id: &str,
    ) -> Option<Value>;
}

#[derive(Clone)]
pub struct TaskManagerStoreRef(Arc<dyn TaskManagerStore>);

impl TaskManagerStoreRef {
    pub fn new(store: Arc<dyn TaskManagerStore>) -> Self {
        Self(store)
    }

    fn inner(&self) -> Arc<dyn TaskManagerStore> {
        self.0.clone()
    }
}

impl std::fmt::Debug for TaskManagerStoreRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskManagerStoreRef")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct TaskManagerOptions {
    pub server_name: String,
    pub review_timeout_ms: u64,
    pub auto_create_task: bool,
    pub store: TaskManagerStoreRef,
}

#[derive(Clone)]
pub struct TaskManagerService {
    registry: ToolRegistry<ToolHandler>,
    auto_create_task: bool,
    store: TaskManagerStoreRef,
}

type ToolHandler = Arc<dyn Fn(Value, &ToolContext) -> Result<Value, String> + Send + Sync>;

struct ToolContext {
    conversation_id: String,
    conversation_turn_id: String,
    auto_create_task: bool,
    on_stream_chunk: Option<TaskStreamChunkCallback>,
    store: TaskManagerStoreRef,
}

impl TaskManagerService {
    pub fn new(opts: TaskManagerOptions) -> Result<Self, String> {
        let mut service = Self {
            registry: ToolRegistry::new(),
            auto_create_task: opts.auto_create_task,
            store: opts.store,
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
        on_stream_chunk: Option<TaskStreamChunkCallback>,
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
            conversation_id: conversation.to_string(),
            conversation_turn_id: turn.to_string(),
            auto_create_task: self.auto_create_task,
            on_stream_chunk,
            store: self.store.clone(),
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
            task_payload_schema(),
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
                    Some(ctx.conversation_turn_id.as_str())
                } else {
                    None
                };
                let tasks = block_on_result(ctx.store.inner().list_tasks_for_context(
                    ctx.conversation_id.as_str(),
                    turn_scope,
                    include_done,
                    limit,
                ))?;
                Ok(text_result(json!({
                    "conversation_id": ctx.conversation_id,
                    "conversation_turn_id": if current_turn_only {
                        Value::String(ctx.conversation_turn_id.clone())
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
                let task = block_on_result(ctx.store.inner().update_task_by_id(
                    ctx.conversation_id.as_str(),
                    task_id.as_str(),
                    patch,
                ))?;
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
                        "items": outcome_item_schema()
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
                let task = block_on_result(ctx.store.inner().complete_task_by_id(
                    ctx.conversation_id.as_str(),
                    task_id.as_str(),
                    patch,
                ))?;
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
                let deleted = block_on_result(
                    ctx.store
                        .inner()
                        .delete_task_by_id(ctx.conversation_id.as_str(), task_id.as_str()),
                )?;
                Ok(text_result(json!({
                    "deleted": deleted,
                    "task_id": task_id,
                    "reason": if deleted {
                        Value::Null
                    } else {
                        Value::String(TASK_NOT_FOUND_ERR.to_string())
                    },
                    "conversation_id": ctx.conversation_id,
                })))
            }),
        );
    }
}

fn handle_add_task(
    args: Value,
    ctx: &ToolContext,
    default_timeout_ms: u64,
) -> Result<Value, String> {
    let draft_tasks = parse_task_drafts(&args)?;
    if draft_tasks.is_empty() {
        return Err("tasks is required".to_string());
    }
    if ctx.auto_create_task {
        let tasks = block_on_result(ctx.store.inner().create_tasks_for_turn(
            ctx.conversation_id.as_str(),
            ctx.conversation_turn_id.as_str(),
            draft_tasks,
        ))?;
        emit_task_board_refresh(ctx);
        return Ok(text_result(json!({
            "confirmed": true,
            "cancelled": false,
            "auto_created": true,
            "created_count": tasks.len(),
            "tasks": tasks,
            "conversation_id": ctx.conversation_id,
            "conversation_turn_id": ctx.conversation_turn_id,
        })));
    }
    let result = block_on_result(ctx.store.inner().review_and_create_tasks(
        ctx.conversation_id.as_str(),
        ctx.conversation_turn_id.as_str(),
        draft_tasks,
        default_timeout_ms,
        ctx.on_stream_chunk.clone(),
    ))?;
    Ok(text_result(result))
}

fn emit_task_board_refresh(ctx: &ToolContext) {
    let Some(event) = block_on_option(ctx.store.inner().task_board_updated_event(
        ctx.conversation_id.as_str(),
        ctx.conversation_turn_id.as_str(),
    )) else {
        return;
    };
    let Some(callback) = ctx.on_stream_chunk.as_ref() else {
        return;
    };
    if let Ok(serialized) = serde_json::to_string(&event) {
        callback(serialized);
    }
}

#[cfg(test)]
mod tests;
