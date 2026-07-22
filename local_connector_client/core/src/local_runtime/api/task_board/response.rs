// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::local_runtime::task_board::LocalTaskBoardTaskRecord;

pub(in crate::local_runtime::api) fn task_response(record: &LocalTaskBoardTaskRecord) -> Value {
    json!({
        "id": record.id,
        "title": record.title,
        "description": record.details,
        "objective": if record.objective.trim().is_empty() { &record.details } else { &record.objective },
        "status": record.status,
        "priority": match record.priority.as_str() { "high" => 10, "low" => -10, _ => 0 },
        "tags": record.tags,
        "result_summary": record.outcome_summary,
        "process_log": record.resume_hint,
        "last_run_id": record.last_run_id,
        "source_session_id": record.source_session_id,
        "source_turn_id": record.source_turn_id,
        "source_user_message_id": record.source_user_message_id,
        "prerequisite_task_ids": record.prerequisite_task_ids,
        "default_model_config_id": record.model_config_id,
        "mcp_config": {
            "enabled_builtin_kinds": record.enabled_builtin_kinds,
            "external_mcp_config_ids": record.external_mcp_config_ids,
            "selected_skill_ids": record.selected_skill_ids,
        },
        "task_kind": record.task_kind,
        "is_planning_task": record.is_planning_task,
        "task_tool_state": {
            "due_at": record.due_at,
            "outcome_items": record.outcome_items,
            "resume_hint": record.resume_hint,
            "blocker_reason": record.blocker_reason,
            "blocker_needs": record.blocker_needs,
            "blocker_kind": record.blocker_kind,
            "completed_at": record.completed_at,
            "last_outcome_at": record.last_outcome_at,
        },
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    })
}
