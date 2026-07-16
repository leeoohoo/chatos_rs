// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_builtin_tools::{
    AskUserPromptPayload, AskUserStore, TaskDraft, TaskStreamChunkCallback,
};
use serde_json::{json, Value};

use crate::local_runtime::ask_user::LocalAskUserStore;

use super::LocalTaskManagerStore;

pub(super) async fn review_and_create_tasks(
    store: &LocalTaskManagerStore,
    conversation_id: &str,
    conversation_turn_id: &str,
    draft_tasks: Vec<TaskDraft>,
    timeout_ms: u64,
    on_stream_chunk: Option<TaskStreamChunkCallback>,
) -> Result<Value, String> {
    let ask_store = LocalAskUserStore::new(
        store.database.clone(),
        store.owner_user_id.clone(),
        store.ask_user_prompts.clone(),
    );
    let decision = ask_store
        .execute_prompt(
            AskUserPromptPayload {
                prompt_id: format!("up_task_review_{}", uuid::Uuid::new_v4()),
                conversation_id: conversation_id.to_string(),
                conversation_turn_id: conversation_turn_id.to_string(),
                tool_call_id: None,
                kind: "choice".to_string(),
                title: "确认创建本地任务".to_string(),
                message: format!(
                    "Task Manager 准备创建 {} 个本地任务，是否继续？",
                    draft_tasks.len()
                ),
                allow_cancel: true,
                timeout_ms,
                payload: json!({
                    "choice": {
                        "multiple": false,
                        "min_selections": 1,
                        "max_selections": 1,
                        "options": [
                            { "value": "confirm", "label": "创建任务" },
                            { "value": "cancel", "label": "取消" }
                        ]
                    },
                    "tasks": &draft_tasks,
                }),
            },
            on_stream_chunk
                .clone()
                .map(|callback| callback as chatos_builtin_tools::AskUserStreamChunkCallback),
        )
        .await?;
    if decision.status != "ok"
        || !decision
            .response
            .selection
            .as_ref()
            .is_some_and(selection_confirms)
    {
        return Ok(json!({
            "confirmed": false,
            "cancelled": true,
            "reason": decision.response.reason.unwrap_or_else(|| decision.status),
        }));
    }
    let tasks = store
        .database
        .create_local_task_board_tasks(
            store.owner_user_id.as_str(),
            conversation_id,
            conversation_turn_id,
            draft_tasks,
        )
        .await
        .map_err(|error| error.to_string())?;
    emit_task_board_updated(
        store,
        conversation_id,
        conversation_turn_id,
        on_stream_chunk,
    )
    .await;
    Ok(json!({
        "confirmed": true,
        "cancelled": false,
        "created_count": tasks.len(),
        "tasks": tasks,
        "conversation_id": conversation_id,
        "conversation_turn_id": conversation_turn_id,
    }))
}

async fn emit_task_board_updated(
    store: &LocalTaskManagerStore,
    conversation_id: &str,
    conversation_turn_id: &str,
    callback: Option<TaskStreamChunkCallback>,
) {
    let (Some(callback), Ok(task_board)) = (
        callback,
        store
            .database
            .local_task_board_prompt(store.owner_user_id.as_str(), conversation_id)
            .await,
    ) else {
        return;
    };
    let event = json!({
        "event": "conversation.task_board.updated",
        "data": {
            "conversation_id": conversation_id,
            "conversation_turn_id": conversation_turn_id,
            "task_board": task_board,
            "runtime_origin": "local_device"
        }
    });
    if let Ok(serialized) = serde_json::to_string(&event) {
        callback(serialized);
    }
}

fn selection_confirms(value: &Value) -> bool {
    value.as_str() == Some("confirm")
        || value
            .as_array()
            .is_some_and(|values| values.iter().any(|value| value.as_str() == Some("confirm")))
}
