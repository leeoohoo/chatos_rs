// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn prompt_tool_definitions() -> Vec<Value> {
    vec![
        tool_definition(
            "list_prompts",
            "List ask_user prompts emitted during Task Runner execution.",
            json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "run_id": { "type": "string" },
                    "status": { "type": "string", "enum": prompt_status_values() }
                },
                "additionalProperties": false
            }),
        ),
        tool_definition(
            "get_prompt",
            "Get one Task Runner ask user prompt by id.",
            required_object_schema(
                json!({
                    "prompt_id": { "type": "string", "minLength": 1 }
                }),
                &["prompt_id"],
            ),
        ),
        tool_definition(
            "submit_prompt",
            "Submit values or selections for a pending Task Runner ask user prompt.",
            required_object_schema(
                json!({
                    "prompt_id": { "type": "string", "minLength": 1 },
                    "values": { "type": "object" },
                    "selection": {},
                    "reason": { "type": "string" }
                }),
                &["prompt_id"],
            ),
        ),
        tool_definition(
            "cancel_prompt",
            "Cancel a pending Task Runner ask user prompt if the prompt allows cancellation.",
            required_object_schema(
                json!({
                    "prompt_id": { "type": "string", "minLength": 1 },
                    "reason": { "type": "string" }
                }),
                &["prompt_id"],
            ),
        ),
    ]
}
