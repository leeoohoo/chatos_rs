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
                        "enum": ["pending", "ready", "running", "succeeded", "failed", "cancelled", "blocked"]
                    },
                    "keyword": {"type": "string", "maxLength": 500},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 50},
                    "offset": {"type": "integer", "minimum": 0, "maximum": 10000, "default": 0}
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
                    "input_payload": {"type": "object"}
                },
                "required": ["title", "objective"],
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
                                "input_payload": {"type": "object"},
                                "prerequisite_refs": {
                                    "type": "array",
                                    "items": {"type": "string", "minLength": 1},
                                    "uniqueItems": true
                                }
                            },
                            "required": ["client_ref", "title", "objective"],
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
                    "reason": {"type": "string", "minLength": 1, "maxLength": 4000},
                    "expected_version": {"type": "integer", "minimum": 1}
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
