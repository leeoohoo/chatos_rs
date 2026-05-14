use std::future::Future;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::core::async_bridge::block_on_result;
use crate::core::tool_io::text_result;

use super::actions::actions_process::{
    kill_process_with_context, poll_process_with_context, read_process_log_with_context,
    wait_process_with_context, write_process_with_context,
};
use super::actions::actions_query::list_processes_with_context;
use super::context::required_trimmed_string;
use super::registration_process::resolve_wait_timeout_ms;
use super::{
    BoundContext, TerminalControllerService, PROCESS_LIST_MAX_LIMIT, PROCESS_POLL_MAX_LIMIT,
    PROCESS_WAIT_MAX_TIMEOUT_MS,
};

impl TerminalControllerService {
    pub(super) fn register_process_compat(&mut self, bound: BoundContext) {
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

                let make_missing_err = |action_name: &str| {
                    format!("terminal_id is required for {}", action_name)
                };
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
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false);
                        let limit = args
                            .get("limit")
                            .and_then(|value| value.as_u64())
                            .unwrap_or(20)
                            .clamp(1, PROCESS_LIST_MAX_LIMIT) as usize;
                        let ctx = bound.clone();
                        run_process_action("list", attach_action, async move {
                            list_processes_with_context(ctx, include_exited, limit).await
                        })
                    }
                    "poll" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("poll"))?;
                        let offset = args
                            .get("offset")
                            .and_then(|value| value.as_i64())
                            .map(|value| value.max(0));
                        let limit = args
                            .get("limit")
                            .and_then(|value| value.as_i64())
                            .unwrap_or(80)
                            .clamp(1, PROCESS_POLL_MAX_LIMIT);
                        let ctx = bound.clone();
                        run_process_action("poll", attach_action, async move {
                            poll_process_with_context(ctx, id.as_str(), offset, limit).await
                        })
                    }
                    "log" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("log"))?;
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
                        run_process_action("log", attach_action, async move {
                            read_process_log_with_context(ctx, id.as_str(), offset, limit).await
                        })
                    }
                    "wait" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("wait"))?;
                        let timeout_ms = resolve_wait_timeout_ms(&args);
                        let ctx = bound.clone();
                        run_process_action("wait", attach_action, async move {
                            wait_process_with_context(ctx, id.as_str(), timeout_ms).await
                        })
                    }
                    "kill" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("kill"))?;
                        let ctx = bound.clone();
                        run_process_action("kill", attach_action, async move {
                            kill_process_with_context(ctx, id.as_str()).await
                        })
                    }
                    "write" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("write"))?;
                        let data = coerce_process_data(args.get("data"))
                            .ok_or_else(|| "data is required for write".to_string())?
                            .to_string();
                        let ctx = bound.clone();
                        run_process_action("write", attach_action, async move {
                            write_process_with_context(ctx, id.as_str(), data.as_str(), false).await
                        })
                    }
                    "submit" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("submit"))?;
                        let data = coerce_process_data(args.get("data"))
                            .unwrap_or_default()
                            .to_string();
                        let ctx = bound.clone();
                        run_process_action("submit", attach_action, async move {
                            write_process_with_context(ctx, id.as_str(), data.as_str(), true).await
                        })
                    }
                    "close" => {
                        let id = terminal_id
                            .clone()
                            .ok_or_else(|| make_missing_err("close"))?;
                        let ctx = bound.clone();
                        run_process_action("close", attach_action, async move {
                            write_process_with_context(ctx, id.as_str(), "\u{4}", false).await
                        })
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

fn run_process_action<Fut>(
    action_name: &str,
    attach_action: impl FnOnce(Value, &str) -> Value,
    future: Fut,
) -> Result<Value, String>
where
    Fut: Future<Output = Result<Value, String>>,
{
    let result = block_on_result(future)?;
    Ok(text_result(attach_action(result, action_name)))
}

pub(super) fn coerce_process_identifier(value: Option<&Value>) -> Option<String> {
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
