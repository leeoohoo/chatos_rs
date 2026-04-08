use serde_json::{json, Value};
use tracing::info;

use super::history_tools::ensure_tool_responses;
use super::runtime_support::cap_tool_content_for_input;
use super::AiClient;

impl AiClient {
    pub(super) async fn load_memory_context_messages_for_scope(
        &self,
        session_id: Option<&str>,
        include_reasoning: bool,
    ) -> Vec<Value> {
        let mut mapped = Vec::new();
        let (merged_summary, _summary_count, history) = if let Some(sid) = session_id {
            self.message_manager
                .get_memory_chat_history_context(sid, 2)
                .await
        } else {
            (None, 0, Vec::new())
        };
        if let Some(summary_text) = merged_summary {
            mapped.push(json!({"role": "system", "content": summary_text}));
        }

        for msg in history {
            if msg
                .metadata
                .as_ref()
                .and_then(|m| m.get("type"))
                .and_then(|v| v.as_str())
                .map(|kind| kind == "session_summary" || kind == "task_execution_notice")
                .unwrap_or(false)
            {
                continue;
            }
            if msg.role == "tool" {
                let mut content = msg.content;
                if content.is_empty() && msg.metadata.is_some() {
                    content = msg
                        .metadata
                        .clone()
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                }
                content = cap_tool_content_for_input(content.as_str());
                mapped.push(json!({
                    "role": "tool",
                    "tool_call_id": msg.tool_call_id.clone().unwrap_or_default(),
                    "content": content
                }));
            } else {
                let mut item = json!({"role": msg.role, "content": msg.content});
                if let Some(tc) = msg.tool_calls {
                    item["tool_calls"] = tc;
                }
                if let Some(tc) = msg
                    .metadata
                    .clone()
                    .and_then(|m| m.get("toolCalls").cloned())
                {
                    item["tool_calls"] = tc;
                }
                if include_reasoning && msg.role == "assistant" {
                    let has_tool_calls = item
                        .get("tool_calls")
                        .map(|value| !value.is_null())
                        .unwrap_or(false);
                    if has_tool_calls {
                        item["reasoning_content"] =
                            Value::String(msg.reasoning.clone().unwrap_or_default());
                    } else if let Some(reasoning) = msg.reasoning.clone() {
                        if !reasoning.trim().is_empty() {
                            item["reasoning_content"] = Value::String(reasoning);
                        }
                    }
                }
                mapped.push(item);
            }
        }

        mapped
    }

    pub(super) async fn maybe_refresh_context_from_memory(
        &self,
        purpose: &str,
        iteration: i64,
        session_id: Option<&str>,
        include_reasoning: bool,
        messages: &mut Vec<Value>,
    ) {
        if purpose != "chat" || iteration <= 0 {
            return;
        }
        if session_id.is_none() {
            return;
        }

        let mut refreshed = Vec::new();
        if let Some(prompt) = self.system_prompt.clone() {
            refreshed.push(json!({"role": "system", "content": prompt}));
        }
        let mapped = self
            .load_memory_context_messages_for_scope(session_id, include_reasoning)
            .await;
        refreshed.extend(ensure_tool_responses(mapped));
        if refreshed != *messages {
            info!(
                "[AI_V2] context refreshed from memory_context: old_messages={}, new_messages={}",
                messages.len(),
                refreshed.len()
            );
            *messages = refreshed;
        }
    }
}
