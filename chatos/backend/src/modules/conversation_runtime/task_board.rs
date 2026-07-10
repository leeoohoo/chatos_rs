// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use serde_json::Value;

use crate::core::internal_context_locale::InternalContextLocale;
use crate::services::task_board_prompt::{
    build_runtime_prefixed_input_items, format_task_board_prompt,
};
use crate::services::task_manager::{list_tasks_for_context, TaskRecord};
use crate::utils::events::Events;

#[path = "task_board/snapshot.rs"]
mod snapshot;

use self::snapshot::sync_task_board_turn_snapshot;
use super::guidance;
use super::user_context::load_runtime_user_context;

const TASK_BOARD_ACTIVE_LIMIT: usize = 200;
const TASK_BOARD_DONE_HISTORY_LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskTurnFollowUpMode {
    ContinueExecution,
    ReviewExecution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskTurnReviewOutcome {
    Pass,
    NeedsMoreWork,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct TaskTurnFollowUpDirective {
    pub mode: TaskTurnFollowUpMode,
    pub locale: InternalContextLocale,
    pub guidance: String,
}

#[derive(Debug, Clone)]
pub struct TaskBoardRuntimeContext {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub locale: InternalContextLocale,
    pub contact_system_prompt: Option<String>,
    pub builtin_mcp_system_prompt: Option<String>,
    pub command_system_prompt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RefreshedTaskBoardRuntime {
    pub updated_event: Value,
}

pub async fn build_task_board_prompt(
    session_id: &str,
    turn_id: Option<&str>,
    locale: InternalContextLocale,
) -> Option<String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return None;
    }

    let tasks = load_task_board_context_tasks(session_id, turn_id).await;
    Some(format_task_board_prompt(tasks.as_slice(), locale))
        .filter(|content| !content.trim().is_empty())
}

pub async fn build_task_turn_follow_up_directive(
    session_id: &str,
    turn_id: &str,
) -> Option<TaskTurnFollowUpDirective> {
    let session_id = session_id.trim();
    let turn_id = turn_id.trim();
    if session_id.is_empty() || turn_id.is_empty() {
        return None;
    }

    let locale = load_runtime_user_context(None, session_id).await.locale;
    let tasks = load_task_board_context_tasks(session_id, Some(turn_id)).await;
    classify_task_turn_follow_up(tasks.as_slice(), locale)
}

async fn load_task_board_context_tasks(session_id: &str, turn_id: Option<&str>) -> Vec<TaskRecord> {
    let active_tasks = list_tasks_for_context(session_id, turn_id, false, TASK_BOARD_ACTIVE_LIMIT)
        .await
        .unwrap_or_default();
    let done_candidates =
        list_tasks_for_context(session_id, turn_id, true, TASK_BOARD_DONE_HISTORY_LIMIT)
            .await
            .unwrap_or_default();
    merge_task_board_context_tasks(active_tasks, done_candidates)
}

fn merge_task_board_context_tasks(
    mut active_tasks: Vec<TaskRecord>,
    done_candidates: Vec<TaskRecord>,
) -> Vec<TaskRecord> {
    let mut seen = active_tasks
        .iter()
        .map(|task| task.id.clone())
        .collect::<HashSet<_>>();

    for task in done_candidates {
        if !is_done_status(task.status.as_str()) {
            continue;
        }
        if seen.insert(task.id.clone()) {
            active_tasks.push(task);
        }
    }

    active_tasks
}

pub fn classify_task_turn_follow_up(
    tasks: &[crate::services::task_manager::TaskRecord],
    locale: InternalContextLocale,
) -> Option<TaskTurnFollowUpDirective> {
    if tasks.is_empty() {
        return None;
    }

    let unfinished_count = tasks
        .iter()
        .filter(|task| is_unfinished_status(task.status.as_str()))
        .count();
    let blocked_count = tasks
        .iter()
        .filter(|task| is_blocked_status(task.status.as_str()))
        .count();
    let done_count = tasks
        .iter()
        .filter(|task| is_done_status(task.status.as_str()))
        .count();
    let mode = if unfinished_count > 0 {
        TaskTurnFollowUpMode::ContinueExecution
    } else {
        TaskTurnFollowUpMode::ReviewExecution
    };
    let board_prompt = format_task_board_prompt(tasks, locale);

    Some(TaskTurnFollowUpDirective {
        mode,
        locale,
        guidance: build_task_turn_follow_up_guidance(
            locale,
            mode,
            unfinished_count,
            blocked_count,
            done_count,
            board_prompt.as_str(),
        ),
    })
}

pub fn parse_task_turn_review_outcome(content: &str) -> TaskTurnReviewOutcome {
    let first_line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_ascii_lowercase();
    let marker = first_line
        .strip_prefix("task_review:")
        .or_else(|| first_line.strip_prefix("task-review:"))
        .map(str::trim)
        .unwrap_or(first_line.as_str());

    if marker.starts_with("pass") {
        TaskTurnReviewOutcome::Pass
    } else if marker.contains("needs_more_work") {
        TaskTurnReviewOutcome::NeedsMoreWork
    } else {
        TaskTurnReviewOutcome::Unknown
    }
}

pub fn strip_task_turn_review_marker(content: &str) -> String {
    let mut lines = content.lines();
    let Some(first_line) = lines.next() else {
        return String::new();
    };
    let normalized = first_line.trim().to_ascii_lowercase();
    if normalized.starts_with("task_review:") || normalized.starts_with("task-review:") {
        return lines.collect::<Vec<_>>().join("\n").trim().to_string();
    }
    content.trim().to_string()
}

pub fn build_hidden_task_turn_review_metadata() -> Value {
    serde_json::json!({
        "hidden": true,
        "task_review": {
            "mode": "internal"
        }
    })
}

pub fn build_task_turn_follow_up_message(guidance: &str) -> Value {
    serde_json::json!([{
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": guidance
        }]
    }])
}

pub fn build_task_turn_review_retry_guidance(locale: InternalContextLocale) -> String {
    if locale.is_english() {
        "The review found remaining issues. Continue in the same turn and fix them before you summarize again."
            .to_string()
    } else {
        "复查发现仍有问题。请继续在同一轮内修正，完成后再重新总结。".to_string()
    }
}

#[cfg(test)]
pub fn build_runtime_context(
    session_id: Option<String>,
    turn_id: Option<String>,
    locale: InternalContextLocale,
    contact_system_prompt: Option<String>,
    builtin_mcp_system_prompt: Option<String>,
    command_system_prompt: Option<String>,
) -> Option<TaskBoardRuntimeContext> {
    let session_id = session_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    let turn_id = turn_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Some(TaskBoardRuntimeContext {
        session_id,
        turn_id,
        locale,
        contact_system_prompt,
        builtin_mcp_system_prompt,
        command_system_prompt,
    })
}

pub async fn load_prefixed_input_items(context: &TaskBoardRuntimeContext) -> Option<Vec<Value>> {
    build_runtime_prefixed_input_items_for_turn(
        &context.session_id,
        context.turn_id.as_deref(),
        context.locale,
        context.contact_system_prompt.as_deref(),
        context.builtin_mcp_system_prompt.as_deref(),
        context.command_system_prompt.as_deref(),
    )
    .await
}

pub async fn build_runtime_prefixed_input_items_for_turn(
    session_id: &str,
    turn_id: Option<&str>,
    locale: InternalContextLocale,
    contact_system_prompt: Option<&str>,
    builtin_mcp_system_prompt: Option<&str>,
    command_system_prompt: Option<&str>,
) -> Option<Vec<Value>> {
    let task_board_prompt = build_task_board_prompt(session_id, turn_id, locale).await;
    build_runtime_prefixed_input_items(
        task_board_prompt.as_deref(),
        contact_system_prompt,
        builtin_mcp_system_prompt,
        command_system_prompt,
    )
}

pub async fn refresh_task_board_runtime_outcome(
    session_id: &str,
    turn_id: &str,
) -> Option<RefreshedTaskBoardRuntime> {
    let session_id = session_id.trim();
    let turn_id = turn_id.trim();
    if session_id.is_empty() || turn_id.is_empty() {
        return None;
    }

    let locale = load_runtime_user_context(None, session_id).await.locale;

    let prompt = build_task_board_prompt(session_id, Some(turn_id), locale).await?;
    if let Some(guidance) = build_task_board_runtime_guidance(prompt.as_str(), locale) {
        let _ = guidance::enqueue_runtime_guidance(session_id, turn_id, guidance.as_str());
    }
    let _ = sync_task_board_turn_snapshot(session_id, turn_id, prompt.as_str()).await;
    Some(RefreshedTaskBoardRuntime {
        updated_event: build_task_board_updated_event(session_id, turn_id, prompt.as_str()),
    })
}

pub fn build_task_board_updated_event(
    session_id: &str,
    turn_id: &str,
    task_board_prompt: &str,
) -> Value {
    serde_json::json!({
        "event": Events::TASK_BOARD_UPDATED,
        "data": {
            "conversation_id": session_id,
            "conversation_turn_id": turn_id,
            "task_board": task_board_prompt,
        }
    })
}

pub fn build_task_board_runtime_guidance(
    task_board_prompt: &str,
    locale: InternalContextLocale,
) -> Option<String> {
    let task_board_prompt = task_board_prompt.trim();
    if task_board_prompt.is_empty() {
        return None;
    }

    Some(if locale.is_english() {
        format!(
            "[Task Board Updated]\n- source: system task board refresh after task mutation\n- rule: replace any stale task assumptions with the latest board below\n- instruction: continue strictly based on this refreshed board\n\n{}",
            task_board_prompt
        )
    } else {
        format!(
            "[Task Board Updated]\n- source: 系统在任务变更后刷新了任务看板\n- rule: 用下方最新看板替换任何过时的任务判断\n- instruction: 严格基于这份刷新后的看板继续执行\n\n{}",
            task_board_prompt
        )
    })
}

fn build_task_turn_follow_up_guidance(
    locale: InternalContextLocale,
    mode: TaskTurnFollowUpMode,
    unfinished_count: usize,
    blocked_count: usize,
    done_count: usize,
    task_board_prompt: &str,
) -> String {
    match mode {
        TaskTurnFollowUpMode::ContinueExecution => {
            if locale.is_english() {
                format!(
                    "The previous assistant response ended too early. Continue in the same turn and finish every non-blocked task before you summarize again. Unfinished tasks: {}. Blocked tasks (ignored for this check): {}. Done tasks: {}.\n\nLatest task board:\n{}",
                    unfinished_count, blocked_count, done_count, task_board_prompt
                )
            } else {
                format!(
                    "上一轮助手已经提前总结了，但当前轮还有未完成任务。请继续在同一轮内执行，先把所有非阻塞任务做完，再重新总结。未完成任务：{}。阻塞任务（本次检查忽略）：{}。已完成任务：{}。\n\n最新任务看板：\n{}",
                    unfinished_count, blocked_count, done_count, task_board_prompt
                )
            }
        }
        TaskTurnFollowUpMode::ReviewExecution => {
            if locale.is_english() {
                format!(
                    "The visible tasks now look complete. Review this turn in the same conversation before we finish. Blocked tasks do not count as unfinished. Output exactly one first line: `TASK_REVIEW: pass` or `TASK_REVIEW: needs_more_work`. Then add a short explanation.\n\nLatest task board:\n{}",
                    task_board_prompt
                )
            } else {
                format!(
                    "当前看板里的非阻塞任务看起来都已完成。请在同一轮对话里复查，确认是否真的完成；阻塞任务不计入未完成。请先输出一行精确结果：`TASK_REVIEW: pass` 或 `TASK_REVIEW: needs_more_work`，然后再给简短说明。\n\n最新任务看板：\n{}",
                    task_board_prompt
                )
            }
        }
    }
}

fn is_unfinished_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "todo" | "doing"
    )
}

fn is_blocked_status(status: &str) -> bool {
    status.trim().eq_ignore_ascii_case("blocked")
}

fn is_done_status(status: &str) -> bool {
    status.trim().eq_ignore_ascii_case("done")
}

#[cfg(test)]
mod tests {
    use super::{
        build_hidden_task_turn_review_metadata, build_runtime_context,
        classify_task_turn_follow_up, parse_task_turn_review_outcome,
        strip_task_turn_review_marker, TaskTurnFollowUpMode, TaskTurnReviewOutcome,
    };
    use crate::core::internal_context_locale::InternalContextLocale;
    use crate::services::task_manager::TaskRecord;
    use serde_json::Value;

    fn build_task_record(id: &str, status: &str) -> TaskRecord {
        TaskRecord {
            id: id.to_string(),
            conversation_id: "session-1".to_string(),
            conversation_turn_id: "turn-1".to_string(),
            title: id.to_string(),
            details: String::new(),
            priority: "medium".to_string(),
            status: status.to_string(),
            tags: Vec::new(),
            due_at: None,
            outcome_summary: String::new(),
            outcome_items: Vec::new(),
            resume_hint: String::new(),
            blocker_reason: String::new(),
            blocker_needs: Vec::new(),
            blocker_kind: String::new(),
            completed_at: None,
            last_outcome_at: None,
            created_at: "2026-05-21T00:00:00Z".to_string(),
            updated_at: "2026-05-21T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn merge_task_board_context_keeps_active_tasks_and_unique_done_history() {
        let merged = super::merge_task_board_context_tasks(
            vec![
                build_task_record("todo-1", "todo"),
                build_task_record("done-duplicate", "done"),
            ],
            vec![
                build_task_record("todo-from-done-query", "todo"),
                build_task_record("done-duplicate", "done"),
                build_task_record("done-2", "done"),
            ],
        );

        let ids = merged
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["todo-1", "done-duplicate", "done-2"]);
    }

    #[test]
    fn build_runtime_context_trims_session_and_turn() {
        let context = build_runtime_context(
            Some("  session-1  ".to_string()),
            Some("  turn-1  ".to_string()),
            InternalContextLocale::EnUs,
            Some("contact".to_string()),
            Some("builtin".to_string()),
            Some("command".to_string()),
        )
        .expect("context should be present");

        assert_eq!(context.session_id, "session-1");
        assert_eq!(context.turn_id.as_deref(), Some("turn-1"));
    }

    #[test]
    fn classify_task_turn_follow_up_prefers_continue_when_unfinished_exists() {
        let tasks = vec![
            TaskRecord {
                id: "1".to_string(),
                conversation_id: "session-1".to_string(),
                conversation_turn_id: "turn-1".to_string(),
                title: "A".to_string(),
                details: String::new(),
                priority: "medium".to_string(),
                status: "doing".to_string(),
                tags: Vec::new(),
                due_at: None,
                outcome_summary: String::new(),
                outcome_items: Vec::new(),
                resume_hint: String::new(),
                blocker_reason: String::new(),
                blocker_needs: Vec::new(),
                blocker_kind: String::new(),
                completed_at: None,
                last_outcome_at: None,
                created_at: "2026-05-21T00:00:00Z".to_string(),
                updated_at: "2026-05-21T00:00:00Z".to_string(),
            },
            TaskRecord {
                id: "2".to_string(),
                conversation_id: "session-1".to_string(),
                conversation_turn_id: "turn-1".to_string(),
                title: "B".to_string(),
                details: String::new(),
                priority: "medium".to_string(),
                status: "blocked".to_string(),
                tags: Vec::new(),
                due_at: None,
                outcome_summary: String::new(),
                outcome_items: Vec::new(),
                resume_hint: String::new(),
                blocker_reason: String::new(),
                blocker_needs: Vec::new(),
                blocker_kind: String::new(),
                completed_at: None,
                last_outcome_at: None,
                created_at: "2026-05-21T00:00:00Z".to_string(),
                updated_at: "2026-05-21T00:00:00Z".to_string(),
            },
        ];

        let directive = classify_task_turn_follow_up(tasks.as_slice(), InternalContextLocale::ZhCn)
            .expect("directive should exist");
        assert_eq!(directive.mode, TaskTurnFollowUpMode::ContinueExecution);
        assert!(directive.guidance.contains("未完成任务：1"));
        assert!(directive.guidance.contains("阻塞任务（本次检查忽略）：1"));
        assert!(directive.guidance.contains("已完成任务：0"));
        assert!(directive.guidance.contains("未完成任务"));
    }

    #[test]
    fn classify_task_turn_follow_up_switches_to_review_when_all_non_blocked_done() {
        let tasks = vec![TaskRecord {
            id: "1".to_string(),
            conversation_id: "session-1".to_string(),
            conversation_turn_id: "turn-1".to_string(),
            title: "A".to_string(),
            details: String::new(),
            priority: "medium".to_string(),
            status: "done".to_string(),
            tags: Vec::new(),
            due_at: None,
            outcome_summary: String::new(),
            outcome_items: Vec::new(),
            resume_hint: String::new(),
            blocker_reason: String::new(),
            blocker_needs: Vec::new(),
            blocker_kind: String::new(),
            completed_at: None,
            last_outcome_at: None,
            created_at: "2026-05-21T00:00:00Z".to_string(),
            updated_at: "2026-05-21T00:00:00Z".to_string(),
        }];

        let directive = classify_task_turn_follow_up(tasks.as_slice(), InternalContextLocale::EnUs)
            .expect("directive should exist");
        assert_eq!(directive.mode, TaskTurnFollowUpMode::ReviewExecution);
        assert!(directive.guidance.contains("TASK_REVIEW: pass"));
    }

    #[test]
    fn parse_task_turn_review_outcome_reads_first_line_marker() {
        assert_eq!(
            parse_task_turn_review_outcome("TASK_REVIEW: pass\nlooks good"),
            TaskTurnReviewOutcome::Pass
        );
        assert_eq!(
            parse_task_turn_review_outcome("task_review: needs_more_work"),
            TaskTurnReviewOutcome::NeedsMoreWork
        );
    }

    #[test]
    fn strip_task_turn_review_marker_removes_protocol_line() {
        assert_eq!(
            strip_task_turn_review_marker("TASK_REVIEW: pass\nlooks good"),
            "looks good"
        );
        assert_eq!(
            strip_task_turn_review_marker("task-review: needs_more_work\nfix it"),
            "fix it"
        );
        assert_eq!(strip_task_turn_review_marker("plain text"), "plain text");
    }

    #[test]
    fn hidden_task_turn_review_metadata_marks_message_hidden() {
        let metadata = build_hidden_task_turn_review_metadata();
        assert_eq!(metadata.get("hidden").and_then(Value::as_bool), Some(true));
        assert_eq!(
            metadata
                .get("task_review")
                .and_then(|value| value.get("mode"))
                .and_then(Value::as_str),
            Some("internal")
        );
    }
}
