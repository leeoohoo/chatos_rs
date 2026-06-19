use chatos_ai_runtime::{StatelessHistoryMessage, build_stateless_history_items_with_output_cap};
use serde_json::Value;
use tracing::info;

use super::compat::cap_tool_output_for_input;
use super::{AiClient, build_current_input_items};
use crate::core::messages::is_session_summary_message;

#[cfg(test)]
pub(super) use chatos_ai_runtime::splice_current_input_items;

impl AiClient {
    pub(super) async fn maybe_refresh_stateless_context(
        &self,
        session_id: Option<&str>,
        stable_prefix_mode: bool,
        raw_input: &Value,
        force_text_content: bool,
        include_tool_items: bool,
        prefixed_input_items: &[Value],
        stateless_context_items: &mut Option<Vec<Value>>,
        input: &mut Value,
    ) {
        if !stable_prefix_mode {
            return;
        }

        if session_id.is_none() {
            return;
        }

        let current_items = build_current_input_items(raw_input, force_text_content);
        let rebuilt = self
            .build_stateless_items(
                session_id.map(|value| value.to_string()),
                stable_prefix_mode,
                force_text_content,
                prefixed_input_items,
                &current_items,
                include_tool_items,
            )
            .await;
        let previous_len = stateless_context_items
            .as_ref()
            .map(|items| items.len())
            .unwrap_or(0);
        let changed = stateless_context_items
            .as_ref()
            .map(|items| items != &rebuilt)
            .unwrap_or(true);
        if changed {
            info!(
                "[Agent Runtime] stateless context refreshed: old_items={}, new_items={}",
                previous_len,
                rebuilt.len()
            );
            *stateless_context_items = Some(rebuilt.clone());
            *input = Value::Array(rebuilt);
        }
    }

    pub(super) async fn build_stateless_items(
        &self,
        session_id: Option<String>,
        _stable_prefix_mode: bool,
        force_text: bool,
        prefixed_input_items: &[Value],
        current_input_items: &[Value],
        include_tool_items: bool,
    ) -> Vec<Value> {
        let mut leading_prefixed_items = Vec::new();
        let mut trailing_prefixed_items = Vec::new();
        let memory_summary_count;
        let history_count;
        let mut memory_summary_used = false;
        let context_data = if let Some(sid) = session_id.as_ref() {
            self.message_manager
                .get_memory_chat_history_context(sid)
                .await
        } else {
            (None, 0, Vec::new())
        };

        let (merged_summary, merged_summary_count, pending_history) = context_data;
        memory_summary_count = merged_summary_count;
        if !prefixed_input_items.is_empty() {
            for item in prefixed_input_items {
                if is_task_board_prefixed_input_item(item) {
                    trailing_prefixed_items.push(item.clone());
                } else {
                    leading_prefixed_items.push(item.clone());
                }
            }
        }
        if let Some(summary_text) = merged_summary {
            memory_summary_used = true;
            let history = pending_history;
            history_count = history.len();
            let history_items = history
                .into_iter()
                .map(|msg| StatelessHistoryMessage {
                    role: msg.role.clone(),
                    content: msg.content.clone(),
                    reasoning: msg.reasoning.clone(),
                    tool_calls: msg.tool_calls.clone(),
                    tool_call_id: msg.tool_call_id.clone(),
                    metadata: msg.metadata.clone(),
                    skip_in_input: is_session_summary_message(&msg),
                })
                .collect::<Vec<_>>();

            let items = build_stateless_history_items_with_output_cap(
                leading_prefixed_items.as_slice(),
                trailing_prefixed_items.as_slice(),
                Some(summary_text.as_str()),
                history_items.as_slice(),
                current_input_items,
                include_tool_items,
                force_text,
                cap_tool_output_for_input,
            );
            info!(
                "[Agent Runtime] stateless items built: memory_summary_used={}, summaries={}, history_messages={}, total_items={}",
                memory_summary_used,
                memory_summary_count,
                history_count,
                items.len()
            );
            return items;
        }
        let history = pending_history;
        history_count = history.len();
        let history_items = history
            .into_iter()
            .map(|msg| StatelessHistoryMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
                reasoning: msg.reasoning.clone(),
                tool_calls: msg.tool_calls.clone(),
                tool_call_id: msg.tool_call_id.clone(),
                metadata: msg.metadata.clone(),
                skip_in_input: is_session_summary_message(&msg),
            })
            .collect::<Vec<_>>();
        let items = build_stateless_history_items_with_output_cap(
            leading_prefixed_items.as_slice(),
            trailing_prefixed_items.as_slice(),
            None,
            history_items.as_slice(),
            current_input_items,
            include_tool_items,
            force_text,
            cap_tool_output_for_input,
        );
        info!(
            "[Agent Runtime] stateless items built: memory_summary_used={}, summaries={}, history_messages={}, total_items={}",
            memory_summary_used,
            memory_summary_count,
            history_count,
            items.len()
        );
        items
    }
}

fn is_task_board_prefixed_input_item(item: &Value) -> bool {
    if item.get("type").and_then(|value| value.as_str()) != Some("message") {
        return false;
    }
    if item.get("role").and_then(|value| value.as_str()) != Some("system") {
        return false;
    }

    item.get("content")
        .and_then(|value| value.as_array())
        .and_then(|parts| parts.first())
        .and_then(|part| part.get("text"))
        .and_then(|value| value.as_str())
        .map(|text| text.contains("[Task Board]"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{is_task_board_prefixed_input_item, splice_current_input_items};

    #[test]
    fn splice_current_input_items_replaces_latest_user_in_place() {
        let mut items = vec![
            json!({
                "type": "message",
                "role": "system",
                "content": [{"type": "input_text", "text": "[summary]"}]
            }),
            json!({
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "old user"}]
            }),
            json!({
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "calling tool"}]
            }),
            json!({
                "type": "function_call",
                "call_id": "call_1"
            }),
            json!({
                "type": "function_call_output",
                "call_id": "call_1"
            }),
        ];

        splice_current_input_items(
            &mut items,
            &[json!({
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "new user"}]
            })],
        );

        assert_eq!(
            items,
            vec![
                json!({
                    "type": "message",
                    "role": "system",
                    "content": [{"type": "input_text", "text": "[summary]"}]
                }),
                json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "new user"}]
                }),
                json!({
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "calling tool"}]
                }),
                json!({
                    "type": "function_call",
                    "call_id": "call_1"
                }),
                json!({
                    "type": "function_call_output",
                    "call_id": "call_1"
                }),
            ]
        );
    }

    #[test]
    fn splice_current_input_items_appends_when_history_has_no_user() {
        let mut items = vec![json!({
            "type": "message",
            "role": "system",
            "content": [{"type": "input_text", "text": "[summary]"}]
        })];

        splice_current_input_items(
            &mut items,
            &[json!({
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "hello"}]
            })],
        );

        assert_eq!(items.len(), 2);
        assert_eq!(
            items[1].get("role").and_then(|value| value.as_str()),
            Some("user")
        );
    }

    #[test]
    fn recognizes_task_board_prefixed_item() {
        let item = json!({
            "type": "message",
            "role": "system",
            "content": [{"type": "input_text", "text": "[Task Board]\nfoo"}]
        });
        assert!(is_task_board_prefixed_input_item(&item));
    }

    #[test]
    fn ignores_non_task_board_prefixed_item() {
        let item = json!({
            "type": "message",
            "role": "system",
            "content": [{"type": "input_text", "text": "contact prompt"}]
        });
        assert!(!is_task_board_prefixed_input_item(&item));
    }
}
