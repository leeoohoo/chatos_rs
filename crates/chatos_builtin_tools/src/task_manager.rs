use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::tool_registry::{block_on_option, block_on_result, text_result, ToolRegistry};

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

fn task_payload_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tasks": {
                "type": "array",
                "items": task_item_schema()
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
                "items": outcome_item_schema()
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
    })
}

fn task_item_schema() -> Value {
    json!({
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
                "items": outcome_item_schema()
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
    })
}

fn outcome_item_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "kind": { "type": "string" },
            "text": { "type": "string" },
            "importance": { "type": "string", "enum": ["high", "medium", "low"] },
            "refs": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["text"],
        "additionalProperties": false
    })
}

pub fn parse_task_drafts(args: &Value) -> Result<Vec<TaskDraft>, String> {
    if let Some(items) = args.get("tasks").and_then(Value::as_array) {
        let mut out = Vec::new();
        for item in items {
            out.push(task_draft_from_value(item)?);
        }
        return Ok(out);
    }
    if args.get("title").and_then(Value::as_str).is_some() {
        return Ok(vec![task_draft_from_map(
            args.as_object()
                .ok_or_else(|| "task payload must be an object".to_string())?,
        )?]);
    }
    Err("tasks or title is required".to_string())
}

pub fn parse_update_patch(value: &Value) -> Result<TaskUpdatePatch, String> {
    let map = match value {
        Value::Object(map) => map.clone(),
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err("changes cannot be empty".to_string());
            }
            let parsed: Value = serde_json::from_str(trimmed)
                .map_err(|_| "changes must be valid JSON".to_string())?;
            parsed
                .as_object()
                .cloned()
                .ok_or_else(|| "changes must be a JSON object".to_string())?
        }
        _ => return Err("changes must be a JSON object string".to_string()),
    };
    if map.is_empty() {
        return Err("changes cannot be empty".to_string());
    }

    let mut patch = TaskUpdatePatch::default();
    for (key, value) in &map {
        match key.as_str() {
            "title" => patch.title = Some(expect_string(value, "changes.title")?),
            "details" | "description" => {
                patch.details = Some(expect_string(value, "changes.details")?)
            }
            "priority" => patch.priority = Some(expect_string(value, "changes.priority")?),
            "status" => patch.status = Some(expect_string(value, "changes.status")?),
            "tags" => patch.tags = Some(parse_tags(value, "changes.tags")?),
            "due_at" | "dueAt" => patch.due_at = Some(parse_due_at(value, "changes.due_at")?),
            "outcome_summary" | "outcomeSummary" => {
                patch.outcome_summary = Some(expect_string(value, "changes.outcome_summary")?)
            }
            "outcome_items" | "outcomeItems" => {
                patch.outcome_items = Some(parse_outcome_items(value, "changes.outcome_items")?);
            }
            "resume_hint" | "resumeHint" => {
                patch.resume_hint = Some(expect_string(value, "changes.resume_hint")?)
            }
            "blocker_reason" | "blockerReason" => {
                patch.blocker_reason = Some(expect_string(value, "changes.blocker_reason")?)
            }
            "blocker_needs" | "blockerNeeds" => {
                patch.blocker_needs = Some(parse_tags(value, "changes.blocker_needs")?);
            }
            "blocker_kind" | "blockerKind" => {
                patch.blocker_kind = Some(expect_string(value, "changes.blocker_kind")?)
            }
            "completed_at" | "completedAt" => {
                patch.completed_at = Some(parse_due_at(value, "changes.completed_at")?);
            }
            "last_outcome_at" | "lastOutcomeAt" => {
                patch.last_outcome_at = Some(parse_due_at(value, "changes.last_outcome_at")?);
            }
            other => return Err(format!("unsupported changes field: {other}")),
        }
    }
    if patch.title.is_none()
        && patch.details.is_none()
        && patch.priority.is_none()
        && patch.status.is_none()
        && patch.tags.is_none()
        && patch.due_at.is_none()
        && patch.outcome_summary.is_none()
        && patch.outcome_items.is_none()
        && patch.resume_hint.is_none()
        && patch.blocker_reason.is_none()
        && patch.blocker_needs.is_none()
        && patch.blocker_kind.is_none()
        && patch.completed_at.is_none()
        && patch.last_outcome_at.is_none()
    {
        return Err("changes cannot be empty".to_string());
    }
    Ok(patch)
}

fn task_draft_from_value(value: &Value) -> Result<TaskDraft, String> {
    let map = value
        .as_object()
        .ok_or_else(|| "each task must be an object".to_string())?;
    task_draft_from_map(map)
}

fn task_draft_from_map(map: &Map<String, Value>) -> Result<TaskDraft, String> {
    let title = map
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| "task title is required".to_string())?
        .to_string();
    Ok(TaskDraft {
        title,
        details: optional_string(map, "details")
            .or_else(|| optional_string(map, "description"))
            .unwrap_or_default(),
        priority: optional_string(map, "priority").unwrap_or_else(default_priority),
        status: optional_string(map, "status").unwrap_or_else(default_status),
        tags: map
            .get("tags")
            .map(|value| parse_tags(value, "task.tags"))
            .transpose()?
            .unwrap_or_default(),
        due_at: optional_string(map, "due_at").or_else(|| optional_string(map, "dueAt")),
        outcome_summary: optional_string(map, "outcome_summary")
            .or_else(|| optional_string(map, "outcomeSummary"))
            .unwrap_or_default(),
        outcome_items: match map.get("outcome_items").or_else(|| map.get("outcomeItems")) {
            Some(value) => parse_outcome_items(value, "task.outcome_items")?,
            None => Vec::new(),
        },
        resume_hint: optional_string(map, "resume_hint")
            .or_else(|| optional_string(map, "resumeHint"))
            .unwrap_or_default(),
        blocker_reason: optional_string(map, "blocker_reason")
            .or_else(|| optional_string(map, "blockerReason"))
            .unwrap_or_default(),
        blocker_needs: match map.get("blocker_needs").or_else(|| map.get("blockerNeeds")) {
            Some(value) => parse_tags(value, "task.blocker_needs")?,
            None => Vec::new(),
        },
        blocker_kind: optional_string(map, "blocker_kind")
            .or_else(|| optional_string(map, "blockerKind"))
            .unwrap_or_default(),
    })
}

fn parse_tags(value: &Value, field: &str) -> Result<Vec<String>, String> {
    match value {
        Value::Array(values) => Ok(values
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect()),
        Value::String(raw) => Ok(raw
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()),
        _ => Err(format!(
            "{field} must be an array or comma-separated string"
        )),
    }
}

fn parse_due_at(value: &Value, field: &str) -> Result<Option<String>, String> {
    match value {
        Value::Null => Ok(None),
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        _ => Err(format!("{field} must be a string or null")),
    }
}

fn parse_outcome_items(value: &Value, field: &str) -> Result<Vec<TaskOutcomeItem>, String> {
    match value {
        Value::Array(values) => values
            .iter()
            .map(|item| {
                let map = item
                    .as_object()
                    .ok_or_else(|| format!("{field} items must be objects"))?;
                let text = map
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("{field} item.text is required"))?
                    .to_string();
                let refs = map
                    .get("refs")
                    .map(|refs| parse_tags(refs, "changes.outcome_items.refs"))
                    .transpose()?
                    .unwrap_or_default();
                Ok(TaskOutcomeItem {
                    kind: map
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("finding")
                        .to_string(),
                    text,
                    importance: map
                        .get("importance")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    refs,
                })
            })
            .collect(),
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(Vec::new());
            }
            let parsed: Value = serde_json::from_str(trimmed)
                .map_err(|_| format!("{field} must be valid JSON when provided as string"))?;
            parse_outcome_items(&parsed, field)
        }
        _ => Err(format!("{field} must be an array or JSON string")),
    }
}

fn expect_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("{field} must be a string"))
}

fn optional_string(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .and_then(trimmed_non_empty)
        .map(ToOwned::to_owned)
}

fn required_string_arg(args: &Value, field: &str) -> Result<String, String> {
    let value = args
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{field} is required"))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(trimmed.to_string())
    }
}

pub fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::{json, Value};

    use super::*;

    #[derive(Debug, Clone)]
    struct NoopTaskStore;

    #[async_trait]
    impl TaskManagerStore for NoopTaskStore {
        async fn create_tasks_for_turn(
            &self,
            _conversation_id: &str,
            _conversation_turn_id: &str,
            _draft_tasks: Vec<TaskDraft>,
        ) -> Result<Vec<Value>, String> {
            Ok(Vec::new())
        }

        async fn review_and_create_tasks(
            &self,
            _conversation_id: &str,
            _conversation_turn_id: &str,
            _draft_tasks: Vec<TaskDraft>,
            _timeout_ms: u64,
            _on_stream_chunk: Option<TaskStreamChunkCallback>,
        ) -> Result<Value, String> {
            Ok(json!({
                "confirmed": false,
                "cancelled": true,
                "reason": "noop",
            }))
        }

        async fn list_tasks_for_context(
            &self,
            _conversation_id: &str,
            _conversation_turn_id: Option<&str>,
            _include_done: bool,
            _limit: usize,
        ) -> Result<Vec<Value>, String> {
            Ok(Vec::new())
        }

        async fn update_task_by_id(
            &self,
            _conversation_id: &str,
            _task_id: &str,
            _patch: TaskUpdatePatch,
        ) -> Result<Value, String> {
            Ok(json!({ "id": "task_1" }))
        }

        async fn complete_task_by_id(
            &self,
            _conversation_id: &str,
            _task_id: &str,
            _patch: Option<TaskUpdatePatch>,
        ) -> Result<Value, String> {
            Ok(json!({ "id": "task_1", "status": "done" }))
        }

        async fn delete_task_by_id(
            &self,
            _conversation_id: &str,
            _task_id: &str,
        ) -> Result<bool, String> {
            Ok(true)
        }

        async fn task_board_updated_event(
            &self,
            _conversation_id: &str,
            _conversation_turn_id: &str,
        ) -> Option<Value> {
            None
        }
    }

    fn test_service(auto_create_task: bool) -> TaskManagerService {
        TaskManagerService::new(TaskManagerOptions {
            server_name: "task_manager".to_string(),
            review_timeout_ms: 120_000,
            auto_create_task,
            store: TaskManagerStoreRef::new(Arc::new(NoopTaskStore)),
        })
        .expect("task manager service should initialize")
    }

    fn contains_schema_key(node: &Value, key: &str) -> bool {
        match node {
            Value::Object(map) => map
                .iter()
                .any(|(name, value)| name == key || contains_schema_key(value, key)),
            Value::Array(items) => items.iter().any(|item| contains_schema_key(item, key)),
            _ => false,
        }
    }

    #[test]
    fn parse_task_drafts_supports_single_task_shape() {
        let args = json!({ "title": "Ship task manager", "priority": "high" });
        let drafts = parse_task_drafts(&args).expect("single task payload should parse");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].title, "Ship task manager");
        assert_eq!(drafts[0].priority, "high");
    }

    #[test]
    fn add_task_schema_is_strict_and_compatible() {
        let add_task_tool = test_service(false)
            .list_tools()
            .into_iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("add_task"))
            .expect("add_task tool should exist");

        let schema = add_task_tool
            .get("inputSchema")
            .expect("add_task should expose inputSchema");

        assert_eq!(
            schema.get("additionalProperties"),
            Some(&Value::Bool(false))
        );

        let root_properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .expect("add_task schema should expose root properties");
        assert!(
            !root_properties.contains_key("timeout_ms"),
            "add_task schema should not allow timeout override"
        );

        let task_item_schema = schema
            .get("properties")
            .and_then(Value::as_object)
            .and_then(|props| props.get("tasks"))
            .and_then(|tasks| tasks.get("items"))
            .expect("tasks.items schema should exist");

        assert_eq!(
            task_item_schema.get("additionalProperties"),
            Some(&Value::Bool(false))
        );

        assert!(
            !contains_schema_key(schema, "oneOf"),
            "add_task schema should not contain oneOf"
        );
    }

    #[test]
    fn update_patch_supports_null_due_at_and_tags_string() {
        let patch = parse_update_patch(&json!({
            "details": "refresh docs",
            "tags": "backend, task",
            "due_at": null
        }))
        .expect("update patch should parse");

        assert_eq!(patch.details.as_deref(), Some("refresh docs"));
        assert_eq!(
            patch.tags,
            Some(vec!["backend".to_string(), "task".to_string()])
        );
        assert_eq!(patch.due_at, Some(None));
    }

    #[test]
    fn task_manager_tools_include_mutations() {
        let tools = test_service(false).list_tools();
        let tool_names: Vec<&str> = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect();

        assert!(tool_names.contains(&"update_task"));
        assert!(tool_names.contains(&"complete_task"));
        assert!(tool_names.contains(&"delete_task"));
    }

    #[test]
    fn task_manager_requires_explicit_conversation_context() {
        let service = test_service(false);

        let missing_conversation = service
            .call_tool("list_tasks", json!({}), None, Some("turn_1"), None)
            .expect_err("task manager must not fall back to a shared conversation");
        assert!(missing_conversation.contains("conversation_id"));

        let missing_turn = service
            .call_tool("list_tasks", json!({}), Some("session_1"), None, None)
            .expect_err("task manager must not fall back to a shared turn");
        assert!(missing_turn.contains("conversation_turn_id"));
    }

    #[test]
    fn update_task_schema_changes_is_string() {
        let update_task_tool = test_service(false)
            .list_tools()
            .into_iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("update_task"))
            .expect("update_task tool should exist");

        let schema = update_task_tool
            .get("inputSchema")
            .expect("update_task should expose inputSchema");

        let changes_type = schema
            .get("properties")
            .and_then(Value::as_object)
            .and_then(|props| props.get("changes"))
            .and_then(|changes| changes.get("type"))
            .and_then(Value::as_str);
        assert_eq!(changes_type, Some("string"));
    }

    #[test]
    fn add_task_description_mentions_confirmation_behavior() {
        let manual_description = test_service(false)
            .list_tools()
            .into_iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("add_task"))
            .and_then(|tool| {
                tool.get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .expect("manual add_task description");
        let auto_description = test_service(true)
            .list_tools()
            .into_iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("add_task"))
            .and_then(|tool| {
                tool.get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .expect("auto add_task description");

        assert!(manual_description.contains("confirmed by the user"));
        assert!(auto_description.contains("persisted automatically"));
    }
}
