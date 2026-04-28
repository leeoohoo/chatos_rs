use serde_json::{json, Value};

use crate::core::tool_registry::async_text_tool_handler_with_optional_string;

use super::actions::actions_process::{
    kill_process_with_context, poll_process_with_context, read_process_log_with_context,
    wait_process_with_context, write_process_with_context,
};
use super::actions::actions_query::list_processes_with_context;
use super::context::required_trimmed_string;
use super::{
    BoundContext, PROCESS_LIST_MAX_LIMIT, PROCESS_POLL_MAX_LIMIT, PROCESS_WAIT_MAX_TIMEOUT_MS,
    TerminalControllerService,
};

impl TerminalControllerService {
    pub(super) fn register_process_list(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, _conversation_id| {
                let include_exited = args
                    .get("include_exited")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                let limit = args
                    .get("limit")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(20)
                    .clamp(1, PROCESS_LIST_MAX_LIMIT) as usize;
                let ctx = bound.clone();
                Ok(async move { list_processes_with_context(ctx, include_exited, limit).await })
            }),
        );
    }

    pub(super) fn register_process_poll(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, _conversation_id| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let offset = args.get("offset").and_then(|value| value.as_i64()).map(|value| {
                    value.max(0)
                });
                let limit = args
                    .get("limit")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(80)
                    .clamp(1, PROCESS_POLL_MAX_LIMIT);
                let ctx = bound.clone();
                Ok(async move {
                    poll_process_with_context(ctx, terminal_id.as_str(), offset, limit).await
                })
            }),
        );
    }

    pub(super) fn register_process_log(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, _conversation_id| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let offset = args
                    .get("offset")
                    .and_then(|value| value.as_i64())
                    .map(|value| value.max(0));
                let limit = args
                    .get("limit")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(200)
                    .clamp(1, PROCESS_POLL_MAX_LIMIT);
                let ctx = bound.clone();
                Ok(async move {
                    read_process_log_with_context(ctx, terminal_id.as_str(), offset, limit).await
                })
            }),
        );
    }

    pub(super) fn register_process_wait(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, _conversation_id| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let timeout_ms = resolve_wait_timeout_ms(&args);
                let ctx = bound.clone();
                Ok(async move {
                    wait_process_with_context(ctx, terminal_id.as_str(), timeout_ms).await
                })
            }),
        );
    }

    pub(super) fn register_process_write(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, _conversation_id| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let data = args
                    .get("data")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| "data is required".to_string())?
                    .to_string();
                let submit = args
                    .get("submit")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                let ctx = bound.clone();
                Ok(async move {
                    write_process_with_context(ctx, terminal_id.as_str(), data.as_str(), submit)
                        .await
                })
            }),
        );
    }

    pub(super) fn register_process_kill(&mut self, bound: BoundContext) {
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
            async_text_tool_handler_with_optional_string(move |args, _conversation_id| {
                let terminal_id = required_trimmed_string(&args, "terminal_id")?;
                let ctx = bound.clone();
                Ok(async move {
                    kill_process_with_context(ctx, terminal_id.as_str()).await
                })
            }),
        );
    }
}

pub(super) fn resolve_wait_timeout_ms(args: &Value) -> u64 {
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
