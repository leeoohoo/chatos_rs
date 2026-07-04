// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn run_tool_definitions() -> Vec<Value> {
    vec![
        tool_definition(
            "list_runs",
            "List Task Runner runs with optional task or status filters.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "status": { "type": "string", "enum": run_status_values() },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500 }
                },
                "additionalProperties": false
            }),
        ),
        tool_definition(
            "get_run",
            "Get one Task Runner run by id.",
            required_object_schema(
                json!({
                    "run_id": { "type": "string", "minLength": 1 }
                }),
                &["run_id"],
            ),
        ),
        tool_definition(
            "start_task_run",
            "Start a newly created Task Runner task that has not run before. Historical tasks are read-only; create a new task for new work or use cancel_task for obsolete work.",
            required_object_schema(
                json!({
                    "task_id": { "type": "string", "minLength": 1 },
                    "prompt_override": { "type": "string" }
                }),
                &["task_id"],
            ),
        ),
        tool_definition(
            "batch_start_task_runs",
            "Start newly created Task Runner tasks that have not run before.",
            required_object_schema(
                json!({
                    "task_ids": {
                        "type": "array",
                        "items": { "type": "string", "minLength": 1 },
                        "minItems": 1
                    },
                    "prompt_override": { "type": "string" }
                }),
                &["task_ids"],
            ),
        ),
        tool_definition(
            "get_task_memory_context",
            "Read the composed Memory Engine context and thread summary for one task.",
            required_object_schema(
                json!({
                    "task_id": { "type": "string", "minLength": 1 },
                    "include_recent_records": { "type": "boolean" },
                    "include_thread_summary": { "type": "boolean" },
                    "include_subject_memory": { "type": "boolean" },
                    "recent_record_limit": { "type": "integer", "minimum": 1, "maximum": 100 },
                    "summary_limit": { "type": "integer", "minimum": 1, "maximum": 50 }
                }),
                &["task_id"],
            ),
        ),
        tool_definition(
            "list_task_memory_records",
            "List Memory Engine records persisted for one Task Runner task thread.",
            required_object_schema(
                json!({
                    "task_id": { "type": "string", "minLength": 1 },
                    "role": { "type": "string" },
                    "record_type": { "type": "string" },
                    "summary_status": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 },
                    "offset": { "type": "integer", "minimum": 0 },
                    "order": { "type": "string", "enum": ["asc", "desc"] }
                }),
                &["task_id"],
            ),
        ),
        tool_definition(
            "summarize_task_memory",
            "Trigger a Memory Engine repair summary job for one task thread.",
            required_object_schema(
                json!({
                    "task_id": { "type": "string", "minLength": 1 }
                }),
                &["task_id"],
            ),
        ),
        tool_definition(
            "cancel_run",
            "Request cancellation for a running or queued Task Runner run.",
            required_object_schema(
                json!({
                    "run_id": { "type": "string", "minLength": 1 }
                }),
                &["run_id"],
            ),
        ),
        tool_definition(
            "retry_run",
            "Admin-only run maintenance tool. Chatos agents should create a new task instead of retrying a historical run.",
            required_object_schema(
                json!({
                    "run_id": { "type": "string", "minLength": 1 }
                }),
                &["run_id"],
            ),
        ),
        tool_definition(
            "list_run_events",
            "List stored execution events for one Task Runner run.",
            required_object_schema(
                json!({
                    "run_id": { "type": "string", "minLength": 1 }
                }),
                &["run_id"],
            ),
        ),
    ]
}
