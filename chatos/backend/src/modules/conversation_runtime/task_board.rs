// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::warn;

use super::user_context::load_runtime_user_context;
use crate::core::internal_context_locale::InternalContextLocale;
use crate::services::task_runner_api_client;

#[derive(Debug, Clone, Deserialize)]
struct TaskRunnerMessageTasksResponse {
    #[serde(default)]
    items: Vec<TaskTurnTask>,
}

#[derive(Debug, Clone, Deserialize)]
struct TaskTurnTask {
    id: String,
    title: String,
    status: String,
    #[serde(default)]
    result_summary: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskTurnFollowUpMode {
    ContinueExecution,
    ReviewExecution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

#[cfg(test)]
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
    Some(format_task_turn_prompt(tasks.as_slice(), locale))
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

    let locale = load_runtime_user_context(None, session_id)
        .await
        .internal_context_locale;
    let tasks = load_task_board_context_tasks(session_id, Some(turn_id)).await;
    classify_task_turn_follow_up(tasks.as_slice(), locale)
}

async fn load_task_board_context_tasks(
    session_id: &str,
    turn_id: Option<&str>,
) -> Vec<TaskTurnTask> {
    let Some(turn_id) = turn_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Vec::new();
    };
    let payload =
        match task_runner_api_client::list_message_tasks("", session_id, None, Some(turn_id)).await
        {
            Ok(payload) => payload,
            Err(error) => {
                warn!(
                    session_id,
                    turn_id,
                    detail = error,
                    "Task Runner turn review lookup failed"
                );
                return Vec::new();
            }
        };
    match serde_json::from_value::<TaskRunnerMessageTasksResponse>(payload) {
        Ok(response) => response.items,
        Err(error) => {
            warn!(session_id, turn_id, detail = %error, "Task Runner turn review payload is invalid");
            Vec::new()
        }
    }
}

fn classify_task_turn_follow_up(
    tasks: &[TaskTurnTask],
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
    let board_prompt = format_task_turn_prompt(tasks, locale);

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

fn format_task_turn_prompt(tasks: &[TaskTurnTask], locale: InternalContextLocale) -> String {
    if tasks.is_empty() {
        return String::new();
    }
    let mut lines = vec![if locale.is_english() {
        "Task Runner tasks created for this turn:".to_string()
    } else {
        "当前轮次创建的 Task Runner 任务：".to_string()
    }];
    for task in tasks {
        lines.push(format!(
            "- [{}] {} (`{}`)",
            task.status.trim(),
            compact_task_text(task.title.as_str(), 240),
            task.id.trim()
        ));
        if let Some(summary) = task
            .result_summary
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            lines.push(format!(
                "  {}: {}",
                if locale.is_english() {
                    "Result"
                } else {
                    "结果"
                },
                compact_task_text(summary, 1_200)
            ));
        }
    }
    lines.join("\n")
}

fn compact_task_text(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = normalized.chars();
    let prefix = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
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

#[cfg(test)]
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
pub async fn build_runtime_prefixed_input_items_for_turn(
    session_id: &str,
    turn_id: Option<&str>,
    locale: InternalContextLocale,
    contact_system_prompt: Option<&str>,
    builtin_mcp_system_prompt: Option<&str>,
    command_system_prompt: Option<&str>,
) -> Option<Vec<Value>> {
    let task_board_prompt = build_task_board_prompt(session_id, turn_id, locale).await;
    let prompts = [
        contact_system_prompt,
        builtin_mcp_system_prompt,
        command_system_prompt,
        task_board_prompt.as_deref(),
    ];
    let items = prompts
        .into_iter()
        .filter_map(|prompt| prompt.map(str::trim).filter(|value| !value.is_empty()))
        .map(|text| {
            serde_json::json!({
                "type": "message",
                "role": "system",
                "content": [{ "type": "input_text", "text": text }]
            })
        })
        .collect::<Vec<_>>();
    (!items.is_empty()).then_some(items)
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
        "draft" | "ready" | "queued" | "running"
    )
}

fn is_blocked_status(status: &str) -> bool {
    status.trim().eq_ignore_ascii_case("blocked")
}

fn is_done_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "succeeded" | "failed" | "cancelled" | "archived"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_hidden_task_turn_review_metadata, classify_task_turn_follow_up, compact_task_text,
        format_task_turn_prompt, parse_task_turn_review_outcome, strip_task_turn_review_marker,
        TaskTurnFollowUpMode, TaskTurnReviewOutcome, TaskTurnTask,
    };
    use crate::core::internal_context_locale::InternalContextLocale;
    use serde_json::Value;

    fn build_task_record(id: &str, status: &str) -> TaskTurnTask {
        TaskTurnTask {
            id: id.to_string(),
            title: id.to_string(),
            status: status.to_string(),
            result_summary: None,
        }
    }

    #[test]
    fn task_turn_prompt_compacts_large_results() {
        let tasks = vec![TaskTurnTask {
            result_summary: Some("path/to/file ".repeat(1_000)),
            ..build_task_record("task-1", "succeeded")
        }];
        let prompt = format_task_turn_prompt(tasks.as_slice(), InternalContextLocale::ZhCn);
        assert!(prompt.contains("当前轮次创建的 Task Runner 任务"));
        assert!(prompt.contains("[succeeded]"));
        assert!(prompt.contains('…'));
        assert!(prompt.chars().count() < 1_600);
        assert_eq!(compact_task_text("  one\n two  ", 20), "one two");
    }

    #[test]
    fn classify_task_turn_follow_up_prefers_continue_when_unfinished_exists() {
        let tasks = vec![
            build_task_record("1", "running"),
            build_task_record("2", "blocked"),
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
        let tasks = vec![build_task_record("1", "succeeded")];

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
