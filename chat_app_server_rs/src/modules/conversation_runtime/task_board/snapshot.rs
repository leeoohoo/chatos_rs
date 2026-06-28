use crate::models::memory_runtime_types::{
    SyncTurnRuntimeSnapshotRequestDto, TurnRuntimeSnapshotLookupResponseDto,
    TurnRuntimeSnapshotSystemMessageDto,
};
use crate::services::chatos_sessions;
use crate::services::text_normalization::normalize_optional_text_ref;

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

pub(super) async fn sync_task_board_turn_snapshot(
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
    let Some(content) = normalize_optional_text_ref(Some(task_board_prompt)) else {
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

#[cfg(test)]
mod tests {
    use super::upsert_task_board_system_messages;
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
}
