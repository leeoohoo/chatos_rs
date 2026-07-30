// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::local_runtime::task_board::LocalTaskBoardTaskRecord;

fn task(
    id: &str,
    task_kind: &str,
    status: &str,
    source_user_message_id: Option<&str>,
) -> LocalTaskBoardTaskRecord {
    LocalTaskBoardTaskRecord {
        id: id.to_string(),
        conversation_id: "session-1".to_string(),
        conversation_turn_id: "turn-1".to_string(),
        source_session_id: "session-1".to_string(),
        source_turn_id: "turn-1".to_string(),
        source_user_message_id: source_user_message_id.map(str::to_string),
        title: id.to_string(),
        details: String::new(),
        priority: "medium".to_string(),
        status: status.to_string(),
        tags: Vec::new(),
        prerequisite_task_ids: Vec::new(),
        due_at: None,
        outcome_summary: String::new(),
        outcome_items: Vec::new(),
        resume_hint: String::new(),
        blocker_reason: String::new(),
        blocker_needs: Vec::new(),
        blocker_kind: String::new(),
        completed_at: None,
        last_outcome_at: None,
        created_at: "2026-07-29T00:00:00Z".to_string(),
        updated_at: "2026-07-29T00:00:00Z".to_string(),
        task_kind: task_kind.to_string(),
        objective: String::new(),
        model_config_id: None,
        is_planning_task: false,
        enabled_builtin_kinds: Vec::new(),
        external_mcp_config_ids: Vec::new(),
        selected_skill_ids: Vec::new(),
        last_run_id: None,
        project_work_item_id: None,
        requirement_id: None,
        execution_group_id: None,
        execution_client_ref: None,
        dependency_context_refs: Vec::new(),
        manager_scope: None,
        task_session_id: None,
        required_for_parent_completion: false,
        closure_state: None,
        closure_reason: None,
        idempotency_key: None,
        lifecycle_updated_at: None,
    }
}

#[test]
fn active_message_tasks_ignore_run_checklist_tasks() {
    let summary = active_message_task_summary(vec![
        task("parent", "task_runner", "done", Some("message-1")),
        task("checklist", "task_manager", "doing", Some("message-1")),
    ]);
    assert_eq!(summary["items"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        summary["active_source_user_message_ids"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    assert_eq!(
        summary["running_source_user_message_ids"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
}

#[test]
fn active_message_tasks_distinguish_ready_from_running() {
    let summary = active_message_task_summary(vec![
        task("queued-parent", "task_runner", "todo", Some("message-1")),
        task("running-parent", "task_runner", "doing", Some("message-2")),
    ]);
    let active_ids = summary["active_source_user_message_ids"]
        .as_array()
        .expect("active ids");
    let running_ids = summary["running_source_user_message_ids"]
        .as_array()
        .expect("running ids");
    assert_eq!(active_ids.len(), 2);
    assert_eq!(running_ids.len(), 1);
    assert_eq!(running_ids[0].as_str(), Some("message-2"));
}
