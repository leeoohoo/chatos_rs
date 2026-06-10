use serde_json::{json, Value};

use crate::core::internal_context_locale::InternalContextLocale;
use crate::services::task_manager::TaskRecord;
use crate::services::text_normalization::normalize_optional_text_ref;

const STATUS_TODO: &str = "todo";
const STATUS_DOING: &str = "doing";
const STATUS_BLOCKED: &str = "blocked";
const STATUS_DONE: &str = "done";
const UNFINISHED_TASK_LIMIT: usize = 5;
const BLOCKED_TASK_LIMIT: usize = 3;
const COMPLETED_TASK_LIMIT: usize = 5;

pub fn build_runtime_prefixed_input_items(
    task_board_prompt: Option<&str>,
    contact_system_prompt: Option<&str>,
    builtin_mcp_system_prompt: Option<&str>,
    command_system_prompt: Option<&str>,
    task_runner_skill_prompt: Option<&str>,
) -> Option<Vec<Value>> {
    build_prefixed_input_items(&[
        contact_system_prompt,
        builtin_mcp_system_prompt,
        task_runner_skill_prompt,
        command_system_prompt,
        task_board_prompt,
    ])
}

pub fn format_task_board_prompt(tasks: &[TaskRecord], locale: InternalContextLocale) -> String {
    let mut lines = if locale.is_english() {
        vec![
            "[Task Board]".to_string(),
            "The current task board is maintained by the system. You do not need to call `task_manager_list_tasks` just to figure out what to do next.".to_string(),
            "You mainly need to do two things: create tasks when the work should be broken down, and immediately call `task_manager_complete_task` or `task_manager_update_task` after finishing a task to update its state.".to_string(),
            "You will only see the next current task after task state changes are written back and the context refreshes.".to_string(),
            "If you just finished a task, continue based on the latest unfinished task shown below. Completed tasks stay on the board and remain visible as `done`.".to_string(),
            "".to_string(),
            "Execution rules:".to_string(),
            "- Even if the work is not very complicated, you should still create tasks first when it requires heavy reading, cross-file verification, or phased execution.".to_string(),
            "- While executing the current task, do not infer the next main task on your own. Follow the board's current execution task directly.".to_string(),
            "- After you complete the current task, you must update task state immediately. Only then will the system place the next current task onto the board.".to_string(),
            "- When a task is blocked, you must record what has already been tried, why it is blocked, and what is needed to continue so later turns do not repeat the same investigation.".to_string(),
        ]
    } else {
        vec![
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
        ]
    };

    if tasks.is_empty() {
        lines.push("".to_string());
        lines.push(task_board_current_label(locale).to_string());
        lines.push(task_board_no_current_task(locale).to_string());
        lines.push("".to_string());
        lines.push(task_board_blocked_label(locale).to_string());
        lines.push(task_board_none(locale).to_string());
        lines.push("".to_string());
        lines.push(task_board_completed_label(locale).to_string());
        lines.push(task_board_none(locale).to_string());
        lines.push("".to_string());
        lines.push(if locale.is_english() {
            "If you judge this to be multi-step work, or even a simple request that still requires heavy reading or verification of context, create tasks first and then continue."
                .to_string()
        } else {
            "若你判断这是一项多步骤工作，或虽然需求不复杂但需要大量读取/核对上下文，请先创建任务，再继续执行。".to_string()
        });
        return lines.join("\n");
    }

    let active_task_id = select_active_task_id(tasks);
    let current_tasks = tasks
        .iter()
        .filter(|task| active_task_id.as_deref() == Some(task.id.as_str()))
        .collect::<Vec<_>>();
    let mut unfinished_tasks = tasks
        .iter()
        .filter(|task| {
            let status = normalize_status(task.status.as_str());
            status == STATUS_TODO || status == STATUS_DOING
        })
        .collect::<Vec<_>>();
    unfinished_tasks.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    unfinished_tasks.truncate(UNFINISHED_TASK_LIMIT);
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
    lines.push(task_board_current_label(locale).to_string());
    if current_tasks.is_empty() {
        lines.push(task_board_no_current_task(locale).to_string());
    } else {
        for task in current_tasks {
            append_task_line(&mut lines, task, true, locale);
        }
    }

    lines.push("".to_string());
    lines.push(task_board_unfinished_label(locale).to_string());
    if unfinished_tasks.is_empty() {
        lines.push(task_board_none(locale).to_string());
    } else {
        for task in unfinished_tasks {
            append_task_line(&mut lines, task, false, locale);
        }
    }

    lines.push("".to_string());
    lines.push(task_board_blocked_label(locale).to_string());
    if blocked_tasks.is_empty() {
        lines.push(task_board_none(locale).to_string());
    } else {
        for task in blocked_tasks {
            append_blocked_task_line(&mut lines, task, locale);
        }
    }

    lines.push("".to_string());
    lines.push(task_board_completed_label(locale).to_string());
    if completed_tasks.is_empty() {
        lines.push(task_board_none(locale).to_string());
    } else {
        for task in completed_tasks {
            append_completed_task_line(&mut lines, task, locale);
        }
    }

    lines.join("\n")
}

fn append_task_line(
    lines: &mut Vec<String>,
    task: &TaskRecord,
    is_current: bool,
    locale: InternalContextLocale,
) {
    let marker = if is_current {
        if locale.is_english() {
            " <- current priority"
        } else {
            " <- 当前优先执行"
        }
    } else {
        ""
    };
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
            "  {}: {}",
            if locale.is_english() {
                "details"
            } else {
                "details"
            },
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

fn append_blocked_task_line(
    lines: &mut Vec<String>,
    task: &TaskRecord,
    locale: InternalContextLocale,
) {
    append_task_line(lines, task, false, locale);
    lines.push(format!(
        "  {}: {}",
        if locale.is_english() {
            "outcome"
        } else {
            "outcome"
        },
        display_outcome_summary(task, locale)
    ));
    lines.push(format!(
        "  {}: {}",
        if locale.is_english() {
            "blocker"
        } else {
            "blocker"
        },
        display_blocker_reason(task, locale)
    ));
    lines.push(format!(
        "  {}: {}",
        if locale.is_english() {
            "needs"
        } else {
            "needs"
        },
        display_blocker_needs(task, locale)
    ));
}

fn append_completed_task_line(
    lines: &mut Vec<String>,
    task: &TaskRecord,
    locale: InternalContextLocale,
) {
    append_task_line(lines, task, false, locale);
    lines.push(format!(
        "  {}: {}",
        if locale.is_english() {
            "outcome"
        } else {
            "outcome"
        },
        display_outcome_summary(task, locale)
    ));
    if !task.resume_hint.trim().is_empty() {
        lines.push(format!(
            "  {}: {}",
            if locale.is_english() { "hint" } else { "hint" },
            compact_text(task.resume_hint.as_str(), 140)
        ));
    }
}

fn select_active_task_id(tasks: &[TaskRecord]) -> Option<String> {
    if let Some(task) = tasks
        .iter()
        .rev()
        .find(|task| normalize_status(task.status.as_str()) == STATUS_DOING)
    {
        return Some(task.id.clone());
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

fn display_outcome_summary(task: &TaskRecord, locale: InternalContextLocale) -> String {
    if !task.outcome_summary.trim().is_empty() {
        return compact_text(task.outcome_summary.as_str(), 180);
    }
    if let Some(item) = task.outcome_items.first() {
        if !item.text.trim().is_empty() {
            return compact_text(item.text.as_str(), 180);
        }
    }
    if locale.is_english() {
        "(no recorded outcome)".to_string()
    } else {
        "(未沉淀成果)".to_string()
    }
}

fn display_blocker_reason(task: &TaskRecord, locale: InternalContextLocale) -> String {
    if !task.blocker_reason.trim().is_empty() {
        return compact_text(task.blocker_reason.as_str(), 180);
    }
    if locale.is_english() {
        "(blocker reason not recorded)".to_string()
    } else {
        "(未说明阻塞原因)".to_string()
    }
}

fn display_blocker_needs(task: &TaskRecord, locale: InternalContextLocale) -> String {
    if task.blocker_needs.is_empty() {
        return if locale.is_english() {
            "(unblock conditions not recorded)".to_string()
        } else {
            "(未说明解阻条件)".to_string()
        };
    }
    let separator = if locale.is_english() { "; " } else { "；" };
    compact_text(task.blocker_needs.join(separator).as_str(), 180)
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

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    normalize_optional_text_ref(value)
}

fn task_board_current_label(locale: InternalContextLocale) -> &'static str {
    if locale.is_english() {
        "Current execution task:"
    } else {
        "当前执行任务："
    }
}

fn task_board_blocked_label(locale: InternalContextLocale) -> &'static str {
    if locale.is_english() {
        "Blocked tasks and blocker details:"
    } else {
        "当前阻塞任务与阻塞信息："
    }
}

fn task_board_unfinished_label(locale: InternalContextLocale) -> &'static str {
    if locale.is_english() {
        "Unfinished tasks:"
    } else {
        "未完成任务："
    }
}

fn task_board_completed_label(locale: InternalContextLocale) -> &'static str {
    if locale.is_english() {
        "Completed task history:"
    } else {
        "已完成任务历史："
    }
}

fn task_board_no_current_task(locale: InternalContextLocale) -> &'static str {
    if locale.is_english() {
        "- No current task"
    } else {
        "- 当前无任务"
    }
}

fn task_board_none(locale: InternalContextLocale) -> &'static str {
    if locale.is_english() {
        "- None"
    } else {
        "- 暂无"
    }
}

#[cfg(test)]
mod tests {
    use super::format_task_board_prompt;
    use crate::core::internal_context_locale::InternalContextLocale;
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
        let prompt = format_task_board_prompt(
            &[
                build_task("task_done", "done task", "done"),
                build_task("task_doing", "doing task", "doing"),
                build_task("task_todo", "todo task", "todo"),
            ],
            InternalContextLocale::ZhCn,
        );

        assert!(prompt.contains("当前任务看板由系统维护"));
        assert!(prompt.contains("`task_manager_complete_task`"));
        assert!(prompt.contains("当前执行任务："));
        assert!(prompt.contains("未完成任务："));
        assert!(prompt.contains("当前阻塞任务与阻塞信息："));
        assert!(prompt.contains("已完成任务历史："));
        assert!(prompt.contains("id=task_doing <- 当前优先执行"));
        assert!(prompt.contains("[done] done task"));
    }

    #[test]
    fn leaves_current_task_empty_when_only_todo_tasks_exist() {
        let prompt = format_task_board_prompt(
            &[
                build_task("task_a", "todo task a", "todo"),
                build_task("task_b", "todo task b", "todo"),
                build_task("task_c", "done task c", "done"),
            ],
            InternalContextLocale::ZhCn,
        );

        let current_section = prompt
            .split("未完成任务：")
            .next()
            .unwrap_or_default()
            .to_string();
        assert!(current_section.contains("当前执行任务："));
        assert!(current_section.contains("- 当前无任务"));
        assert!(!current_section.contains("<- 当前优先执行"));
    }

    #[test]
    fn prefers_latest_doing_over_older_doing() {
        let prompt = format_task_board_prompt(
            &[
                build_task("task_doing_a", "doing task a", "doing"),
                build_task("task_todo", "todo task", "todo"),
                build_task("task_doing_b", "doing task b", "doing"),
            ],
            InternalContextLocale::ZhCn,
        );

        assert!(prompt.contains("id=task_doing_b <- 当前优先执行"));
        assert!(!prompt.contains("id=task_doing_a <- 当前优先执行"));
    }

    #[test]
    fn prompts_to_create_tasks_when_board_is_empty() {
        let prompt = format_task_board_prompt(&[], InternalContextLocale::ZhCn);
        assert!(prompt.contains("当前无任务"));
        assert!(prompt.contains("当前阻塞任务与阻塞信息："));
        assert!(prompt.contains("已完成任务历史："));
    }

    #[test]
    fn done_tasks_do_not_appear_as_current_task() {
        let prompt = format_task_board_prompt(
            &[
                build_task("task_done_a", "done task a", "done"),
                build_task("task_done_b", "done task b", "done"),
            ],
            InternalContextLocale::ZhCn,
        );

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

        let prompt = format_task_board_prompt(&[blocked], InternalContextLocale::ZhCn);
        assert!(prompt.contains("当前阻塞任务与阻塞信息："));
        assert!(prompt.contains("blocker: waiting for protocol decision"));
        assert!(prompt.contains("needs: confirm whether total_lines can be added"));
    }

    #[test]
    fn unfinished_tasks_are_listed_separately() {
        let prompt = format_task_board_prompt(
            &[
                build_task("task_todo", "todo task", "todo"),
                build_task("task_doing", "doing task", "doing"),
                build_task("task_done", "done task", "done"),
            ],
            InternalContextLocale::ZhCn,
        );

        assert!(prompt.contains("未完成任务："));
        assert!(prompt.contains("id=task_todo"));
        assert!(prompt.contains("id=task_doing"));
        assert!(prompt.contains("已完成任务历史："));
    }

    #[test]
    fn formats_english_task_board_prompt() {
        let prompt = format_task_board_prompt(
            &[build_task("task_doing", "doing task", "doing")],
            InternalContextLocale::EnUs,
        );

        assert!(prompt.contains("The current task board is maintained by the system"));
        assert!(prompt.contains("Current execution task:"));
        assert!(prompt.contains("Unfinished tasks:"));
        assert!(prompt.contains("Blocked tasks and blocker details:"));
        assert!(prompt.contains("Completed task history:"));
        assert!(prompt.contains("id=task_doing <- current priority"));
    }
}
