// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
            "prerequisite_task_id": {
                "type": "string",
                "description": "Optional existing task id that must be completed before this task when there is a clear execution order."
            },
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
            },
            "scope": {
                "type": "string",
                "enum": ["run_checklist", "durable_followup"],
                "description": "run_checklist belongs to the current execution session; durable_followup survives independently after the current run."
            },
            "required_for_parent_completion": {
                "type": "boolean",
                "description": "Whether a run_checklist entry blocks parent completion. Ignored for durable_followup."
            },
            "idempotency_key": {
                "type": "string",
                "description": "Stable semantic key used to reuse an existing task in the current session instead of creating a duplicate."
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
            "prerequisite_task_id": {
                "type": "string",
                "description": "Optional existing task id that must be completed before this task when there is a clear execution order."
            },
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
            },
            "scope": {
                "type": "string",
                "enum": ["run_checklist", "durable_followup"],
                "description": "run_checklist belongs to the current execution session; durable_followup survives independently after the current run."
            },
            "required_for_parent_completion": {
                "type": "boolean",
                "description": "Whether a run_checklist entry blocks parent completion. Ignored for durable_followup."
            },
            "idempotency_key": {
                "type": "string",
                "description": "Stable semantic key used to reuse an existing task in the current session instead of creating a duplicate."
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
