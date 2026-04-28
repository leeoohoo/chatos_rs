use serde_json::{json, Value};
use tracing::info;

use crate::core::messages::{
    attach_message_tool_calls, attach_reasoning_content, build_assistant_role_message,
    is_session_summary_message,
};
use crate::core::tool_call::extract_message_tool_calls;
use crate::core::tool_call::build_tool_role_message;

use super::history_tools::ensure_tool_responses;
use super::runtime_support::cap_tool_content_for_input;
use super::AiClient;

impl AiClient {
    pub(super) async fn load_runtime_prefixed_messages(&self) -> Option<Vec<Value>> {
        self.task_board_refresh_context.load_prefixed_messages().await
    }

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
            if is_session_summary_message(&msg) {
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
                mapped.push(build_tool_role_message(
                    msg.tool_call_id.clone().unwrap_or_default().as_str(),
                    content.as_str(),
                ));
            } else {
                let mut item = if msg.role == "assistant" {
                    build_assistant_role_message(Value::String(msg.content.clone()))
                } else {
                    json!({"role": msg.role, "content": msg.content})
                };
                let tool_calls =
                    extract_message_tool_calls(msg.tool_calls.as_ref(), msg.metadata.as_ref());
                attach_message_tool_calls(
                    &mut item,
                    (!tool_calls.is_empty()).then_some(Value::Array(tool_calls)),
                );
                if include_reasoning && msg.role == "assistant" {
                    let preserve_empty_reasoning = item
                        .get("tool_calls")
                        .map(|value| !value.is_null())
                        .unwrap_or(false);
                    attach_reasoning_content(
                        &mut item,
                        msg.reasoning.as_deref(),
                        preserve_empty_reasoning,
                    );
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
        if let Some(items) = self.load_runtime_prefixed_messages().await {
            refreshed.extend(items);
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
