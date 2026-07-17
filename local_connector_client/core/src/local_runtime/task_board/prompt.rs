// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::LocalTaskBoardTaskRecord;

pub(crate) fn format_local_task_board_prompt(tasks: &[LocalTaskBoardTaskRecord]) -> String {
    if tasks.is_empty() {
        return String::new();
    }
    let mut lines = vec![
        "[Local Task Board]".to_string(),
        "Use this SQLite task board as the current execution state. Keep it updated with task_manager tools."
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
