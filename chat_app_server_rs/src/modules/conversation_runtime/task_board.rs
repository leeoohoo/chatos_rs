use serde_json::Value;

use crate::core::internal_context_locale::InternalContextLocale;
use crate::models::memory_runtime_types::{
    SyncTurnRuntimeSnapshotRequestDto, TurnRuntimeSnapshotLookupResponseDto,
    TurnRuntimeSnapshotSystemMessageDto,
};
use crate::services::chatos_sessions;
use crate::services::task_board_prompt::{
    build_runtime_prefixed_input_items, format_task_board_prompt,
};
use crate::services::task_manager::list_tasks_for_context;
use crate::services::text_normalization::normalize_optional_text_ref;
use crate::utils::events::Events;

use super::guidance;
use super::user_context::load_runtime_user_context;

const TASK_BOARD_LIMIT: usize = 200;

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
    #[allow(dead_code)]
    pub unfinished_count: usize,
    #[allow(dead_code)]
    pub blocked_count: usize,
    #[allow(dead_code)]
    pub done_count: usize,
}

#[derive(Debug, Clone)]
pub struct TaskBoardRuntimeContext {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub locale: InternalContextLocale,
    pub contact_system_prompt: Option<String>,
    pub builtin_mcp_system_prompt: Option<String>,
    pub command_system_prompt: Option<String>,
    pub task_runner_skill_prompt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RefreshedTaskBoardRuntime {
    pub updated_event: Value,
}

#[derive(Debug, Clone)]
struct TaskBoardSnapshotPatch {
    user_message_id: Option<String>,
    status: String,
    snapshot_source: String,
    snapshot_version: i64,
    captured_at: Option<String>,
    tools: Option<Vec<crate::models::memory_runtime_types::TurnRuntimeSnapshotToolDto>>,
    runtime: Option<crate::models::memory_runtime_types::TurnRuntimeSnapshotRuntimeDto>,
    system_messages: Vec<TurnRuntimeSnapshotSystemMessageDto>,
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

    let tasks = list_tasks_for_context(session_id, turn_id, true, TASK_BOARD_LIMIT)
        .await
        .unwrap_or_default();
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
    let tasks = list_tasks_for_context(session_id, Some(turn_id), true, TASK_BOARD_LIMIT)
        .await
        .unwrap_or_default();
    classify_task_turn_follow_up(tasks.as_slice(), locale)
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
        unfinished_count,
        blocked_count,
        done_count,
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

pub fn build_runtime_context(
    session_id: Option<String>,
    turn_id: Option<String>,
    locale: InternalContextLocale,
    contact_system_prompt: Option<String>,
    builtin_mcp_system_prompt: Option<String>,
    command_system_prompt: Option<String>,
    task_runner_skill_prompt: Option<String>,
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
        task_runner_skill_prompt,
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
        context.task_runner_skill_prompt.as_deref(),
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
    task_runner_skill_prompt: Option<&str>,
) -> Option<Vec<Value>> {
    let task_board_prompt = build_task_board_prompt(session_id, turn_id, locale).await;
    build_runtime_prefixed_input_items(
        task_board_prompt.as_deref(),
        contact_system_prompt,
        builtin_mcp_system_prompt,
        command_system_prompt,
        task_runner_skill_prompt,
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

async fn sync_task_board_turn_snapshot(
    session_id: &str,
    turn_id: &str,
    task_board_prompt: &str,
) -> Result<(), String> {
    let lookup = chatos_sessions::get_turn_runtime_snapshot_by_turn(session_id, turn_id).await?;
    let payload = build_task_board_snapshot_payload(build_task_board_snapshot_patch(
        lookup,
        task_board_prompt,
    ));
    chatos_sessions::sync_turn_runtime_snapshot(session_id, turn_id, &payload)
        .await
        .map(|_| ())
}

fn build_task_board_snapshot_patch(
    lookup: TurnRuntimeSnapshotLookupResponseDto,
    task_board_prompt: &str,
) -> TaskBoardSnapshotPatch {
    if let Some(snapshot) = lookup.snapshot {
        TaskBoardSnapshotPatch {
            user_message_id: snapshot.user_message_id,
            status: snapshot.status,
            snapshot_source: snapshot.snapshot_source,
            snapshot_version: snapshot.snapshot_version.max(1),
            captured_at: Some(snapshot.captured_at),
            system_messages: upsert_task_board_system_messages(
                snapshot.system_messages.as_slice(),
                task_board_prompt,
            ),
            tools: Some(snapshot.tools),
            runtime: snapshot.runtime,
        }
    } else {
        TaskBoardSnapshotPatch {
            user_message_id: None,
            status: match lookup.status.trim() {
                "completed" => "completed".to_string(),
                "failed" => "failed".to_string(),
                _ => "running".to_string(),
            },
            snapshot_source: "captured".to_string(),
            snapshot_version: 1,
            captured_at: None,
            system_messages: upsert_task_board_system_messages(&[], task_board_prompt),
            tools: None,
            runtime: None,
        }
    }
}

fn build_task_board_snapshot_payload(
    patch: TaskBoardSnapshotPatch,
) -> SyncTurnRuntimeSnapshotRequestDto {
    SyncTurnRuntimeSnapshotRequestDto {
        user_message_id: patch.user_message_id,
        status: Some(patch.status),
        snapshot_source: Some(patch.snapshot_source),
        snapshot_version: Some(patch.snapshot_version),
        captured_at: patch.captured_at,
        system_messages: Some(patch.system_messages),
        tools: patch.tools,
        runtime: patch.runtime,
    }
}

fn upsert_task_board_system_messages(
    messages: &[TurnRuntimeSnapshotSystemMessageDto],
    task_board_prompt: &str,
) -> Vec<TurnRuntimeSnapshotSystemMessageDto> {
    let Some(content) = normalize_optional_text(Some(task_board_prompt)) else {
        return messages
            .iter()
            .filter(|item| item.id.trim() != "task_board")
            .cloned()
            .collect();
    };

    let mut next_messages = messages
        .iter()
        .filter(|item| item.id.trim() != "task_board")
        .cloned()
        .collect::<Vec<_>>();
    let insert_at = next_messages
        .iter()
        .position(|item| {
            let id = item.id.trim();
            id != "base_system" && id != "contact_system"
        })
        .unwrap_or(next_messages.len());
    next_messages.insert(
        insert_at,
        TurnRuntimeSnapshotSystemMessageDto {
            id: "task_board".to_string(),
            source: "task_runtime_board".to_string(),
            content,
        },
    );
    next_messages
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    normalize_optional_text_ref(value)
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
                    unfinished_count,
                    blocked_count,
                    done_count,
                    task_board_prompt
                )
            } else {
                format!(
                    "上一轮助手已经提前总结了，但当前轮还有未完成任务。请继续在同一轮内执行，先把所有非阻塞任务做完，再重新总结。未完成任务：{}。阻塞任务（本次检查忽略）：{}。已完成任务：{}。\n\n最新任务看板：\n{}",
                    unfinished_count,
                    blocked_count,
                    done_count,
                    task_board_prompt
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
        strip_task_turn_review_marker, upsert_task_board_system_messages, TaskTurnFollowUpMode,
        TaskTurnReviewOutcome,
    };
    use crate::core::internal_context_locale::InternalContextLocale;
    use crate::models::memory_runtime_types::TurnRuntimeSnapshotSystemMessageDto;
    use crate::services::task_manager::TaskRecord;
    use serde_json::Value;

    #[test]
    fn upsert_task_board_system_messages_inserts_after_contact_prompts() {
        let messages = vec![
            TurnRuntimeSnapshotSystemMessageDto {
                id: "base_system".to_string(),
                source: "active_system_context".to_string(),
                content: "base".to_string(),
            },
            TurnRuntimeSnapshotSystemMessageDto {
                id: "contact_system".to_string(),
                source: "contact_runtime_context".to_string(),
                content: "contact".to_string(),
            },
            TurnRuntimeSnapshotSystemMessageDto {
                id: "builtin_mcp".to_string(),
                source: "builtin_mcp_policy".to_string(),
                content: "builtin".to_string(),
            },
        ];

        let updated =
            upsert_task_board_system_messages(messages.as_slice(), "[Task Board]\nlatest");

        assert_eq!(updated.len(), 4);
        assert_eq!(updated[2].id, "task_board");
        assert_eq!(updated[2].source, "task_runtime_board");
        assert_eq!(updated[2].content, "[Task Board]\nlatest");
        assert_eq!(updated[3].id, "builtin_mcp");
    }

    #[test]
    fn upsert_task_board_system_messages_replaces_existing_entry() {
        let messages = vec![
            TurnRuntimeSnapshotSystemMessageDto {
                id: "contact_system".to_string(),
                source: "contact_runtime_context".to_string(),
                content: "contact".to_string(),
            },
            TurnRuntimeSnapshotSystemMessageDto {
                id: "task_board".to_string(),
                source: "task_runtime_board".to_string(),
                content: "stale".to_string(),
            },
            TurnRuntimeSnapshotSystemMessageDto {
                id: "memory_summary".to_string(),
                source: "memory_context_summary".to_string(),
                content: "summary".to_string(),
            },
        ];

        let updated = upsert_task_board_system_messages(messages.as_slice(), "fresh");

        assert_eq!(updated.len(), 3);
        assert_eq!(updated[1].id, "task_board");
        assert_eq!(updated[1].content, "fresh");
        assert_eq!(updated[2].id, "memory_summary");
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
            Some("task runner skill".to_string()),
        )
        .expect("context should be present");

        assert_eq!(context.session_id, "session-1");
        assert_eq!(context.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(
            context.task_runner_skill_prompt.as_deref(),
            Some("task runner skill")
        );
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
        assert_eq!(directive.unfinished_count, 1);
        assert_eq!(directive.blocked_count, 1);
        assert_eq!(directive.done_count, 0);
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
