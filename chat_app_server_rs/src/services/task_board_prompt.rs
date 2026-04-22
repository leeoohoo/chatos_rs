use serde_json::{json, Value};

use crate::services::memory_server_client::{
    self, SyncTurnRuntimeSnapshotRequestDto, TurnRuntimeSnapshotSystemMessageDto,
};
use crate::services::runtime_guidance_manager::runtime_guidance_manager;
use crate::services::task_manager::{list_tasks_for_context, TaskRecord};
use crate::utils::events::Events;

const TASK_BOARD_LIMIT: usize = 12;
const STATUS_TODO: &str = "todo";
const STATUS_DOING: &str = "doing";
const STATUS_BLOCKED: &str = "blocked";
const STATUS_DONE: &str = "done";
const BLOCKED_TASK_LIMIT: usize = 3;
const COMPLETED_TASK_LIMIT: usize = 5;

pub async fn build_task_board_prompt(
    conversation_id: &str,
    conversation_turn_id: Option<&str>,
) -> Option<String> {
    let conversation_id = conversation_id.trim();
    if conversation_id.is_empty() {
        return None;
    }

    let tasks = list_tasks_for_context(
        conversation_id,
        conversation_turn_id,
        true,
        TASK_BOARD_LIMIT,
    )
        .await
        .unwrap_or_default();
    Some(format_task_board_prompt(tasks.as_slice()))
        .filter(|content| !content.trim().is_empty())
}

pub async fn build_runtime_prefixed_messages(
    session_id: &str,
    turn_id: Option<&str>,
    contact_system_prompt: Option<&str>,
    builtin_mcp_system_prompt: Option<&str>,
    command_system_prompt: Option<&str>,
) -> Option<Vec<Value>> {
    let task_board_prompt = build_task_board_prompt(session_id, turn_id).await;
    build_prefixed_messages(&[
        contact_system_prompt,
        task_board_prompt.as_deref(),
        builtin_mcp_system_prompt,
        command_system_prompt,
    ])
}

pub async fn build_runtime_prefixed_input_items(
    session_id: &str,
    turn_id: Option<&str>,
    contact_system_prompt: Option<&str>,
    builtin_mcp_system_prompt: Option<&str>,
    command_system_prompt: Option<&str>,
) -> Option<Vec<Value>> {
    let task_board_prompt = build_task_board_prompt(session_id, turn_id).await;
    build_prefixed_input_items(&[
        contact_system_prompt,
        task_board_prompt.as_deref(),
        builtin_mcp_system_prompt,
        command_system_prompt,
    ])
}

fn format_task_board_prompt(tasks: &[TaskRecord]) -> String {
    let mut lines = vec![
        "[Task Board]".to_string(),
        "当前任务看板由系统维护，你不需要主动调用 `task_manager_list_tasks` 来判断现在该做什么。".to_string(),
        "你主要负责两件事：需要拆解时创建任务；完成某项任务后立刻调用 `task_manager_complete_task` 或 `task_manager_update_task` 更新状态。".to_string(),
        "只有在任务状态被更新后，你才会在下一次上下文刷新里看到新的当前任务。".to_string(),
        "如果刚完成一项任务，继续依据下面看板里的最新未完成任务推进；已完成任务会保留在看板中，并显示为 `done`。".to_string(),
        "".to_string(),
        "执行要求：".to_string(),
        "- 如果工作虽然不复杂，但需要读取较多内容、跨文件核对、分阶段推进，也应该先拆成任务再执行。".to_string(),
        "- 执行当前任务时，不要再自己推断下一项主任务是什么，直接以本看板中的“当前执行任务”为准。".to_string(),
        "- 当你完成当前任务后，必须立即更新任务状态；只有状态变更后，系统才会把新的当前任务放进看板。".to_string(),
        "- 当任务被阻塞时，必须写明已做事项、阻塞原因和继续推进所需条件，避免后续重复排障。".to_string(),
    ];

    if tasks.is_empty() {
        lines.push("".to_string());
        lines.push("当前执行任务：".to_string());
        lines.push("- 当前无任务".to_string());
        lines.push("".to_string());
        lines.push("当前阻塞任务与阻塞信息：".to_string());
        lines.push("- 暂无".to_string());
        lines.push("".to_string());
        lines.push("已完成任务历史：".to_string());
        lines.push("- 暂无".to_string());
        lines.push("".to_string());
        lines.push("若你判断这是一项多步骤工作，或虽然需求不复杂但需要大量读取/核对上下文，请先创建任务，再继续执行。".to_string());
        return lines.join("\n");
    }

    let active_task_id = select_active_task_id(tasks);
    let current_tasks = tasks
        .iter()
        .filter(|task| active_task_id.as_deref() == Some(task.id.as_str()))
        .collect::<Vec<_>>();
    let mut blocked_tasks = tasks
        .iter()
        .filter(|task| normalize_status(task.status.as_str()) == STATUS_BLOCKED)
        .collect::<Vec<_>>();
    blocked_tasks.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    blocked_tasks.truncate(BLOCKED_TASK_LIMIT);
    let mut completed_tasks = tasks
        .iter()
        .filter(|task| normalize_status(task.status.as_str()) == STATUS_DONE)
        .collect::<Vec<_>>();
    completed_tasks.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    completed_tasks.truncate(COMPLETED_TASK_LIMIT);

    lines.push("".to_string());
    lines.push("当前执行任务：".to_string());
    if current_tasks.is_empty() {
        lines.push("- 当前无任务".to_string());
    } else {
        for task in current_tasks {
            append_task_line(&mut lines, task, true);
        }
    }

    lines.push("".to_string());
    lines.push("当前阻塞任务与阻塞信息：".to_string());
    if blocked_tasks.is_empty() {
        lines.push("- 暂无".to_string());
    } else {
        for task in blocked_tasks {
            append_blocked_task_line(&mut lines, task);
        }
    }

    lines.push("".to_string());
    lines.push("已完成任务历史：".to_string());
    if completed_tasks.is_empty() {
        lines.push("- 暂无".to_string());
    } else {
        for task in completed_tasks {
            append_completed_task_line(&mut lines, task);
        }
    }

    lines.join("\n")
}

fn append_task_line(lines: &mut Vec<String>, task: &TaskRecord, is_current: bool) {
    let marker = if is_current { " <- 当前优先执行" } else { "" };
    lines.push(format!(
        "- [{}] {} ({}) id={}{}",
        normalize_status(task.status.as_str()),
        compact_text(task.title.as_str(), 80),
        normalize_priority(task.priority.as_str()),
        task.id,
        marker
    ));
    if !task.details.trim().is_empty() {
        lines.push(format!(
            "  details: {}",
            compact_text(task.details.as_str(), 140)
        ));
    }
    if let Some(due_at) = task
        .due_at
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("  due_at: {}", due_at));
    }
}

fn append_blocked_task_line(lines: &mut Vec<String>, task: &TaskRecord) {
    append_task_line(lines, task, false);
    lines.push(format!("  outcome: {}", display_outcome_summary(task)));
    lines.push(format!("  blocker: {}", display_blocker_reason(task)));
    lines.push(format!("  needs: {}", display_blocker_needs(task)));
}

fn append_completed_task_line(lines: &mut Vec<String>, task: &TaskRecord) {
    append_task_line(lines, task, false);
    lines.push(format!("  outcome: {}", display_outcome_summary(task)));
    if !task.resume_hint.trim().is_empty() {
        lines.push(format!(
            "  hint: {}",
            compact_text(task.resume_hint.as_str(), 140)
        ));
    }
}

fn select_active_task_id(tasks: &[TaskRecord]) -> Option<String> {
    for preferred_status in [STATUS_DOING, STATUS_TODO] {
        if let Some(task) = tasks.iter().find(|task| task.status.trim() == preferred_status) {
            return Some(task.id.clone());
        }
    }
    None
}

fn normalize_status(status: &str) -> &'static str {
    match status.trim() {
        STATUS_DOING => STATUS_DOING,
        STATUS_BLOCKED => STATUS_BLOCKED,
        STATUS_DONE => STATUS_DONE,
        _ => STATUS_TODO,
    }
}

fn normalize_priority(priority: &str) -> &'static str {
    match priority.trim() {
        "high" => "high",
        "low" => "low",
        _ => "medium",
    }
}

fn compact_text(input: &str, max_chars: usize) -> String {
    let normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }

    let mut out = String::new();
    for (index, ch) in normalized.chars().enumerate() {
        if index >= max_chars {
            break;
        }
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn display_outcome_summary(task: &TaskRecord) -> String {
    if !task.outcome_summary.trim().is_empty() {
        return compact_text(task.outcome_summary.as_str(), 180);
    }
    if let Some(item) = task.outcome_items.first() {
        if !item.text.trim().is_empty() {
            return compact_text(item.text.as_str(), 180);
        }
    }
    "(未沉淀成果)".to_string()
}

fn display_blocker_reason(task: &TaskRecord) -> String {
    if !task.blocker_reason.trim().is_empty() {
        return compact_text(task.blocker_reason.as_str(), 180);
    }
    "(未说明阻塞原因)".to_string()
}

fn display_blocker_needs(task: &TaskRecord) -> String {
    if task.blocker_needs.is_empty() {
        return "(未说明解阻条件)".to_string();
    }
    compact_text(task.blocker_needs.join("；").as_str(), 180)
}

fn build_prefixed_messages(system_prompts: &[Option<&str>]) -> Option<Vec<Value>> {
    let mut prefixed_messages_items = Vec::new();
    for prompt in system_prompts
        .iter()
        .filter_map(|item| normalize_optional_text(*item))
    {
        prefixed_messages_items.push(json!({
            "role": "system",
            "content": prompt,
        }));
    }
    if prefixed_messages_items.is_empty() {
        None
    } else {
        Some(prefixed_messages_items)
    }
}

fn build_prefixed_input_items(system_prompts: &[Option<&str>]) -> Option<Vec<Value>> {
    let mut prefixed_input_items = Vec::new();
    for prompt in system_prompts
        .iter()
        .filter_map(|item| normalize_optional_text(*item))
    {
        prefixed_input_items.push(json!({
            "type": "message",
            "role": "system",
            "content": [{ "type": "input_text", "text": prompt }],
        }));
    }
    if prefixed_input_items.is_empty() {
        None
    } else {
        Some(prefixed_input_items)
    }
}

pub fn build_task_board_runtime_guidance(task_board_prompt: &str) -> Option<String> {
    let task_board_prompt = task_board_prompt.trim();
    if task_board_prompt.is_empty() {
        return None;
    }

    Some(format!(
        "[Task Board Updated]\n- source: system task board refresh after task mutation\n- rule: replace any stale task assumptions with the latest board below\n- instruction: continue strictly based on this refreshed board\n\n{}",
        task_board_prompt
    ))
}

pub async fn enqueue_task_board_refresh(
    session_id: &str,
    turn_id: &str,
) -> Option<String> {
    let session_id = session_id.trim();
    let turn_id = turn_id.trim();
    if session_id.is_empty() || turn_id.is_empty() {
        return None;
    }

    let prompt = build_task_board_prompt(session_id, Some(turn_id)).await?;
    if let Some(guidance) = build_task_board_runtime_guidance(prompt.as_str()) {
        let _ = runtime_guidance_manager().enqueue_guidance(session_id, turn_id, guidance.as_str());
    }
    let _ = sync_task_board_turn_snapshot(session_id, turn_id, prompt.as_str()).await;
    Some(prompt)
}

async fn sync_task_board_turn_snapshot(
    session_id: &str,
    turn_id: &str,
    task_board_prompt: &str,
) -> Result<(), String> {
    let lookup = memory_server_client::get_turn_runtime_snapshot_by_turn(session_id, turn_id).await?;
    let payload = if let Some(snapshot) = lookup.snapshot {
        SyncTurnRuntimeSnapshotRequestDto {
            user_message_id: snapshot.user_message_id,
            status: Some(snapshot.status),
            snapshot_source: Some(snapshot.snapshot_source),
            snapshot_version: Some(snapshot.snapshot_version.max(1)),
            captured_at: Some(snapshot.captured_at),
            system_messages: Some(upsert_task_board_system_messages(
                snapshot.system_messages.as_slice(),
                task_board_prompt,
            )),
            tools: Some(snapshot.tools),
            runtime: snapshot.runtime,
        }
    } else {
        SyncTurnRuntimeSnapshotRequestDto {
            user_message_id: None,
            status: Some(match lookup.status.trim() {
                "completed" => "completed".to_string(),
                "failed" => "failed".to_string(),
                _ => "running".to_string(),
            }),
            snapshot_source: Some("captured".to_string()),
            snapshot_version: Some(1),
            captured_at: None,
            system_messages: Some(upsert_task_board_system_messages(&[], task_board_prompt)),
            tools: None,
            runtime: None,
        }
    };
    memory_server_client::sync_turn_runtime_snapshot(session_id, turn_id, &payload)
        .await
        .map(|_| ())
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

pub fn build_task_board_updated_event_payload(
    conversation_id: &str,
    conversation_turn_id: &str,
    task_board_prompt: &str,
) -> Value {
    json!({
        "event": Events::TASK_BOARD_UPDATED,
        "data": {
            "conversation_id": conversation_id,
            "conversation_turn_id": conversation_turn_id,
            "task_board": task_board_prompt,
        }
    })
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::format_task_board_prompt;
    use crate::services::task_manager::TaskRecord;

    fn build_task(id: &str, title: &str, status: &str) -> TaskRecord {
        TaskRecord {
            id: id.to_string(),
            conversation_id: "session_1".to_string(),
            conversation_turn_id: "turn_1".to_string(),
            title: title.to_string(),
            details: "details".to_string(),
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
            created_at: "2026-04-21T00:00:00Z".to_string(),
            updated_at: "2026-04-21T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn marks_first_doing_task_as_current() {
        let prompt = format_task_board_prompt(&[
            build_task("task_done", "done task", "done"),
            build_task("task_doing", "doing task", "doing"),
            build_task("task_todo", "todo task", "todo"),
        ]);

        assert!(prompt.contains("当前任务看板由系统维护"));
        assert!(prompt.contains("`task_manager_complete_task`"));
        assert!(prompt.contains("当前执行任务："));
        assert!(prompt.contains("当前阻塞任务与阻塞信息："));
        assert!(prompt.contains("已完成任务历史："));
        assert!(prompt.contains("id=task_doing <- 当前优先执行"));
        assert!(prompt.contains("[done] done task"));
    }

    #[test]
    fn prefers_earliest_todo_over_newer_todo() {
        let prompt = format_task_board_prompt(&[
            build_task("task_a", "todo task a", "todo"),
            build_task("task_b", "todo task b", "todo"),
            build_task("task_c", "done task c", "done"),
        ]);

        assert!(prompt.contains("id=task_a <- 当前优先执行"));
        assert!(!prompt.contains("id=task_b <- 当前优先执行"));
    }

    #[test]
    fn prompts_to_create_tasks_when_board_is_empty() {
        let prompt = format_task_board_prompt(&[]);
        assert!(prompt.contains("当前无任务"));
        assert!(prompt.contains("当前阻塞任务与阻塞信息："));
        assert!(prompt.contains("已完成任务历史："));
    }

    #[test]
    fn done_tasks_do_not_appear_as_current_task() {
        let prompt = format_task_board_prompt(&[
            build_task("task_done_a", "done task a", "done"),
            build_task("task_done_b", "done task b", "done"),
        ]);

        let current_section = prompt
            .split("已完成任务历史：")
            .next()
            .unwrap_or_default()
            .to_string();
        assert!(current_section.contains("当前执行任务："));
        assert!(current_section.contains("- 当前无任务"));
        assert!(!current_section.contains("<- 当前优先执行"));
        assert!(prompt.contains("[done] done task a"));
        assert!(prompt.contains("[done] done task b"));
    }

    #[test]
    fn blocked_tasks_show_reason_and_needs() {
        let mut blocked = build_task("task_blocked", "blocked task", "blocked");
        blocked.outcome_summary = "checked remote read_file and found missing field".to_string();
        blocked.blocker_reason = "waiting for protocol decision".to_string();
        blocked.blocker_needs = vec!["confirm whether total_lines can be added".to_string()];

        let prompt = format_task_board_prompt(&[blocked]);
        assert!(prompt.contains("当前阻塞任务与阻塞信息："));
        assert!(prompt.contains("blocker: waiting for protocol decision"));
        assert!(prompt.contains("needs: confirm whether total_lines can be added"));
    }

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

        let updated = upsert_task_board_system_messages(messages.as_slice(), "[Task Board]\nlatest");

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
}
