mod actions;
mod capture;
mod context;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;

use self::actions::{
    execute_command_with_context, get_recent_logs_with_context, kill_process_with_context,
    list_processes_with_context, poll_process_with_context, read_process_log_with_context,
    wait_process_with_context, write_process_with_context,
};
use self::context::{canonicalize_path, required_trimmed_string};

pub struct TerminalControllerOptions {
    pub root: PathBuf,
    pub user_id: Option<String>,
    pub project_id: Option<String>,
    pub idle_timeout_ms: u64,
    pub max_wait_ms: u64,
    pub max_output_chars: usize,
}

#[derive(Clone)]
pub struct TerminalControllerService {
    tools: HashMap<String, Tool>,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    input_schema: Value,
    handler: ToolHandler,
}

type ToolHandler = Arc<dyn Fn(Value, Option<&str>) -> Result<Value, String> + Send + Sync>;

pub(super) const RECENT_LOGS_MAX_PER_TERMINAL_LIMIT: i64 = 50;
pub(super) const RECENT_LOGS_MAX_TERMINAL_LIMIT: u64 = 20;
pub(super) const RECENT_LOGS_PER_ENTRY_MAX_CHARS: usize = 1_500;
pub(super) const RECENT_LOGS_TOTAL_MAX_CHARS_PER_TERMINAL: usize = 16_000;
pub(super) const PROCESS_LIST_MAX_LIMIT: u64 = 100;
pub(super) const PROCESS_POLL_MAX_LIMIT: i64 = 200;
pub(super) const PROCESS_WAIT_MAX_TIMEOUT_MS: u64 = 600_000;

#[derive(Clone)]
pub(super) struct BoundContext {
    pub(super) root: PathBuf,
    pub(super) user_id: Option<String>,
    pub(super) project_id: Option<String>,
    pub(super) idle_timeout_ms: u64,
    pub(super) max_wait_ms: u64,
    pub(super) max_output_chars: usize,
}

impl TerminalControllerService {
    pub fn new(opts: TerminalControllerOptions) -> Result<Self, String> {
        std::fs::create_dir_all(&opts.root)
            .map_err(|err| format!("create terminal controller root failed: {}", err))?;
        let root = canonicalize_path(&opts.root)?;

        let mut service = Self {
            tools: HashMap::new(),
        };

        let bound = BoundContext {
            root: root.clone(),
            user_id: opts.user_id.clone(),
            project_id: opts
                .project_id
                .as_deref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            idle_timeout_ms: opts.idle_timeout_ms.max(1_000),
            max_wait_ms: opts.max_wait_ms.max(5_000),
            max_output_chars: opts.max_output_chars.max(1_000),
        };

        let root_for_desc = root.to_string_lossy().to_string();
        service.register_execute_command(bound.clone(), root_for_desc.as_str());
        service.register_get_recent_logs(bound.clone());
        service.register_process_list(bound.clone());
        service.register_process_poll(bound.clone());
        service.register_process_log(bound.clone());
        service.register_process_wait(bound.clone());
        service.register_process_write(bound.clone());
        service.register_process_kill(bound.clone());
        service.register_process_compat(bound);

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
        conversation_id: Option<&str>,
    ) -> Result<Value, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool not found: {name}"))?;
        (tool.handler)(args, conversation_id)
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

    fn register_execute_command(&mut self, bound: BoundContext, root_for_desc: &str) {
        self.register_tool(
            "execute_command",
            &format!(
                "LOCAL ONLY: execute shell command in the local project terminal with path switching. Relative path is resolved from project root ({root_for_desc}). This tool does NOT execute on remote SSH hosts. For remote servers, use builtin_remote_connection_controller.run_command instead."
            ),
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Local directory path under project root."
                    },
                    "common": {
                        "type": "string",
                        "description": "Local shell command to run."
                    },
                    "command": {
                        "type": "string",
                        "description": "Alias of common. Local shell command to run."
                    },
                    "background": {
                        "type": "boolean",
                        "default": false,
                        "description": "When true, return immediately and use process_poll/process_wait to track progress."
                    }
                },
                "additionalProperties": false,
                "required": ["path"]
            }),
            Arc::new(move |args, _conversation_id| {
                let path = required_trimmed_string(&args, "path")?;
                let command = args
                    .get("common")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(|v| v.to_string())
                    .or_else(|| {
                        args.get("command")
                            .and_then(|v| v.as_str())
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(|v| v.to_string())
                    })
                    .ok_or_else(|| "common is required".to_string())?;
                let background = args
                    .get("background")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    execute_command_with_context(ctx, path.as_str(), command.as_str(), background)
                        .await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_get_recent_logs(&mut self, bound: BoundContext) {
        self.register_tool(
            "get_recent_logs",
            "Get recent logs grouped by terminal for current agent project.",
            json!({
                "type": "object",
                "properties": {
                    "per_terminal_limit": { "type": "integer", "minimum": 1, "maximum": 50 },
                    "terminal_limit": { "type": "integer", "minimum": 1, "maximum": 20 }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args, _conversation_id| {
                let per_terminal_limit = args
                    .get("per_terminal_limit")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(10)
                    .clamp(1, RECENT_LOGS_MAX_PER_TERMINAL_LIMIT);
                let terminal_limit =
                    args.get("terminal_limit")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(20)
                        .clamp(1, RECENT_LOGS_MAX_TERMINAL_LIMIT) as usize;
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    get_recent_logs_with_context(ctx, per_terminal_limit, terminal_limit).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_process_list(&mut self, bound: BoundContext) {
        self.register_tool(
            "process_list",
            "List local terminal processes in current agent project context.",
            json!({
                "type": "object",
                "properties": {
                    "include_exited": { "type": "boolean", "default": false },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": PROCESS_LIST_MAX_LIMIT
                    }
                },
                "additionalProperties": false
            }),
            Arc::new(move |args, _conversation_id| {
                let include_exited = args
                    .get("include_exited")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(20)
                    .clamp(1, PROCESS_LIST_MAX_LIMIT) as usize;
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    list_processes_with_context(ctx, include_exited, limit).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_process_poll(&mut self, bound: BoundContext) {
        self.register_tool(
            "process_poll",
            "Poll one local terminal process (status and buffered output logs).",
            json!({
                "type": "object",
                "properties": {
                    "terminal_id": {
                        "type": "string",
                        "description": "Terminal process id from process_list or execute_command result."
                    },
                    "offset": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Optional absolute log offset for incremental polling."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": PROCESS_POLL_MAX_LIMIT,
                        "description": "Max logs to fetch."
                    }
                },
                "required": ["terminal_id"],
                "additionalProperties": false
            }),
            Arc::new(move |args, _conversation_id| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let offset = args.get("offset").and_then(|v| v.as_i64()).map(|v| v.max(0));
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(80)
                    .clamp(1, PROCESS_POLL_MAX_LIMIT);
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    poll_process_with_context(ctx, terminal_id.as_str(), offset, limit).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_process_log(&mut self, bound: BoundContext) {
        self.register_tool(
            "process_log",
            "Read process logs in Hermes-compatible text mode with optional offset pagination.",
            json!({
                "type": "object",
                "properties": {
                    "terminal_id": {
                        "type": "string",
                        "description": "Terminal process id."
                    },
                    "offset": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Optional line offset for pagination."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": PROCESS_POLL_MAX_LIMIT,
                        "description": "Maximum lines to return."
                    }
                },
                "required": ["terminal_id"],
                "additionalProperties": false
            }),
            Arc::new(move |args, _conversation_id| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let offset = args
                    .get("offset")
                    .and_then(|v| v.as_i64())
                    .map(|v| v.max(0));
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(200)
                    .clamp(1, PROCESS_POLL_MAX_LIMIT);
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    read_process_log_with_context(ctx, terminal_id.as_str(), offset, limit).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_process_wait(&mut self, bound: BoundContext) {
        self.register_tool(
            "process_wait",
            "Wait until a local terminal process exits or becomes idle.",
            json!({
                "type": "object",
                "properties": {
                    "terminal_id": {
                        "type": "string",
                        "description": "Terminal process id."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 1_000,
                        "maximum": PROCESS_WAIT_MAX_TIMEOUT_MS,
                        "description": "Maximum wait time in milliseconds."
                    },
                    "timeout": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": PROCESS_WAIT_MAX_TIMEOUT_MS / 1_000,
                        "description": "Alias of timeout_ms in seconds."
                    }
                },
                "required": ["terminal_id"],
                "additionalProperties": false
            }),
            Arc::new(move |args, _conversation_id| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let timeout_ms = resolve_wait_timeout_ms(&args);
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    wait_process_with_context(ctx, terminal_id.as_str(), timeout_ms).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_process_write(&mut self, bound: BoundContext) {
        self.register_tool(
            "process_write",
            "Write stdin content to a local terminal process.",
            json!({
                "type": "object",
                "properties": {
                    "terminal_id": {
                        "type": "string",
                        "description": "Terminal process id."
                    },
                    "data": {
                        "type": "string",
                        "description": "Raw stdin content to send."
                    },
                    "submit": {
                        "type": "boolean",
                        "default": false,
                        "description": "Append one newline (Enter key) after data."
                    }
                },
                "required": ["terminal_id", "data"],
                "additionalProperties": false
            }),
            Arc::new(move |args, _conversation_id| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let data = args
                    .get("data")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "data is required".to_string())?
                    .to_string();
                let submit = args
                    .get("submit")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    write_process_with_context(ctx, terminal_id.as_str(), data.as_str(), submit)
                        .await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_process_kill(&mut self, bound: BoundContext) {
        self.register_tool(
            "process_kill",
            "Terminate a local terminal process session.",
            json!({
                "type": "object",
                "properties": {
                    "terminal_id": {
                        "type": "string",
                        "description": "Terminal process id."
                    }
                },
                "required": ["terminal_id"],
                "additionalProperties": false
            }),
            Arc::new(move |args, _conversation_id| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let ctx = bound.clone();
                let result = block_on_result(async move {
                    kill_process_with_context(ctx, terminal_id.as_str()).await
                })?;
                Ok(text_result(result))
            }),
        );
    }

    fn register_process_compat(&mut self, bound: BoundContext) {
        self.register_tool(
            "process",
            "Hermes-compatible process manager. Actions: list/poll/log/wait/kill/write/submit/close.",
            json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "poll", "log", "wait", "kill", "write", "submit", "close"]
                    },
                    "terminal_id": { "type": "string", "description": "Process id." },
                    "include_exited": { "type": "boolean", "default": false },
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": PROCESS_POLL_MAX_LIMIT
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 1_000,
                        "maximum": PROCESS_WAIT_MAX_TIMEOUT_MS
                    },
                    "timeout": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": PROCESS_WAIT_MAX_TIMEOUT_MS / 1_000,
                        "description": "Alias of timeout_ms in seconds."
                    },
                    "data": { "type": "string" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
            Arc::new(move |args, _conversation_id| {
                let action = required_trimmed_string(&args, "action")?.to_ascii_lowercase();
                let terminal_id = args
                    .get("terminal_id")
                    .and_then(|value| coerce_process_identifier(Some(value)));

                let make_missing_err = |act: &str| format!("terminal_id is required for {}", act);
                let attach_action = |mut value: Value, action_name: &str| {
                    if let Some(map) = value.as_object_mut() {
                        map.insert(
                            "action".to_string(),
                            Value::String(action_name.to_string()),
                        );
                    }
                    value
                };

                match action.as_str() {
                    "list" => {
                        let include_exited = args
                            .get("include_exited")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let limit = args
                            .get("limit")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(20)
                            .clamp(1, PROCESS_LIST_MAX_LIMIT) as usize;
                        let ctx = bound.clone();
                        let result = block_on_result(async move {
                            list_processes_with_context(ctx, include_exited, limit).await
                        })?;
                        Ok(text_result(attach_action(result, "list")))
                    }
                    "poll" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("poll"))?;
                        let offset =
                            args.get("offset").and_then(|v| v.as_i64()).map(|v| v.max(0));
                        let limit = args
                            .get("limit")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(80)
                            .clamp(1, PROCESS_POLL_MAX_LIMIT);
                        let ctx = bound.clone();
                        let result = block_on_result(async move {
                            poll_process_with_context(ctx, id.as_str(), offset, limit).await
                        })?;
                        Ok(text_result(attach_action(result, "poll")))
                    }
                    "log" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("log"))?;
                        let offset =
                            args.get("offset").and_then(|v| v.as_i64()).map(|v| v.max(0));
                        let limit = args
                            .get("limit")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(200)
                            .clamp(1, PROCESS_POLL_MAX_LIMIT);
                        let ctx = bound.clone();
                        let result = block_on_result(async move {
                            read_process_log_with_context(ctx, id.as_str(), offset, limit).await
                        })?;
                        Ok(text_result(attach_action(result, "log")))
                    }
                    "wait" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("wait"))?;
                        let timeout_ms = resolve_wait_timeout_ms(&args);
                        let ctx = bound.clone();
                        let result = block_on_result(async move {
                            wait_process_with_context(ctx, id.as_str(), timeout_ms).await
                        })?;
                        Ok(text_result(attach_action(result, "wait")))
                    }
                    "kill" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("kill"))?;
                        let ctx = bound.clone();
                        let result = block_on_result(async move {
                            kill_process_with_context(ctx, id.as_str()).await
                        })?;
                        Ok(text_result(attach_action(result, "kill")))
                    }
                    "write" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("write"))?;
                        let data = coerce_process_data(args.get("data"))
                            .ok_or_else(|| "data is required for write".to_string())?
                            .to_string();
                        let ctx = bound.clone();
                        let result = block_on_result(async move {
                            write_process_with_context(ctx, id.as_str(), data.as_str(), false).await
                        })?;
                        Ok(text_result(attach_action(result, "write")))
                    }
                    "submit" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("submit"))?;
                        let data = coerce_process_data(args.get("data"))
                            .unwrap_or_default()
                            .to_string();
                        let ctx = bound.clone();
                        let result = block_on_result(async move {
                            write_process_with_context(ctx, id.as_str(), data.as_str(), true).await
                        })?;
                        Ok(text_result(attach_action(result, "submit")))
                    }
                    "close" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("close"))?;
                        let ctx = bound.clone();
                        let result = block_on_result(async move {
                            write_process_with_context(ctx, id.as_str(), "\u{4}", false).await
                        })?;
                        Ok(text_result(attach_action(result, "close")))
                    }
                    _ => Err(
                        "Unknown process action. Use one of: list, poll, log, wait, kill, write, submit, close"
                            .to_string(),
                    ),
                }
            }),
        );
    }
}

fn coerce_process_identifier(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(Value::Number(raw)) => Some(raw.to_string()),
        _ => None,
    }
}

fn resolve_wait_timeout_ms(args: &Value) -> u64 {
    args.get("timeout_ms")
        .and_then(|value| value.as_u64())
        .or_else(|| {
            args.get("timeout")
                .and_then(|value| value.as_u64())
                .map(|seconds| seconds.saturating_mul(1_000))
        })
        .unwrap_or(30_000)
        .clamp(1_000, PROCESS_WAIT_MAX_TIMEOUT_MS)
}

fn coerce_process_data(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(raw)) => Some(raw.to_string()),
        Some(Value::Number(raw)) => Some(raw.to_string()),
        Some(Value::Bool(raw)) => Some(raw.to_string()),
        Some(Value::Null) => Some(String::new()),
        Some(other) => Some(other.to_string()),
        None => None,
    }
}
