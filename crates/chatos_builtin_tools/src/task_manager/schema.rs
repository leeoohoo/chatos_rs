use serde_json::{json, Value};

pub(super) fn task_payload_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tasks": {
                "type": "array",
                "items": task_item_schema()
            },
            "title": { "type": "string" },
            "details": { "type": "string" },
            "priority": { "type": "string", "enum": ["high", "medium", "low"] },
            "status": { "type": "string", "enum": ["todo", "doing", "blocked", "done"] },
            "tags": { "type": "array", "items": { "type": "string" } },
            "due_at": { "type": "string" },
            "outcome_summary": { "type": "string" },
            "outcome_items": {
                "type": "array",
                "items": outcome_item_schema()
            },
            "resume_hint": { "type": "string" },
            "blocker_reason": { "type": "string" },
            "blocker_needs": { "type": "array", "items": { "type": "string" } },
            "blocker_kind": {
                "type": "string",
                "enum": ["external_dependency", "permission", "missing_information", "design_decision", "environment_failure", "upstream_bug", "unknown"]
            }
        },
        "additionalProperties": false
    })
}

fn task_item_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "details": { "type": "string" },
            "priority": { "type": "string", "enum": ["high", "medium", "low"] },
            "status": { "type": "string", "enum": ["todo", "doing", "blocked", "done"] },
            "tags": { "type": "array", "items": { "type": "string" } },
            "due_at": { "type": "string" },
            "outcome_summary": { "type": "string" },
            "outcome_items": {
                "type": "array",
                "items": outcome_item_schema()
            },
            "resume_hint": { "type": "string" },
            "blocker_reason": { "type": "string" },
            "blocker_needs": { "type": "array", "items": { "type": "string" } },
            "blocker_kind": {
                "type": "string",
                "enum": ["external_dependency", "permission", "missing_information", "design_decision", "environment_failure", "upstream_bug", "unknown"]
            }
        },
        "required": ["title"],
        "additionalProperties": false
    })
}

pub(super) fn outcome_item_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "kind": { "type": "string" },
            "text": { "type": "string" },
            "importance": { "type": "string", "enum": ["high", "medium", "low"] },
            "refs": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["text"],
        "additionalProperties": false
    })
}
