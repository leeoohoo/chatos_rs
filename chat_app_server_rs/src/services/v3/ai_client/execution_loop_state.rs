use serde_json::Value;
use tokio::time::{sleep, Duration};
use tracing::warn;

use crate::core::messages::{optional_text_has_content, text_has_content};
use crate::services::v3::ai_request_handler::AiResponse;

use super::execution_loop_guidance::is_non_terminal_finish_reason;
use super::input_transform::{build_current_input_items, to_message_item};
use super::prev_context::should_use_prev_id_for_next_turn;
use super::AiClient;

impl AiClient {
    pub(in crate::services::v3::ai_client) async fn try_recover_from_non_terminal_empty_response(
        &mut self,
        ai_response: &AiResponse,
        session_id: Option<&String>,
        turn_id: Option<&String>,
        raw_input: &Value,
        stable_prefix_mode: bool,
        include_tool_items: bool,
        prefixed_input_items: &[Value],
        force_text_content: bool,
        adaptive_history_limit: i64,
        non_terminal_empty_retry_count: &mut usize,
        max_non_terminal_empty_retries: usize,
        use_prev_id: &mut bool,
        can_use_prev_id: &mut bool,
        previous_response_id: &mut Option<String>,
        stateless_context_items: &mut Option<Vec<Value>>,
        input: &mut Value,
        iteration: &mut i64,
    ) -> Result<bool, String> {
        let finish_reason = ai_response.finish_reason.as_deref();
        let has_content = text_has_content(ai_response.content.as_str());
        let has_reasoning = optional_text_has_content(ai_response.reasoning.as_deref());

        if !is_non_terminal_finish_reason(finish_reason) || has_content || has_reasoning {
            return Ok(false);
        }

        *non_terminal_empty_retry_count += 1;
        let response_id_for_log = ai_response
            .response_id
            .as_deref()
            .unwrap_or("none")
            .to_string();
        warn!(
            "[AI_V3] non-terminal empty response detected: session_id={}, turn_id={}, finish_reason={}, response_id={}, iteration={}, retry={}/{}",
            session_id.map(|value| value.as_str()).unwrap_or("n/a"),
            turn_id.map(|value| value.as_str()).unwrap_or("n/a"),
            finish_reason.unwrap_or("none"),
            response_id_for_log,
            *iteration,
            *non_terminal_empty_retry_count,
            max_non_terminal_empty_retries,
        );

        if *non_terminal_empty_retry_count > max_non_terminal_empty_retries {
            return Err(format!(
                "AI 响应未完成（finish_reason={}）且未返回内容，重试 {} 次后仍未恢复",
                finish_reason.unwrap_or("unknown"),
                max_non_terminal_empty_retries
            ));
        }

        if *use_prev_id {
            warn!(
                "[AI_V3] disable previous_response_id after non-terminal empty response: session_id={}",
                session_id.map(|value| value.as_str()).unwrap_or("n/a")
            );
            if let Some(sid) = session_id {
                self.prev_response_id_disabled_sessions.insert(sid.clone());
            }
            *can_use_prev_id = false;
            *use_prev_id = false;
            *previous_response_id = None;
            let stateless = if let Some(items) = stateless_context_items.clone() {
                items
            } else {
                self.build_stateless_from_raw_input(
                    session_id,
                    raw_input,
                    force_text_content,
                    adaptive_history_limit,
                    stable_prefix_mode,
                    include_tool_items,
                    prefixed_input_items,
                )
                .await
            };
            if !stateless.is_empty() {
                *stateless_context_items = Some(stateless.clone());
                *input = Value::Array(stateless);
            }
        }

        let backoff_ms = 200_u64 * *non_terminal_empty_retry_count as u64;
        sleep(Duration::from_millis(backoff_ms)).await;
        *iteration += 1;
        Ok(true)
    }

    pub(in crate::services::v3::ai_client) async fn advance_after_tool_execution(
        &mut self,
        ai_response: &AiResponse,
        session_id: Option<&String>,
        raw_input: &Value,
        adaptive_history_limit: i64,
        stable_prefix_mode: bool,
        force_text_content: bool,
        prefixed_input_items: &[Value],
        include_tool_items: bool,
        prefer_stateless: bool,
        use_prev_id: bool,
        can_use_prev_id: &mut bool,
        tool_call_items: &[Value],
        tool_outputs: &[Value],
        stateless_context_items: &mut Option<Vec<Value>>,
        pending_tool_calls: &mut Option<Vec<Value>>,
        pending_tool_outputs: &mut Option<Vec<Value>>,
    ) -> (Value, Option<String>, bool) {
        *pending_tool_outputs = Some(tool_outputs.to_vec());
        *pending_tool_calls = Some(tool_call_items.to_vec());

        let assistant_item =
            build_assistant_response_item(ai_response.content.as_str(), force_text_content);
        if let Some(items) = stateless_context_items.as_mut() {
            append_tool_turn_items(
                items,
                assistant_item.as_ref(),
                include_tool_items,
                tool_call_items,
                tool_outputs,
            );
        }

        let mut next_input = Value::Array(tool_outputs.to_vec());
        let mut next_prev_id = ai_response.response_id.clone();
        let mut next_use_prev_id = should_use_prev_id_for_next_turn(
            prefer_stateless,
            *can_use_prev_id,
            next_prev_id.is_some(),
        );
        if use_prev_id && next_prev_id.is_none() {
            warn!("[AI_V3] missing response_id for tool call; fallback to stateless input");
            if let Some(sid) = session_id {
                self.prev_response_id_disabled_sessions.insert(sid.clone());
            }
            *can_use_prev_id = false;
            next_use_prev_id = false;
        }

        let needs_fresh_stateless_context = stateless_context_items.is_none();
        if !next_use_prev_id {
            let mut stateless = if let Some(items) = stateless_context_items.clone() {
                items
            } else {
                let current_items = build_current_input_items(raw_input, force_text_content);
                self.build_stateless_items(
                    session_id.cloned(),
                    adaptive_history_limit,
                    stable_prefix_mode,
                    force_text_content,
                    prefixed_input_items,
                    &current_items,
                    include_tool_items,
                )
                .await
            };

            if needs_fresh_stateless_context {
                append_tool_turn_items(
                    &mut stateless,
                    assistant_item.as_ref(),
                    include_tool_items,
                    tool_call_items,
                    tool_outputs,
                );
                *stateless_context_items = Some(stateless.clone());
            }

            next_input = Value::Array(stateless);
            next_prev_id = None;
        }

        (next_input, next_prev_id, next_use_prev_id)
    }
}

fn build_assistant_response_item(content: &str, force_text_content: bool) -> Option<Value> {
    if content.is_empty() {
        return None;
    }

    Some(to_message_item(
        "assistant",
        &Value::String(content.to_string()),
        force_text_content,
    ))
}

fn append_tool_turn_items(
    items: &mut Vec<Value>,
    assistant_item: Option<&Value>,
    include_tool_items: bool,
    tool_call_items: &[Value],
    tool_outputs: &[Value],
) {
    if let Some(item) = assistant_item {
        items.push(item.clone());
    }
    if include_tool_items {
        items.extend(tool_call_items.iter().cloned());
        items.extend(tool_outputs.iter().cloned());
    }
}
