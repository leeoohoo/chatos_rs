// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::LocalTaskBoardTaskRecord;

pub(crate) fn format_local_task_board_prompt(tasks: &[LocalTaskBoardTaskRecord]) -> String {
    if tasks.is_empty() {
        return String::new();
    }
    let mut lines = vec![
        "[Local Task Board]".to_string(),
        "Use this SQLite task board as read-only execution context. No task-board MCP tools are available in current Task Runner executions."
            .to_string(),
    ];
    for task in tasks {
        let marker = match task.status.as_str() {
            "done" => "x",
            "blocked" => "!",
            "doing" => ">",
            _ => " ",
        };
        let mut line = format!(
            "- [{marker}] {} | {} | {} | {}",
            task.id, task.priority, task.status, task.title
        );
        if !task.prerequisite_task_ids.is_empty() {
            line.push_str(format!(" | after={}", task.prerequisite_task_ids.join(",")).as_str());
        }
        if !task.outcome_summary.trim().is_empty() {
            line.push_str(format!(" | outcome={}", task.outcome_summary.trim()).as_str());
        }
        if !task.blocker_reason.trim().is_empty() {
            line.push_str(format!(" | blocked_by={}", task.blocker_reason.trim()).as_str());
        }
        lines.push(line);
    }
    lines.join("\n")
}

pub(crate) fn format_local_task_runner_context_prompt(
    tasks: &[LocalTaskBoardTaskRecord],
    active_parent_task_id: &str,
) -> String {
    let tasks = tasks
        .iter()
        .filter(|task| task.task_kind != "task_manager")
        .collect::<Vec<_>>();
    if tasks.is_empty() {
        return String::new();
    }
    let mut lines = vec![
        "[Local Task Runner Parent Context]".to_string(),
        "Read-only conversation-level task context. No task-board MCP tools are available in this Task Runner execution; complete the requested work directly with the exposed tools."
            .to_string(),
        "Treat the active parent item below as context for deciding what work to complete directly."
            .to_string(),
    ];
    for task in tasks {
        let marker = match task.status.as_str() {
            "done" => "x",
            "blocked" => "!",
            "doing" => ">",
            _ => " ",
        };
        let active = if task.id == active_parent_task_id {
            " | active_parent=true"
        } else {
            ""
        };
        let mut line = format!(
            "- [{marker}] {} | {} | {}{}",
            task.priority, task.status, task.title, active
        );
        if !task.outcome_summary.trim().is_empty() {
            line.push_str(format!(" | outcome={}", task.outcome_summary.trim()).as_str());
        }
        if !task.blocker_reason.trim().is_empty() {
            line.push_str(format!(" | blocked_by={}", task.blocker_reason.trim()).as_str());
        }
        lines.push(line);
    }
    lines.join("\n")
}
