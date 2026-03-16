use std::collections::HashSet;

use serde_json::{json, Value};
use tracing::info;

use super::compat::cap_tool_output_for_input;
use super::{build_current_input_items, to_message_item, AiClient};

impl AiClient {
    pub(super) async fn maybe_refresh_stateless_context(
        &self,
        session_id: Option<&str>,
        stable_prefix_mode: bool,
        use_prev_id: bool,
        raw_input: &Value,
        force_text_content: bool,
        history_limit: i64,
        include_tool_items: bool,
        stateless_context_items: &mut Option<Vec<Value>>,
        input: &mut Value,
    ) {
        if !stable_prefix_mode || use_prev_id {
            return;
        }

        if session_id.is_none() {
            return;
        }

        let current_items = build_current_input_items(raw_input, force_text_content);
        let rebuilt = self
            .build_stateless_items(
                session_id.map(|value| value.to_string()),
                history_limit,
                stable_prefix_mode,
                force_text_content,
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
                "[AI_V3] stateless context refreshed: old_items={}, new_items={}, history_limit={}",
                previous_len,
                rebuilt.len(),
                history_limit
            );
            *stateless_context_items = Some(rebuilt.clone());
            *input = Value::Array(rebuilt);
        }
    }

    pub(super) async fn build_stateless_items(
        &self,
        session_id: Option<String>,
        history_limit: i64,
        stable_prefix_mode: bool,
        force_text: bool,
        current_input_items: &[Value],
        include_tool_items: bool,
    ) -> Vec<Value> {
        let mut items = Vec::new();
        let memory_summary_count;
        let history_count;
        let mut memory_summary_used = false;
        let mut tool_call_ids: HashSet<String> = HashSet::new();
        let mut tool_output_ids: HashSet<String> = HashSet::new();
        let context_data = if let Some(sid) = session_id.as_ref() {
            self.message_manager
                .get_memory_chat_history_context(sid, 2)
                .await
        } else {
            (None, 0, Vec::new())
        };

        let use_full_pending_history = stable_prefix_mode && history_limit >= self.history_limit;
        let (merged_summary, merged_summary_count, mut pending_history) = context_data;
        memory_summary_count = merged_summary_count;
        if let Some(summary_text) = merged_summary {
            memory_summary_used = true;
            items.push(to_message_item(
                "system",
                &Value::String(summary_text),
                force_text,
            ));
        }
        if !use_full_pending_history
            && history_limit > 0
            && pending_history.len() > history_limit as usize
        {
            let keep_from = pending_history.len() - history_limit as usize;
            pending_history = pending_history.split_off(keep_from);
        }
        let history = pending_history;

        history_count = history.len();

        if include_tool_items {
            for msg in &history {
                if msg.role == "tool" {
                    if let Some(call_id) = msg.tool_call_id.clone() {
                        if !call_id.is_empty() {
                            tool_output_ids.insert(call_id);
                        }
                    }
                }
            }
        }

        for msg in history {
            if msg
                .metadata
                .as_ref()
                .and_then(|m| m.get("type"))
                .and_then(|v| v.as_str())
                == Some("session_summary")
            {
                continue;
            }

            if msg.role == "user"
                || msg.role == "assistant"
                || msg.role == "system"
                || msg.role == "developer"
            {
                items.push(to_message_item(
                    &msg.role,
                    &Value::String(msg.content.clone()),
                    force_text,
                ));
                if include_tool_items {
                    let mut tool_calls = msg.tool_calls.clone().or_else(|| {
                        msg.metadata
                            .as_ref()
                            .and_then(|m| m.get("toolCalls").cloned())
                    });
                    if let Some(Value::String(s)) = tool_calls.clone() {
                        if let Ok(v) = serde_json::from_str::<Value>(&s) {
                            tool_calls = Some(v);
                        }
                    }
                    if msg.role == "assistant" {
                        if let Some(arr) = tool_calls.and_then(|v| v.as_array().cloned()) {
                            for tc in arr {
                                let call_id = tc
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .or_else(|| tc.get("call_id").and_then(|v| v.as_str()))
                                    .unwrap_or("")
                                    .to_string();
                                if call_id.is_empty() {
                                    continue;
                                }
                                if !tool_output_ids.contains(&call_id) {
                                    continue;
                                }
                                let func = tc.get("function").cloned().unwrap_or(json!({}));
                                let name = func
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .or_else(|| tc.get("name").and_then(|v| v.as_str()))
                                    .unwrap_or("")
                                    .to_string();
                                let args = func
                                    .get("arguments")
                                    .cloned()
                                    .or_else(|| tc.get("arguments").cloned())
                                    .unwrap_or(Value::String("{}".to_string()));
                                let args_str = if let Some(s) = args.as_str() {
                                    s.to_string()
                                } else {
                                    args.to_string()
                                };
                                tool_call_ids.insert(call_id.clone());
                                items.push(json!({
                                    "type": "function_call",
                                    "call_id": call_id,
                                    "name": name,
                                    "arguments": args_str
                                }));
                            }
                        }
                    }
                }
                continue;
            }
            if msg.role == "tool" {
                if include_tool_items {
                    if let Some(call_id) = msg.tool_call_id.clone() {
                        if tool_call_ids.contains(&call_id) {
                            let output = cap_tool_output_for_input(msg.content.as_str());
                            items.push(json!({
                                "type": "function_call_output",
                                "call_id": call_id,
                                "output": output
                            }));
                        }
                    }
                }
            }
        }

        if let Some(last) = items.last() {
            if last.get("type").and_then(|v| v.as_str()) == Some("message")
                && last.get("role").and_then(|v| v.as_str()) == Some("user")
            {
                items.pop();
            }
        }
        items.extend_from_slice(current_input_items);
        info!(
            "[AI_V3] stateless items built: stable_prefix_mode={}, memory_summary_used={}, summaries={}, history_messages={}, total_items={}",
            stable_prefix_mode,
            memory_summary_used,
            memory_summary_count,
            history_count,
            items.len()
        );
        items
    }
}
