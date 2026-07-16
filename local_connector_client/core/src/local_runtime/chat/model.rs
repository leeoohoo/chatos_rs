// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use chatos_ai_runtime::{
    build_stateless_history_items, AiRuntime, AiRuntimeOptions, AiRuntimeResult,
    MemoryRecordWriter, ModelRuntimeConfig, RuntimeCallbacks, RuntimeLifecycleHook,
    RuntimeRecordOptions, StatelessHistoryMessage, ToolExecutor,
};
use tokio_util::sync::CancellationToken;

use crate::local_runtime::storage::{
    LocalMemorySummaryRecord, LocalMessageRecord, LocalSubjectMemoryRecord,
};
pub(super) async fn run_text_turn(
    model_config: ModelRuntimeConfig,
    session_id: &str,
    turn_id: &str,
    summary: Option<LocalMemorySummaryRecord>,
    recalls: Vec<LocalSubjectMemoryRecord>,
    messages: Vec<LocalMessageRecord>,
    task_board: String,
    tool_executor: Option<Arc<dyn ToolExecutor>>,
    record_writer: Arc<dyn MemoryRecordWriter>,
    abort_token: CancellationToken,
    lifecycle_hook: Arc<dyn RuntimeLifecycleHook>,
    callbacks: RuntimeCallbacks,
) -> Result<AiRuntimeResult, String> {
    let mut history = Vec::with_capacity(
        messages.len()
            + usize::from(summary.is_some())
            + usize::from(!recalls.is_empty())
            + usize::from(!task_board.trim().is_empty()),
    );
    if !task_board.trim().is_empty() {
        history.push(task_board_history_message(task_board));
    }
    if !recalls.is_empty() {
        history.push(recall_history_message(recalls));
    }
    if let Some(summary) = summary {
        history.push(summary_history_message(summary));
    }
    history.extend(messages.into_iter().map(history_message));
    let input = build_stateless_history_items(&[], &[], None, &history, &[], false, false);
    let request = model_config.to_model_request(serde_json::Value::Array(input), Vec::new());
    let options = AiRuntimeOptions::new(Some(session_id.to_string()), Some(turn_id.to_string()))
        .with_caller_model(Some(model_config.model.clone()))
        .with_caller_model_runtime(Some(model_config.to_tool_caller_model_runtime()))
        .with_abort_token(Some(abort_token))
        .with_lifecycle_hook(Some(lifecycle_hook))
        .with_callbacks(callbacks)
        .with_record_options(RuntimeRecordOptions::persist_all());
    AiRuntime::new(tool_executor)
        .with_record_writer(Some(record_writer))
        .with_max_iterations(12)
        .run_turn(request, options)
        .await
}

fn task_board_history_message(task_board: String) -> StatelessHistoryMessage {
    StatelessHistoryMessage {
        role: "system".to_string(),
        content: task_board,
        reasoning: None,
        tool_calls: None,
        tool_call_id: None,
        metadata: Some(serde_json::json!({
            "runtime_origin": "local_device",
            "context_kind": "task_board",
        })),
        skip_in_input: false,
    }
}

fn recall_history_message(recalls: Vec<LocalSubjectMemoryRecord>) -> StatelessHistoryMessage {
    let content = recalls
        .into_iter()
        .map(|recall| {
            format!(
                "[{}:{}] {}",
                recall.subject_type,
                recall.subject_id,
                truncate_recall(recall.recall_text.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    StatelessHistoryMessage {
        role: "system".to_string(),
        content: format!(
            "Local project and agent memory from other sessions on this device:\n{content}"
        ),
        reasoning: None,
        tool_calls: None,
        tool_call_id: None,
        metadata: Some(serde_json::json!({
            "runtime_origin": "local_device",
            "memory_kind": "subject_recall",
        })),
        skip_in_input: false,
    }
}

fn truncate_recall(value: &str) -> String {
    const LIMIT: usize = 4_000;
    if value.chars().count() <= LIMIT {
        value.to_string()
    } else {
        format!(
            "{}\n[truncated]",
            value.chars().take(LIMIT).collect::<String>()
        )
    }
}

fn summary_history_message(summary: LocalMemorySummaryRecord) -> StatelessHistoryMessage {
    StatelessHistoryMessage {
        role: "system".to_string(),
        content: format!(
            "Local conversation memory (generated on this device):\n{}",
            summary.summary_text
        ),
        reasoning: None,
        tool_calls: None,
        tool_call_id: None,
        metadata: Some(serde_json::json!({
            "runtime_origin": "local_device",
            "memory_summary_id": summary.id,
        })),
        skip_in_input: false,
    }
}

fn history_message(record: LocalMessageRecord) -> StatelessHistoryMessage {
    StatelessHistoryMessage {
        role: record.role,
        content: record.content,
        reasoning: record.reasoning,
        tool_calls: parse_json(record.tool_calls_json),
        tool_call_id: record.tool_call_id,
        metadata: parse_json(record.metadata_json),
        skip_in_input: false,
    }
}

fn parse_json(raw: Option<String>) -> Option<serde_json::Value> {
    raw.and_then(|value| serde_json::from_str(value.as_str()).ok())
}

#[cfg(test)]
mod tests {
    use crate::local_runtime::storage::LocalSubjectMemoryRecord;

    use super::{recall_history_message, task_board_history_message};

    #[test]
    fn builds_local_subject_recall_as_system_context() {
        let message = recall_history_message(vec![LocalSubjectMemoryRecord {
            id: "recall-1".to_string(),
            subject_type: "project".to_string(),
            subject_id: "project-1".to_string(),
            project_id: "project-1".to_string(),
            recall_key: "session:one".to_string(),
            recall_text: "Use SQLite locally.".to_string(),
            source_session_id: "session-one".to_string(),
            source_summary_id: "summary-one".to_string(),
            level: 0,
            confidence: None,
            last_seen_at: None,
            created_at: "2026-07-15T00:00:00Z".to_string(),
            updated_at: "2026-07-15T00:00:00Z".to_string(),
        }]);

        assert_eq!(message.role, "system");
        assert!(message.content.contains("project:project-1"));
        assert!(message.content.contains("Use SQLite locally."));
    }

    #[test]
    fn builds_local_task_board_as_system_context() {
        let message = task_board_history_message("[Local Task Board]\n- task".to_string());
        assert_eq!(message.role, "system");
        assert!(message.content.contains("Local Task Board"));
        assert_eq!(
            message
                .metadata
                .as_ref()
                .and_then(|value| value.get("context_kind"))
                .and_then(serde_json::Value::as_str),
            Some("task_board")
        );
    }
}
