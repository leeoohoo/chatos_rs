// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::local_runtime::storage::{
    LocalMemoryContext, LocalMemorySummaryRecord, LocalMessageRecord, LocalSubjectMemoryRecord,
};

use super::sanitize_task_runner_memory_context;

#[test]
fn task_runner_memory_context_redacts_parent_and_previous_run_task_ids() {
    let context = LocalMemoryContext {
        summary: Some(LocalMemorySummaryRecord {
            id: "summary-1".to_string(),
            session_id: "session-1".to_string(),
            summary_text:
                "Failed complete_task task_id=lc_async_task_ba4453d1-c956-4e4f-8196-63eb75facd15 and update_task task_id=lc_task_b4d31671-ca78-4980-901b-e1d8383a0beb"
                    .to_string(),
            summary_model: "test".to_string(),
            trigger_type: "test".to_string(),
            source_start_message_id: None,
            source_end_message_id: None,
            source_message_count: 1,
            source_estimated_tokens: 1,
            level: 0,
            status: "ready".to_string(),
            error_message: None,
            created_at: "2026-07-29T00:00:00Z".to_string(),
            updated_at: "2026-07-29T00:00:00Z".to_string(),
        }),
        recalls: vec![LocalSubjectMemoryRecord {
            id: "recall-1".to_string(),
            subject_type: "session".to_string(),
            subject_id: "session-1".to_string(),
            project_id: "project-1".to_string(),
            recall_key: "key".to_string(),
            recall_text: "reuse lc_async_task_deadbeef or lc_task_old-run incorrectly"
                .to_string(),
            source_session_id: "session-1".to_string(),
            source_summary_id: "summary-1".to_string(),
            level: 0,
            confidence: None,
            last_seen_at: None,
            created_at: "2026-07-29T00:00:00Z".to_string(),
            updated_at: "2026-07-29T00:00:00Z".to_string(),
        }],
        messages: vec![LocalMessageRecord {
            id: "message-1".to_string(),
            session_id: "session-1".to_string(),
            turn_id: Some("turn-1".to_string()),
            sequence_no: 1,
            role: "tool".to_string(),
            content: "{\"task_id\":\"lc_async_task_fd4d4131-2cd4-4293-9762-c4800a3f422\",\"old\":\"lc_task_fd4d4131-2cd4-4293-9762-c4800a3f422\"}"
                .to_string(),
            reasoning: Some("lc_async_task_reasoning lc_task_reasoning".to_string()),
            tool_calls_json: Some(
                "[{\"arguments\":{\"task_id\":\"lc_async_task_tool_call\",\"old\":\"lc_task_tool_call\"}}]".to_string(),
            ),
            tool_call_id: Some("call-1".to_string()),
            metadata_json: Some(
                "{\"task_id\":\"lc_async_task_metadata\",\"old\":\"lc_task_metadata\"}"
                    .to_string(),
            ),
            created_at: "2026-07-29T00:00:00Z".to_string(),
        }],
    };

    let sanitized = sanitize_task_runner_memory_context(context);
    let serialized = serde_json::to_string(&sanitized.summary).expect("serialize summary")
        + sanitized.recalls[0].recall_text.as_str()
        + sanitized.messages[0].content.as_str()
        + sanitized.messages[0]
            .reasoning
            .as_deref()
            .unwrap_or_default()
        + sanitized.messages[0]
            .tool_calls_json
            .as_deref()
            .unwrap_or_default()
        + sanitized.messages[0]
            .metadata_json
            .as_deref()
            .unwrap_or_default();
    assert!(!serialized.contains("lc_async_task_"));
    assert!(!serialized.contains("lc_task_"));
    assert!(serialized.contains("[conversation_parent_task_id_hidden]"));
    assert!(serialized.contains("[previous_run_checklist_task_id_hidden]"));
}
