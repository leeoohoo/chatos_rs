use serde_json::{json, Value};

use super::{PROCESS_LIST_MAX_LIMIT, PROCESS_POLL_MAX_LIMIT, PROCESS_WAIT_MAX_TIMEOUT_MS};

pub(super) fn execute_command_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "Local directory path under project root. Defaults to project root when omitted."
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
        "additionalProperties": false
    })
}

pub(super) fn recent_logs_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "per_terminal_limit": { "type": "integer", "minimum": 1, "maximum": 50 },
            "terminal_limit": { "type": "integer", "minimum": 1, "maximum": 20 }
        },
        "additionalProperties": false
    })
}

pub(super) fn process_list_schema() -> Value {
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
    })
}

pub(super) fn process_poll_schema() -> Value {
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
    })
}

pub(super) fn process_log_schema() -> Value {
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
    })
}

pub(super) fn process_wait_schema() -> Value {
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
    })
}

pub(super) fn process_write_schema() -> Value {
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
    })
}

pub(super) fn process_kill_schema() -> Value {
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
    })
}

pub(super) fn process_compat_schema() -> Value {
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
    })
}
