use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::core::mcp_tools::ToolStreamChunkCallback;
use crate::services::task_manager::{
    create_task_review, create_tasks_for_turn, list_tasks_for_context, wait_for_task_review_decision,
    TaskCreateReviewPayload, TaskDraft, TaskReviewAction, REVIEW_TIMEOUT_ERR
};
use crate::utils::events::Events;

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

struct ToolContext<'a> {
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

        service.register_tool(
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

        service.register_tool(
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
}

fn handle_add_task(args: Value, ctx: &ToolContext, default_timeout_ms: u64) -> Result<Value, String> {
    let draft_tasks = parse_task_drafts(&args)?;
    if draft_tasks.is_empty() {
        return Err("tasks is required".to_string());
    }

    // Keep review timeout fixed by server policy to avoid per-call drift.
    let timeout_ms = default_timeout_ms;

    let (review_payload, receiver) = block_on_result(create_task_review(
        ctx.session_id,
        ctx.conversation_turn_id,
        draft_tasks,
        timeout_ms,
    ))?;

    emit_review_required_event(ctx.on_stream_chunk.as_ref(), &review_payload);

    let decision = match block_on_result(wait_for_task_review_decision(
        review_payload.review_id.as_str(),
        receiver,
        review_payload.timeout_ms,
    )) {
        Ok(value) => value,
        Err(err) if err == REVIEW_TIMEOUT_ERR => {
            return Ok(cancelled_result("review_timeout"));
        }
        Err(err) => return Err(err),
    };

    match decision.action {
        TaskReviewAction::Confirm => {
            let tasks = block_on_result(create_tasks_for_turn(
                ctx.session_id,
                ctx.conversation_turn_id,
                decision.tasks,
            ))?;
            Ok(text_result(json!({
                "confirmed": true,
                "cancelled": false,
                "created_count": tasks.len(),
                "tasks": tasks,
                "session_id": ctx.session_id,
                "conversation_turn_id": ctx.conversation_turn_id,
            })))
        }
        TaskReviewAction::Cancel => {
            let reason = decision
                .reason
                .unwrap_or_else(|| "user_cancelled".to_string());
            Ok(cancelled_result(reason.as_str()))
        }
    }
}

fn parse_task_drafts(args: &Value) -> Result<Vec<TaskDraft>, String> {
    if let Some(items) = args.get("tasks").and_then(|value| value.as_array()) {
        let mut out = Vec::new();
        for item in items {
            out.push(task_draft_from_value(item)?);
        }
        return Ok(out);
    }

    if args.get("title").and_then(|value| value.as_str()).is_some() {
        return Ok(vec![task_draft_from_map(
            args.as_object()
                .ok_or_else(|| "task payload must be an object".to_string())?,
        )?]);
    }

    Err("tasks or title is required".to_string())
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
        .and_then(|value| value.as_str())
        .ok_or_else(|| "task title is required".to_string())?
        .to_string();

    let details = optional_string(map, "details")
        .or_else(|| optional_string(map, "description"))
        .unwrap_or_default();

    let priority = optional_string(map, "priority").unwrap_or_else(|| "medium".to_string());
    let status = optional_string(map, "status").unwrap_or_else(|| "todo".to_string());
    let due_at = optional_string(map, "due_at").or_else(|| optional_string(map, "dueAt"));

    let tags = match map.get("tags") {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(|item| item.to_string()))
            .collect(),
        Some(Value::String(raw)) => raw
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        _ => Vec::new(),
    };

    Ok(TaskDraft {
        title,
        details,
        priority,
        status,
        tags,
        due_at,
    })
}

fn optional_string(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(|value| value.as_str())
        .and_then(trimmed_non_empty)
        .map(|value| value.to_string())
}

fn emit_review_required_event(
    on_stream_chunk: Option<&ToolStreamChunkCallback>,
    payload: &TaskCreateReviewPayload,
) {
    let Some(callback) = on_stream_chunk else {
        return;
    };

    let event_payload = json!({
        "event": Events::TASK_CREATE_REVIEW_REQUIRED,
        "data": payload,
    });

    if let Ok(serialized) = serde_json::to_string(&event_payload) {
        callback(serialized);
    }
}

fn cancelled_result(reason: &str) -> Value {
    text_result(json!({
        "confirmed": false,
        "cancelled": true,
        "reason": reason,
    }))
}

fn text_result(data: Value) -> Value {
    let text = if data.is_string() {
        data.as_str().unwrap_or("").to_string()
    } else {
        serde_json::to_string_pretty(&data).unwrap_or_else(|_| "{}".to_string())
    };

    json!({
        "content": [
            { "type": "text", "text": text }
        ]
    })
}

fn block_on_result<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        let runtime = tokio::runtime::Runtime::new().map_err(|err| err.to_string())?;
        runtime.block_on(future)
    }
}

fn trimmed_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_task_drafts, TaskManagerOptions, TaskManagerService};
    use serde_json::{json, Value};

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
        let service = TaskManagerService::new(TaskManagerOptions {
            server_name: "task_manager".to_string(),
            review_timeout_ms: 120_000,
        })
        .expect("task manager service should initialize");

        let add_task_tool = service
            .list_tools()
            .into_iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("add_task"))
            .expect("add_task tool should exist");

        let schema = add_task_tool
            .get("inputSchema")
            .expect("add_task should expose inputSchema");

        assert_eq!(schema.get("additionalProperties"), Some(&Value::Bool(false)));

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
}
