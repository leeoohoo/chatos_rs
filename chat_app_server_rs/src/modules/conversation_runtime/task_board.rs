use serde_json::Value;

use crate::core::internal_context_locale::InternalContextLocale;
use crate::models::memory_runtime_types::{
    SyncTurnRuntimeSnapshotRequestDto, TurnRuntimeSnapshotLookupResponseDto,
    TurnRuntimeSnapshotSystemMessageDto,
};
use crate::services::chatos_sessions;
use crate::services::task_board_prompt::{
    build_runtime_prefixed_input_items, build_runtime_prefixed_messages, format_task_board_prompt,
};
use crate::services::task_manager::list_tasks_for_context;
use crate::services::text_normalization::normalize_optional_text_ref;
use crate::utils::events::Events;

use super::guidance;
use super::user_context::load_runtime_user_context;

const TASK_BOARD_LIMIT: usize = 200;

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

pub async fn load_prefixed_messages(context: &TaskBoardRuntimeContext) -> Option<Vec<Value>> {
    build_runtime_prefixed_messages_for_turn(
        &context.session_id,
        context.turn_id.as_deref(),
        context.locale,
        context.contact_system_prompt.as_deref(),
        context.builtin_mcp_system_prompt.as_deref(),
        context.command_system_prompt.as_deref(),
    )
    .await
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

pub async fn build_runtime_prefixed_messages_for_turn(
    session_id: &str,
    turn_id: Option<&str>,
    locale: InternalContextLocale,
    contact_system_prompt: Option<&str>,
    builtin_mcp_system_prompt: Option<&str>,
    command_system_prompt: Option<&str>,
) -> Option<Vec<Value>> {
    let task_board_prompt = build_task_board_prompt(session_id, turn_id, locale).await;
    build_runtime_prefixed_messages(
        task_board_prompt.as_deref(),
        contact_system_prompt,
        builtin_mcp_system_prompt,
        command_system_prompt,
    )
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

#[cfg(test)]
mod tests {
    use super::{build_runtime_context, upsert_task_board_system_messages};
    use crate::core::internal_context_locale::InternalContextLocale;
    use crate::models::memory_runtime_types::TurnRuntimeSnapshotSystemMessageDto;

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
        )
        .expect("context should be present");

        assert_eq!(context.session_id, "session-1");
        assert_eq!(context.turn_id.as_deref(), Some("turn-1"));
    }
}
