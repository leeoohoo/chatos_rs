// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

pub const LIST_TASKS_TOOL: &str = "list_tasks";
pub const GET_TASK_TOOL: &str = "get_task";
pub const CREATE_TASK_TOOL: &str = "create_task";
pub const CREATE_TASKS_TOOL: &str = "create_tasks_with_prerequisites";
pub const CANCEL_TASK_TOOL: &str = "cancel_task";
pub const WAIT_FOR_TASK_COMPLETION_TOOL: &str = "wait_for_task_completion";
pub const GET_TASK_DEPENDENCY_GRAPH_TOOL: &str = "get_task_dependency_graph";
pub const TASK_BUILTIN_KIND_VALUES: [&str; 8] = [
    "AskUser",
    "CodeMaintainerRead",
    "CodeMaintainerWrite",
    "TerminalController",
    "RemoteConnectionController",
    "RequirementSurveyRead",
    "RequirementSurveyWrite",
    "Notepad",
];
const TASK_SELECTABLE_BUILTIN_KIND_VALUES: [&str; 7] = [
    "CodeMaintainerRead",
    "CodeMaintainerWrite",
    "TerminalController",
    "RemoteConnectionController",
    "RequirementSurveyRead",
    "RequirementSurveyWrite",
    "Notepad",
];
pub const TASK_TOOL_NAMES: [&str; 7] = [
    LIST_TASKS_TOOL,
    GET_TASK_TOOL,
    CREATE_TASK_TOOL,
    CREATE_TASKS_TOOL,
    CANCEL_TASK_TOOL,
    WAIT_FOR_TASK_COMPLETION_TOOL,
    GET_TASK_DEPENDENCY_GRAPH_TOOL,
];
pub const TASK_READ_ONLY_TOOLS: [&str; 4] = [
    LIST_TASKS_TOOL,
    GET_TASK_TOOL,
    WAIT_FOR_TASK_COMPLETION_TOOL,
    GET_TASK_DEPENDENCY_GRAPH_TOOL,
];
pub const TASK_APPROVAL_EXEMPT_TOOLS: [&str; 3] =
    [CREATE_TASK_TOOL, CREATE_TASKS_TOOL, CANCEL_TASK_TOOL];

pub fn task_model_tools() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": LIST_TASKS_TOOL,
            "description": "List durable local tasks created from the current conversation/project. Use keyword when the user refers to earlier work. Results are newest-first and never include another conversation or account.",
            "parameters": {
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["draft", "ready", "queued", "running", "succeeded", "failed", "blocked", "cancelled", "archived"]
                    },
                    "keyword": {"type": "string", "maxLength": 500},
                    "tag": {"type": "string", "maxLength": 256},
                    "scheduled_only": {"type": "boolean"},
                    "parent_task_id": {"type": "string", "maxLength": 256},
                    "source_run_id": {"type": "string", "maxLength": 256},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 50},
                    "offset": {"type": "integer", "minimum": 0, "maximum": 100000, "default": 0}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": GET_TASK_TOOL,
            "description": "Get one durable local task created from the current conversation/project by task_id.",
            "parameters": {
                "type": "object",
                "properties": {"task_id": {"type": "string", "minLength": 1}},
                "required": ["task_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": CREATE_TASK_TOOL,
            "description": "Create one durable local task for the current conversation/project. Use this whenever answering requires inspecting project files, using execution tools, or doing tracked work; never ask the user to re-upload an already bound project. The Rust Local Agent Host binds, persists, schedules, and writes the result back locally.",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "minLength": 1},
                    "objective": {"type": "string", "minLength": 1},
                    "description": {"type": "string"},
                    "input_payload": {},
                    "priority": {"type": "integer"},
                    "tags": {
                        "type": "array",
                        "items": {"type": "string"},
                        "uniqueItems": true
                    },
                    "default_model_config_id": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Optional explicit local model configuration id for this Task. When omitted, inherit the model selected for the current Main Chat."
                    },
                    "thinking_level": thinking_level_override_schema(),
                    "requires_execution": {
                        "type": "boolean",
                        "description": "Whether this Task needs command execution, tests, builds, Git operations, or file mutation. Project reads remain available without an execution workspace."
                    },
                    "enabled_builtin_kinds": builtin_kind_selection_schema(),
                    "external_mcp_config_ids": {
                        "type": "array",
                        "maxItems": 0,
                        "items": {"type": "string"},
                        "description": "External MCP configurations are not locally selectable yet; send an empty array."
                    },
                    "plugin_hints": plugin_hints_schema(),
                    "prerequisite_task_ids": prerequisite_task_ids_schema(),
                    "schedule": task_schedule_schema()
                },
                "required": ["title", "objective", "requires_execution", "enabled_builtin_kinds"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": CREATE_TASKS_TOOL,
            "description": "Create a durable local task graph for the current conversation/project. Use investigation, implementation and review stages when prerequisites are needed instead of asking the user to provide the bound project again. Each task uses a unique client_ref and prerequisite_refs may only reference tasks in this call.",
            "parameters": {
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 50,
                        "items": {
                            "type": "object",
                            "properties": {
                                "client_ref": {"type": "string", "minLength": 1},
                                "title": {"type": "string", "minLength": 1},
                                "objective": {"type": "string", "minLength": 1},
                                "description": {"type": "string"},
                                "input_payload": {},
                                "priority": {"type": "integer"},
                                "tags": {
                                    "type": "array",
                                    "items": {"type": "string"},
                                    "uniqueItems": true
                                },
                                "default_model_config_id": {
                                    "type": "string",
                                    "minLength": 1,
                                    "description": "Optional explicit local model configuration id for this Task. When omitted, inherit the model selected for the current Main Chat."
                                },
                                "thinking_level": thinking_level_override_schema(),
                                "requires_execution": {"type": "boolean"},
                                "enabled_builtin_kinds": builtin_kind_selection_schema(),
                                "external_mcp_config_ids": {
                                    "type": "array",
                                    "maxItems": 0,
                                    "items": {"type": "string"}
                                },
                                "plugin_hints": plugin_hints_schema(),
                                "owned_paths": {
                                    "type": "array",
                                    "maxItems": 200,
                                    "items": {"type": "string", "minLength": 1},
                                    "uniqueItems": true
                                },
                                "prerequisite_refs": {
                                    "type": "array",
                                    "items": {"type": "string", "minLength": 1},
                                    "uniqueItems": true
                                },
                                "context_refs": {
                                    "type": "array",
                                    "items": {"type": "string", "minLength": 1},
                                    "uniqueItems": true,
                                    "description": "Non-blocking context relationships to other client_ref values. They are preserved for explanation and graph display but never delay scheduling."
                                },
                                "prerequisite_task_ids": prerequisite_task_ids_schema(),
                                "schedule": task_schedule_schema()
                            },
                            "required": ["client_ref", "title", "objective", "requires_execution", "enabled_builtin_kinds"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["tasks"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": CANCEL_TASK_TOOL,
            "description": "Cancel a pending or running local task from the current conversation/project because it conflicts with the user's latest intent.",
            "parameters": {
                "type": "object",
                "properties": {
                    "task_id": {"type": "string", "minLength": 1},
                    "reason": {"type": "string", "minLength": 1, "maxLength": 1000},
                    "replacement_task_ids": {
                        "type": "array",
                        "items": {"type": "string", "minLength": 1},
                        "uniqueItems": true,
                        "description": "New Task ids that supersede this Task. Replacement cancellations are internal plan maintenance and do not publish a user-facing cancellation callback."
                    }
                },
                "required": ["task_id", "reason"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": WAIT_FOR_TASK_COMPLETION_TOOL,
            "description": "Use exactly once after tasks have been created or adjusted. This is a background handoff signal, not polling; immediately return a concise handoff summary because the final result is written back to this conversation.",
            "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
        }),
        json!({
            "type": "function",
            "name": GET_TASK_DEPENDENCY_GRAPH_TOOL,
            "description": "Get the complete local dependency graph containing one task from the current conversation/project.",
            "parameters": {
                "type": "object",
                "properties": {"task_id": {"type": "string", "minLength": 1}},
                "required": ["task_id"],
                "additionalProperties": false
            }
        }),
    ]
}

fn thinking_level_override_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["none", "auto", "minimal", "low", "medium", "high", "xhigh", "max"],
        "description": "Optional reasoning level override for this Task. Omit it to use the selected model configuration's default Thinking level."
    })
}

fn builtin_kind_selection_schema() -> Value {
    json!({
        "type": "array",
        "items": {"type": "string", "enum": TASK_SELECTABLE_BUILTIN_KIND_VALUES},
        "uniqueItems": true,
        "description": "Select only the minimum local capabilities required by this Task. AskUser is added automatically when required by policy; CodeMaintainerWrite implies CodeMaintainerRead; RequirementSurveyWrite implies RequirementSurveyRead."
    })
}

fn plugin_hints_schema() -> Value {
    json!({
        "type": "array",
        "maxItems": 16,
        "items": {
            "type": "object",
            "properties": {
                "plugin_key": {"type": "string", "minLength": 1},
                "reason": {"type": "string", "maxLength": 1000}
            },
            "required": ["plugin_key"],
            "additionalProperties": false
        }
    })
}

fn prerequisite_task_ids_schema() -> Value {
    json!({
        "type": "array",
        "items": {"type": "string", "minLength": 1},
        "uniqueItems": true,
        "description": "Existing local Task ids that must complete successfully before this Task runs. Use only ids returned by Task tools for the current user and project."
    })
}

fn task_schedule_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "mode": {"type": "string", "enum": ["manual", "once", "interval", "contact_async"]},
            "run_at": {"type": "string", "description": "Optional RFC 3339 time at which this Task may start."},
            "interval_seconds": {"type": "integer", "minimum": 1}
        },
        "additionalProperties": false,
        "description": "Optional scheduling request. Main Chat Tasks retain contact_async behavior; a supplied run_at delays local execution."
    })
}
