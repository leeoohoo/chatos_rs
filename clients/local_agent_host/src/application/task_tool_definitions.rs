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
pub const RESOLVE_TASK_FOLLOWUP_TOOL: &str = "resolve_task_followup";
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
pub const TASK_TOOL_NAMES: [&str; 8] = [
    LIST_TASKS_TOOL,
    GET_TASK_TOOL,
    CREATE_TASK_TOOL,
    CREATE_TASKS_TOOL,
    CANCEL_TASK_TOOL,
    WAIT_FOR_TASK_COMPLETION_TOOL,
    GET_TASK_DEPENDENCY_GRAPH_TOOL,
    RESOLVE_TASK_FOLLOWUP_TOOL,
];
pub const TASK_READ_ONLY_TOOLS: [&str; 4] = [
    LIST_TASKS_TOOL,
    GET_TASK_TOOL,
    WAIT_FOR_TASK_COMPLETION_TOOL,
    GET_TASK_DEPENDENCY_GRAPH_TOOL,
];
pub const TASK_APPROVAL_EXEMPT_TOOLS: [&str; 4] = [
    CREATE_TASK_TOOL,
    CREATE_TASKS_TOOL,
    CANCEL_TASK_TOOL,
    RESOLVE_TASK_FOLLOWUP_TOOL,
];

pub fn task_model_tools() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "name": LIST_TASKS_TOOL,
            "description": "List durable tasks created from the current conversation/project. Use this before answering any question about current Task state instead of relying on conversation history or retained Memory. Results are newest-first, mark superseded historical Tasks, and never include another conversation or account.",
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
            "description": "Get a durable task by task_id and resolve its complete replacement chain to the current leaf Task(s). Always use current_tasks for the current status; requested_task may be a cancelled or otherwise superseded historical Task.",
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
            "description": "Create one coherent durable task for the current conversation/project. Use this only when splitting would not enable useful concurrency, specialization, ownership, or independent acceptance. The Rust Local Agent Host binds, persists, schedules, and writes the result back locally.",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "minLength": 1},
                    "objective": {"type": "string", "minLength": 1},
                    "expected_outputs": task_contract_list_schema("Concrete artifacts, decisions, reports, or verified changes this Task must return."),
                    "acceptance_criteria": task_contract_list_schema("Observable, evidence-based conditions that must be true before this Task can be reported complete."),
                    "final_response_max_chars": {"type":"integer", "minimum":1, "maximum":32000, "description":"Optional explicit maximum for the entire final user-facing response, counted in Unicode scalar values including whitespace, punctuation and Markdown. Set this when the user requires a bounded textual delivery; not a file-content limit."},
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
                        "description": "Optional explicit model configuration id for this Task. When omitted, inherit the model selected for the current Main Chat."
                    },
                    "thinking_level": thinking_level_override_schema(),
                    "requires_execution": {
                        "type": "boolean",
                        "description": "Whether this Task needs command execution, tests, builds, Git operations, or file mutation. Project reads remain available without an execution workspace."
                    },
                    "enabled_builtin_kinds": builtin_kind_selection_schema(),
                    "notepad_user_request_quote": notepad_user_request_quote_schema(),
                    "external_mcp_config_ids": {
                        "type": "array",
                        "maxItems": 0,
                        "items": {"type": "string"},
                        "description": "External MCP configurations are not locally selectable yet; send an empty array."
                    },
                    "plugin_hints": plugin_hints_schema(),
                    "owned_paths": task_owned_paths_schema(),
                    "prerequisite_task_ids": prerequisite_task_ids_schema(),
                    "supersedes_task_ids": supersedes_task_ids_schema(),
                    "schedule": task_schedule_schema()
                },
                "required": ["title", "objective", "expected_outputs", "acceptance_criteria", "requires_execution", "enabled_builtin_kinds"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": CREATE_TASKS_TOOL,
            "description": "Create a durable local multi-Task graph for a compound outcome. Use distinct deliverables, specialist needs, safe parallel work, real prerequisite stages, or independent verification as decomposition boundaries. Each Task uses a unique client_ref and prerequisite_refs may only reference Tasks in this call.",
            "parameters": {
                "type": "object",
                "properties": {
                    "supersedes_task_ids": supersedes_task_ids_schema(),
                    "graph_goal": {"type": "string", "minLength": 1, "maxLength": 8000},
                    "decomposition_rationale": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 8000,
                        "description": "Why these Tasks have distinct outputs or ownership and how the graph enables specialization, concurrency, dependencies, or independent acceptance."
                    },
                    "notification_policy": graph_notification_policy_schema(),
                    "final_delivery_ref": {
                        "type": "string", "minLength": 1,
                        "description": "Optional client_ref of the final synthesis/delivery Task. It must depend directly or transitively on every other Task. Its full user-facing result is delivered once to Main Chat instead of excerpts of all reports."
                    },
                    "tasks": {
                        "type": "array",
                        "minItems": 2,
                        "maxItems": 50,
                        "items": create_task_item_schema()
                    }
                },
                "required": ["graph_goal", "decomposition_rationale", "tasks"],
                "additionalProperties": false
            }
        }),
        json!({
            "type": "function",
            "name": CANCEL_TASK_TOOL,
            "description": "Cancel a pending or running task from the current conversation/project because it conflicts with the user's latest intent. When the current Run contains a pending task_followup_review, prefer resolve_task_followup so every candidate is decided together. A direct cancellation is reconciled atomically with that Task's review, but every remaining pending candidate must still be resolved before replacement work can be created.",
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
        super::task_followup_tool::definition(),
    ]
}

fn task_contract_list_schema(description: &str) -> Value {
    json!({
        "type": "array",
        "minItems": 1,
        "maxItems": 50,
        "items": {"type": "string", "minLength": 1, "maxLength": 4000},
        "description": description
    })
}

fn supersedes_task_ids_schema() -> Value {
    json!({
        "type": "array", "maxItems": 50, "uniqueItems": true,
        "items": {"type": "string", "minLength": 1},
        "description": "Ids of affected old tasks already confirmed cancelled with cancel_task. Creates corrected work instead of reusing the old source graph; all ids must belong to this conversation. Omit for ordinary creation."
    })
}

fn thinking_level_override_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["none", "auto", "minimal", "low", "medium", "high", "xhigh", "max"],
        "description": "Optional reasoning level override for this Task. Omit it to use the selected model configuration's default Thinking level. Use medium for implementation from an established diagnosis or checklist and for routine browser acceptance. Use high only when the Task must resolve substantial ambiguity, compare multiple plausible architectures or causes, or make a consequential synthesis; many steps, Plugin use, and broad regression coverage alone do not justify high."
    })
}

fn builtin_kind_selection_schema() -> Value {
    json!({
        "type": "array",
        "items": {"type": "string", "enum": TASK_SELECTABLE_BUILTIN_KIND_VALUES},
        "uniqueItems": true,
        "description": "Select only capabilities explicitly required by the user's requested deliverable or named input. Never add a capability because it might be useful for scratch work or generic persistence. In particular, select Notepad only when the user explicitly requests a Notepad record or identifies an existing note as input. AskUser is added automatically when required by policy; CodeMaintainerWrite implies CodeMaintainerRead; RequirementSurveyWrite implies RequirementSurveyRead."
    })
}

fn notepad_user_request_quote_schema() -> Value {
    json!({
        "type": "string",
        "minLength": 2,
        "maxLength": 500,
        "description": "Required only when enabled_builtin_kinds contains Notepad and Notepad was not explicitly selected in the UI. Copy an exact phrase from the current user message that asks to record/save something in Notepad or identifies an existing note as input. Do not paraphrase and do not use generic task text as evidence."
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
                "component_key": {"type": "string", "minLength": 1},
                "reason": {"type": "string", "maxLength": 1000}
            },
            "required": ["plugin_key", "component_key"],
            "additionalProperties": false
        }
    })
}

fn graph_notification_policy_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["progress_summary", "all_tasks", "milestones", "graph_summary"],
        "default": "progress_summary",
        "description": "progress_summary (default) shows a short per-Run status message at actual execution start, updates it at completion/failure/block/cancel, then delivers one full final result. graph_summary is opt-in quiet mode; milestones emits marked milestones, failures/blockers and a final summary; all_tasks preserves legacy Task callbacks."
    })
}

fn create_task_item_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "client_ref": {"type": "string", "minLength": 1},
            "title": {"type": "string", "minLength": 1},
            "objective": {"type": "string", "minLength": 1},
            "expected_outputs": task_contract_list_schema("Concrete artifacts, decisions, reports, or verified changes owned by this Task."),
            "acceptance_criteria": task_contract_list_schema("Observable, evidence-based completion conditions for this Task."),
            "final_response_max_chars": {"type":"integer", "minimum":1, "maximum":32000, "description":"Optional explicit maximum for the entire final user-facing response, counted in Unicode scalar values including whitespace, punctuation and Markdown. Set this when the user requires a bounded textual delivery; not a file-content limit."},
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
                "description": "Optional explicit model configuration id for this Task. When omitted, inherit the model selected for the current Main Chat."
            },
            "thinking_level": thinking_level_override_schema(),
            "requires_execution": {"type": "boolean"},
            "enabled_builtin_kinds": builtin_kind_selection_schema(),
            "notepad_user_request_quote": notepad_user_request_quote_schema(),
            "external_mcp_config_ids": {
                "type": "array",
                "maxItems": 0,
                "items": {"type": "string"}
            },
            "plugin_hints": plugin_hints_schema(),
            "owned_paths": task_owned_paths_schema(),
            "notification_milestone": notification_milestone_schema(),
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
        "required": ["client_ref", "title", "objective", "expected_outputs", "acceptance_criteria", "requires_execution", "enabled_builtin_kinds"],
        "additionalProperties": false
    })
}

fn task_owned_paths_schema() -> Value {
    json!({
        "type": "array", "maxItems": 200,
        "items": {"type": "string", "minLength": 1, "maxLength": 4096},
        "uniqueItems": true,
        "description": "Normalized project-relative files or directories this Task may mutate through built-ins or Plugins. Independent writers must use disjoint paths; add a prerequisite when two Tasks may touch the same file. For CodeMaintainerWrite, an omitted or empty list claims the whole project. Plugin-only Tasks with no known project-file mutation may use an empty list and are isolated by Plugin runtime."
    })
}

fn notification_milestone_schema() -> Value {
    json!({
        "type": "boolean",
        "default": false,
        "description": "Under milestones policy, publish this Task's start and terminal result as an important user-visible milestone."
    })
}

fn prerequisite_task_ids_schema() -> Value {
    json!({
        "type": "array",
        "items": {"type": "string", "minLength": 1},
        "uniqueItems": true,
        "description": "Existing Task ids that must complete successfully before this Task runs. Use only ids returned by Task tools for the current user and project."
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
        "description": "Optional scheduling request. Main Chat Tasks retain contact_async behavior; a supplied run_at delays execution."
    })
}
