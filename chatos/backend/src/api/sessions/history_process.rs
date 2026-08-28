// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::models::message::Message;

mod compact;
mod turn_display;
mod turn_process_stats;
mod turn_slices;

pub(super) fn build_compact_history_messages_from_turn_slices(
    slices: Vec<memory_engine_sdk::TurnRecordSlice>,
) -> Vec<Message> {
    turn_slices::build_compact_history_messages_from_turn_slices(slices)
}

pub(super) fn build_compact_history_messages_from_turn_slices_with_process(
    slices: Vec<memory_engine_sdk::TurnRecordSlice>,
    process_messages_by_turn: &std::collections::HashMap<String, Vec<Message>>,
) -> Vec<Message> {
    turn_slices::build_compact_history_messages_from_turn_slices_with_process(
        slices,
        process_messages_by_turn,
    )
}

pub(super) fn turn_slice_needs_task_runner_callback_process_messages(
    slice: &memory_engine_sdk::TurnRecordSlice,
) -> bool {
    turn_slices::turn_slice_needs_task_runner_callback_process_messages(slice)
}

pub(super) fn find_user_index_by_turn_id(messages: &[Message], turn_id: &str) -> Option<usize> {
    turn_display::find_user_index_by_turn_id(messages, turn_id)
}

pub(super) fn build_turn_display_messages(messages: &[Message], user_index: usize) -> Vec<Message> {
    turn_display::build_turn_display_messages(messages, user_index)
}

pub(super) fn build_turn_display_messages_with_process_records(
    messages: &[Message],
    user_index: usize,
    process_records: &[Message],
) -> Vec<Message> {
    turn_display::build_turn_display_messages_with_process_records(
        messages,
        user_index,
        process_records,
    )
}

pub(super) fn build_compact_history_messages(messages: Vec<Message>) -> Vec<Message> {
    compact::build_compact_history_messages(messages)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::{json, Value};

    use super::{
        build_compact_history_messages, build_compact_history_messages_from_turn_slices,
        build_compact_history_messages_from_turn_slices_with_process, build_turn_display_messages,
        build_turn_display_messages_with_process_records,
        turn_slice_needs_task_runner_callback_process_messages,
    };
    use crate::models::message::Message;

    fn build_message(role: &str, content: &str) -> Message {
        Message::new(
            "session-1".to_string(),
            role.to_string(),
            content.to_string(),
        )
    }

    fn build_engine_record(
        id: &str,
        role: &str,
        content: &str,
        turn_id: &str,
    ) -> memory_engine_sdk::EngineRecord {
        memory_engine_sdk::EngineRecord {
            id: id.to_string(),
            thread_id: "session-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            source_id: "chatos".to_string(),
            external_record_id: None,
            role: role.to_string(),
            record_type: "message".to_string(),
            content: content.to_string(),
            structured_payload: None,
            metadata: Some(json!({
                "conversation_turn_id": turn_id
            })),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: "2026-06-12T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn compact_history_keeps_task_runner_callbacks_visible_after_plan_summary() {
        let mut user = build_message("user", "help");
        user.id = "user-1".to_string();
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-1"
        }));

        let mut plan = build_message("assistant", "I created the tasks.");
        plan.id = "assistant-plan".to_string();
        plan.metadata = Some(json!({
            "conversation_turn_id": "turn-1"
        }));

        let mut callback = build_message("assistant", "Task A completed.");
        callback.id = "assistant-callback".to_string();
        callback.message_mode = Some("task_runner_callback".to_string());
        callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update"
            }
        }));

        let compact = build_compact_history_messages(vec![user, plan, callback]);
        assert_eq!(compact.len(), 3);
        assert_eq!(compact[0].role, "user");
        assert_eq!(compact[1].id, "assistant-plan");
        assert_eq!(compact[2].id, "assistant-callback");
        assert_eq!(
            compact[2]
                .metadata
                .as_ref()
                .and_then(|value| value.get("conversation_turn_id")),
            None
        );
    }

    #[test]
    fn compact_history_hides_cancelled_task_runner_callbacks() {
        let mut user = build_message("user", "run tasks");
        user.id = "user-1".to_string();
        user.metadata = Some(json!({"conversation_turn_id": "turn-1"}));

        let mut plan = build_message("assistant", "The task plan is ready.");
        plan.id = "assistant-plan".to_string();
        plan.metadata = Some(json!({"conversation_turn_id": "turn-1"}));

        let mut cancelled = build_message("assistant", "Task README was cancelled.");
        cancelled.id = "assistant-cancelled".to_string();
        cancelled.message_mode = Some("task_runner_callback".to_string());
        cancelled.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "event": "task.cancelled",
                "status": "cancelled"
            }
        }));

        let compact = build_compact_history_messages(vec![user, plan, cancelled]);
        assert_eq!(
            compact
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["user-1", "assistant-plan"]
        );
    }

    #[test]
    fn turn_display_recovers_process_records_missing_from_visible_history() {
        let mut user = build_message("user", "help");
        user.id = "user-1".to_string();
        user.metadata = Some(json!({ "conversation_turn_id": "turn-1" }));

        let mut final_assistant = build_message("assistant", "done");
        final_assistant.id = "assistant-final".to_string();
        final_assistant.metadata = Some(json!({ "conversation_turn_id": "turn-1" }));

        let mut process_assistant = build_message("assistant", "");
        process_assistant.id = "assistant-process".to_string();
        process_assistant.reasoning = Some("inspect repository".to_string());
        process_assistant.metadata = Some(json!({ "conversation_turn_id": "turn-1" }));

        let mut process_tool = build_message("tool", "tool result");
        process_tool.id = "tool-process".to_string();
        process_tool.tool_call_id = Some("call-1".to_string());
        process_tool.metadata = Some(json!({ "conversation_turn_id": "turn-1" }));

        let display = build_turn_display_messages_with_process_records(
            &[user, final_assistant],
            0,
            &[process_assistant, process_tool],
        );

        assert_eq!(display.len(), 4);
        assert_eq!(display[0].id, "user-1");
        assert_eq!(display[1].id, "assistant-process");
        assert_eq!(display[2].id, "tool-process");
        assert_eq!(display[3].id, "assistant-final");
        for message in &display[1..3] {
            assert_eq!(
                message
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("historyProcessLoaded"))
                    .and_then(Value::as_bool),
                Some(true)
            );
        }
        let history_process = display[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("historyProcess"))
            .expect("history process metadata");
        assert_eq!(
            history_process
                .get("processMessageCount")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            history_process.get("thinkingCount").and_then(Value::as_u64),
            Some(1)
        );
    }

    #[test]
    fn compact_history_marks_task_runner_user_completed_after_plan_summary() {
        let mut user = build_message("user", "help");
        user.id = "user-1".to_string();
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "overall_status": "processing"
            }
        }));

        let mut plan = build_message("assistant", "I created the tasks.");
        plan.id = "assistant-plan".to_string();
        plan.message_mode = Some("task_runner_async_plan".to_string());
        plan.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "message_kind": "plan_summary"
            }
        }));

        let compact = build_compact_history_messages(vec![user, plan]);
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("overall_status"))
                .and_then(|value| value.as_str()),
            Some("completed")
        );
    }

    #[test]
    fn compact_history_from_turn_slices_adds_process_metadata_and_final_link() {
        let user = build_engine_record("user-1", "user", "help", "turn-1");
        let assistant = build_engine_record("assistant-1", "assistant", "done", "turn-1");

        let compact = build_compact_history_messages_from_turn_slices(vec![
            memory_engine_sdk::TurnRecordSlice {
                turn_id: "turn-1".to_string(),
                user_record: user,
                final_assistant_record: Some(assistant),
                has_process: true,
                tool_call_count: 2,
                thinking_count: 1,
                process_message_count: 3,
            },
        ]);

        assert_eq!(compact.len(), 2);
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcess"))
                .and_then(|value| value.get("toolCallCount"))
                .and_then(|value| value.as_u64()),
            Some(2)
        );
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcess"))
                .and_then(|value| value.get("finalAssistantMessageId"))
                .and_then(|value| value.as_str()),
            Some("assistant-1")
        );
        assert_eq!(
            compact[1]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyFinalForUserMessageId"))
                .and_then(|value| value.as_str()),
            Some("user-1")
        );
    }

    #[test]
    fn compact_history_keeps_intermediate_tool_call_turn_processing() {
        let mut user = build_engine_record("user-1", "user", "plan it", "turn-1");
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "overall_status": "processing"
            }
        }));
        let mut assistant = build_engine_record("assistant-1", "assistant", "", "turn-1");
        assistant.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "response_status": "tool_calls",
            "toolCalls": [{
                "id": "call-1",
                "type": "function",
                "function": {
                    "name": "project_management_service_list_project_tasks",
                    "arguments": "{}"
                }
            }],
            "task_runner_async": {
                "mode": "contact_async",
                "message_kind": "plan_summary"
            }
        }));

        let compact = build_compact_history_messages_from_turn_slices(vec![
            memory_engine_sdk::TurnRecordSlice {
                turn_id: "turn-1".to_string(),
                user_record: user,
                final_assistant_record: Some(assistant),
                has_process: true,
                tool_call_count: 1,
                thinking_count: 0,
                process_message_count: 1,
            },
        ]);

        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("overall_status"))
                .and_then(Value::as_str),
            Some("processing")
        );
    }

    #[test]
    fn compact_history_from_turn_slices_keeps_task_runner_callback_visible() {
        let user = build_engine_record("user-1", "user", "help", "turn-1");
        let mut callback = build_engine_record(
            "task_runner_callback::user-1::task-1::task.completed::run-1",
            "assistant",
            "Task completed.",
            "turn-1",
        );
        callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));

        let compact = build_compact_history_messages_from_turn_slices(vec![
            memory_engine_sdk::TurnRecordSlice {
                turn_id: "turn-1".to_string(),
                user_record: user,
                final_assistant_record: Some(callback),
                has_process: true,
                tool_call_count: 0,
                thinking_count: 0,
                process_message_count: 1,
            },
        ]);

        assert_eq!(compact.len(), 2);
        assert_eq!(compact[0].id, "user-1");
        assert_eq!(
            compact[1].id,
            "task_runner_callback::user-1::task-1::task.completed::run-1"
        );
        assert_eq!(
            compact[1]
                .metadata
                .as_ref()
                .and_then(|value| value.get("conversation_turn_id")),
            None
        );
        assert_eq!(
            compact[1]
                .metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("source_turn_id"))
                .and_then(|value| value.as_str()),
            Some("turn-1")
        );
    }

    #[test]
    fn compact_history_sanitizes_legacy_task_runner_callback_content() {
        let user = build_engine_record("user-1", "user", "help", "turn-1");
        let mut callback = build_engine_record(
            "task_runner_callback::user-1::task-1::task.completed::run-1",
            "assistant",
            "任务「创建需求」已完成\n\n结果摘要：\n- requirement_id: `6f7854a9-7a6e-4aef-887b-9de81198f349`\n- 状态：`draft`\n- 文档类型：`implementation_plan`",
            "turn-1",
        );
        callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1",
                "task_title": "创建需求",
                "result_summary": "requirement_id: 6f7854a9-7a6e-4aef-887b-9de81198f349\n状态：draft\n文档类型：implementation_plan"
            }
        }));

        let compact = build_compact_history_messages_from_turn_slices(vec![
            memory_engine_sdk::TurnRecordSlice {
                turn_id: "turn-1".to_string(),
                user_record: user,
                final_assistant_record: Some(callback),
                has_process: true,
                tool_call_count: 0,
                thinking_count: 0,
                process_message_count: 1,
            },
        ]);

        assert!(compact[1].content.contains("任务「创建需求」已完成"));
        assert!(compact[1].content.contains("草稿"));
        assert!(compact[1].content.contains("实施计划"));
        assert!(!compact[1].content.contains("requirement_id"));
        assert!(!compact[1].content.contains("6f7854a9"));
        assert!(!compact[1].content.contains("implementation_plan"));
        let result_summary = compact[1]
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("result_summary"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert!(!result_summary.contains("requirement_id"));
        assert!(!result_summary.contains("6f7854a9"));
    }

    #[test]
    fn compact_history_preserves_completed_callback_user_summary() {
        let user = build_engine_record("user-1", "user", "help", "turn-1");
        let mut callback = build_engine_record(
            "task_runner_callback::user-1::task-1::task.completed::run-1",
            "assistant",
            "任务「梳理项目用途」已完成\n\n结果摘要：\n已梳理项目用途、核心流程和主要模块。\n更多实施细节可在任务详情中查看。",
            "turn-1",
        );
        callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "event": "task.completed",
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1",
                "task_title": "梳理项目用途",
                "detail_source": "result_summary",
                "result_summary": "已梳理项目用途、核心流程和主要模块。\n更多实施细节可在任务详情中查看。"
            }
        }));

        let compact = build_compact_history_messages_from_turn_slices(vec![
            memory_engine_sdk::TurnRecordSlice {
                turn_id: "turn-1".to_string(),
                user_record: user,
                final_assistant_record: Some(callback),
                has_process: true,
                tool_call_count: 0,
                thinking_count: 0,
                process_message_count: 1,
            },
        ]);

        assert!(compact[1]
            .content
            .contains("已梳理项目用途、核心流程和主要模块。"));
        assert!(compact[1]
            .content
            .contains("更多实施细节可在任务详情中查看。"));
        assert!(!compact[1]
            .content
            .contains("已完成当前任务并通过任务内验证"));
    }

    #[test]
    fn compact_history_sanitizes_legacy_internal_prompt_failure() {
        let user = build_engine_record("user-1", "user", "help", "turn-1");
        let mut callback = build_engine_record(
            "task_runner_callback::user-1::task-1::task.failed::run-1",
            "assistant",
            "任务「实现复杂领域契约」执行失败\n\n结果摘要：\ntask_runner_run_phase failed: resolve published prompt for vendor gpt failed: plugin management request was rejected with status 409: agent_prompt_checksum_invalid",
            "turn-1",
        );
        callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1",
                "task_title": "实现复杂领域契约",
                "result_summary": "agent_prompt_checksum_invalid"
            }
        }));

        let compact = build_compact_history_messages_from_turn_slices(vec![
            memory_engine_sdk::TurnRecordSlice {
                turn_id: "turn-1".to_string(),
                user_record: user,
                final_assistant_record: Some(callback),
                has_process: true,
                tool_call_count: 0,
                thinking_count: 0,
                process_message_count: 1,
            },
        ]);

        assert_eq!(compact[1].content, "任务暂时无法启动，请稍后重试。");
        assert!(!compact[1].content.contains("checksum"));
        let result_summary = compact[1]
            .metadata
            .as_ref()
            .and_then(|value| value.get("task_runner_async"))
            .and_then(|value| value.get("result_summary"))
            .and_then(Value::as_str);
        assert_eq!(result_summary, Some("任务暂时无法启动，请稍后重试。"));
    }

    #[test]
    fn compact_history_from_turn_slices_keeps_plan_summary_before_callback() {
        let user = build_engine_record("user-1", "user", "help", "turn-1");
        let mut plan = build_message("assistant", "I created the async task.");
        plan.id = "assistant-plan".to_string();
        plan.message_mode = Some("task_runner_async_plan".to_string());
        plan.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "message_kind": "plan_summary"
            }
        }));
        let mut callback = build_engine_record(
            "task_runner_callback::user-1::task-1::task.completed::run-1",
            "assistant",
            "Task completed.",
            "turn-1",
        );
        callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));
        let mut process_messages_by_turn = HashMap::new();
        process_messages_by_turn.insert("turn-1".to_string(), vec![plan]);

        let compact = build_compact_history_messages_from_turn_slices_with_process(
            vec![memory_engine_sdk::TurnRecordSlice {
                turn_id: "turn-1".to_string(),
                user_record: user,
                final_assistant_record: Some(callback),
                has_process: true,
                tool_call_count: 0,
                thinking_count: 0,
                process_message_count: 2,
            }],
            &process_messages_by_turn,
        );

        assert_eq!(compact.len(), 3);
        assert_eq!(compact[0].id, "user-1");
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcess"))
                .and_then(|value| value.get("finalAssistantMessageId"))
                .and_then(|value| value.as_str()),
            Some("assistant-plan")
        );
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcess"))
                .and_then(|value| value.get("processMessageCount"))
                .and_then(|value| value.as_u64()),
            Some(1)
        );
        assert_eq!(compact[1].id, "assistant-plan");
        assert_eq!(
            compact[1]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyFinalForUserMessageId"))
                .and_then(|value| value.as_str()),
            Some("user-1")
        );
        assert_eq!(
            compact[2].id,
            "task_runner_callback::user-1::task-1::task.completed::run-1"
        );
    }

    #[test]
    fn compact_history_from_turn_slices_keeps_callback_that_arrived_before_plan_summary() {
        let mut user = build_engine_record("user-1", "user", "help", "turn-1");
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "created_task_ids": ["task-1"],
                "terminal_task_ids": ["task-1"],
                "failed_task_ids": ["task-1"]
            }
        }));
        let mut plan = build_engine_record(
            "assistant-plan",
            "assistant",
            "I created the async task.",
            "turn-1",
        );
        plan.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "message_kind": "plan_summary"
            }
        }));
        let mut callback = build_message("assistant", "Task failed.");
        callback.id = "task_runner_callback::user-1::task-1::task.failed::run-1".to_string();
        callback.message_mode = Some("task_runner_callback".to_string());
        callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));
        let slice = memory_engine_sdk::TurnRecordSlice {
            turn_id: "turn-1".to_string(),
            user_record: user,
            final_assistant_record: Some(plan),
            has_process: true,
            tool_call_count: 0,
            thinking_count: 0,
            process_message_count: 1,
        };
        assert!(turn_slice_needs_task_runner_callback_process_messages(
            &slice
        ));

        let mut process_messages_by_turn = HashMap::new();
        process_messages_by_turn.insert("turn-1".to_string(), vec![callback]);
        let compact = build_compact_history_messages_from_turn_slices_with_process(
            vec![slice],
            &process_messages_by_turn,
        );

        assert_eq!(
            compact
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "user-1",
                "assistant-plan",
                "task_runner_callback::user-1::task-1::task.failed::run-1",
            ]
        );
    }

    #[test]
    fn compact_history_from_turn_slices_keeps_started_callback_while_task_is_running() {
        let mut user = build_engine_record("user-1", "user", "help", "turn-1");
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "overall_status": "processing",
                "last_event": "task.run.started",
                "created_task_ids": ["task-1"],
                "running_task_ids": ["task-1"],
                "terminal_task_ids": [],
                "pending_task_count": 1
            }
        }));
        let mut plan = build_engine_record(
            "assistant-plan",
            "assistant",
            "I created the async task.",
            "turn-1",
        );
        plan.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "message_kind": "plan_summary"
            }
        }));
        let mut started = build_message("assistant", "Task started.");
        started.id = "task_runner_callback::user-1::task-1::run-1".to_string();
        started.message_mode = Some("task_runner_callback".to_string());
        started.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_lifecycle_update",
                "event": "task.run.started",
                "task_id": "task-1",
                "run_id": "run-1",
                "source_turn_id": "turn-1",
                "source_user_message_id": "user-1"
            }
        }));
        let slice = memory_engine_sdk::TurnRecordSlice {
            turn_id: "turn-1".to_string(),
            user_record: user,
            final_assistant_record: Some(plan),
            has_process: true,
            tool_call_count: 0,
            thinking_count: 0,
            process_message_count: 1,
        };
        assert!(turn_slice_needs_task_runner_callback_process_messages(
            &slice
        ));

        let mut process_messages_by_turn = HashMap::new();
        process_messages_by_turn.insert("turn-1".to_string(), vec![started]);
        let compact = build_compact_history_messages_from_turn_slices_with_process(
            vec![slice],
            &process_messages_by_turn,
        );

        assert_eq!(
            compact
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "user-1",
                "assistant-plan",
                "task_runner_callback::user-1::task-1::run-1",
            ]
        );
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("overall_status"))
                .and_then(Value::as_str),
            Some("processing")
        );
    }

    #[test]
    fn compact_history_from_turn_slices_keeps_all_task_runner_callbacks() {
        let user = build_engine_record("user-1", "user", "help", "turn-1");
        let mut plan = build_message("assistant", "I created three async tasks.");
        plan.id = "assistant-plan".to_string();
        plan.message_mode = Some("task_runner_async_plan".to_string());
        plan.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "message_kind": "plan_summary"
            }
        }));
        let mut callback_1 = build_message("assistant", "Task 1 completed.");
        callback_1.id = "task_runner_callback::user-1::task-1::task.completed::run-1".to_string();
        callback_1.message_mode = Some("task_runner_callback".to_string());
        callback_1.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));
        let mut callback_2 = build_message("assistant", "Task 2 completed.");
        callback_2.id = "task_runner_callback::user-1::task-2::task.completed::run-2".to_string();
        callback_2.message_mode = Some("task_runner_callback".to_string());
        callback_2.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));
        let mut final_callback = build_engine_record(
            "task_runner_callback::user-1::task-3::task.completed::run-3",
            "assistant",
            "Task 3 completed.",
            "turn-1",
        );
        final_callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));
        let mut process_messages_by_turn = HashMap::new();
        process_messages_by_turn.insert("turn-1".to_string(), vec![plan, callback_1, callback_2]);

        let compact = build_compact_history_messages_from_turn_slices_with_process(
            vec![memory_engine_sdk::TurnRecordSlice {
                turn_id: "turn-1".to_string(),
                user_record: user,
                final_assistant_record: Some(final_callback),
                has_process: true,
                tool_call_count: 0,
                thinking_count: 0,
                process_message_count: 3,
            }],
            &process_messages_by_turn,
        );

        assert_eq!(
            compact
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "user-1",
                "assistant-plan",
                "task_runner_callback::user-1::task-1::task.completed::run-1",
                "task_runner_callback::user-1::task-2::task.completed::run-2",
                "task_runner_callback::user-1::task-3::task.completed::run-3",
            ]
        );
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcess"))
                .and_then(|value| value.get("processMessageCount"))
                .and_then(|value| value.as_u64()),
            Some(0)
        );
    }

    #[test]
    fn compact_history_from_turn_slices_sorts_recovered_task_runner_callbacks_by_created_at() {
        let user = build_engine_record("user-1", "user", "help", "turn-1");
        let mut earlier_callback = build_message("assistant", "Task failed first.");
        earlier_callback.id =
            "task_runner_callback::user-1::task-1::task.failed::run-1".to_string();
        earlier_callback.created_at = "2026-08-11T09:16:00Z".to_string();
        earlier_callback.message_mode = Some("task_runner_callback".to_string());
        earlier_callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));
        let mut later_callback = build_message("assistant", "Task completed later.");
        later_callback.id =
            "task_runner_callback::user-1::task-1::task.completed::run-2".to_string();
        later_callback.created_at = "2026-08-11T09:28:00Z".to_string();
        later_callback.message_mode = Some("task_runner_callback".to_string());
        later_callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));
        let mut process_messages_by_turn = HashMap::new();
        process_messages_by_turn.insert(
            "turn-1".to_string(),
            vec![later_callback.clone(), earlier_callback.clone()],
        );

        let mut final_assistant = build_engine_record(
            "task_runner_callback::user-1::task-1::task.completed::run-2",
            "assistant",
            "Task completed later.",
            "turn-1",
        );
        final_assistant.created_at = "2026-08-11T09:28:00Z".to_string();
        final_assistant.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update",
                "source_turn_id": "turn-1"
            }
        }));

        let compact = build_compact_history_messages_from_turn_slices_with_process(
            vec![memory_engine_sdk::TurnRecordSlice {
                turn_id: "turn-1".to_string(),
                user_record: user,
                final_assistant_record: Some(final_assistant),
                has_process: true,
                tool_call_count: 0,
                thinking_count: 0,
                process_message_count: 2,
            }],
            &process_messages_by_turn,
        );

        assert_eq!(
            compact
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "user-1",
                "task_runner_callback::user-1::task-1::task.failed::run-1",
                "task_runner_callback::user-1::task-1::task.completed::run-2",
            ]
        );
    }

    #[test]
    fn compact_history_repairs_stale_processing_status_from_terminal_tracking() {
        let mut user = build_message("user", "help");
        user.id = "user-1".to_string();
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "mode": "contact_async",
                "overall_status": "processing",
                "terminal_task_ids": ["task-1"]
            }
        }));

        let compact = build_compact_history_messages(vec![user]);
        assert_eq!(
            compact[0]
                .metadata
                .as_ref()
                .and_then(|value| value.get("task_runner_async"))
                .and_then(|value| value.get("overall_status"))
                .and_then(|value| value.as_str()),
            Some("completed")
        );
    }

    #[test]
    fn turn_display_keeps_task_runner_callbacks_out_of_process_bucket() {
        let mut user = build_message("user", "help");
        user.id = "user-1".to_string();
        user.metadata = Some(json!({
            "conversation_turn_id": "turn-1"
        }));

        let mut plan = build_message("assistant", "I created the tasks.");
        plan.id = "assistant-plan".to_string();
        plan.metadata = Some(json!({
            "conversation_turn_id": "turn-1"
        }));

        let mut callback = build_message("assistant", "Task A completed.");
        callback.id = "assistant-callback".to_string();
        callback.message_mode = Some("task_runner_callback".to_string());
        callback.metadata = Some(json!({
            "conversation_turn_id": "turn-1",
            "task_runner_async": {
                "message_kind": "task_terminal_update"
            }
        }));

        let display = build_turn_display_messages(&[user, plan, callback], 0);
        assert_eq!(display.len(), 3);
        assert_eq!(display[1].id, "assistant-plan");
        assert_eq!(display[2].id, "assistant-callback");
        assert_eq!(
            display[2]
                .metadata
                .as_ref()
                .and_then(|value| value.get("historyProcessUserMessageId")),
            None
        );
    }
}
