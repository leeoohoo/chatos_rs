use std::collections::HashSet;

use serde_json::Value;
use tokio::time::{sleep, Duration};
use tracing::warn;

use crate::core::messages::{optional_text_has_content, text_has_content};
use crate::services::agent_runtime::ai_request_handler::AiResponse;

use super::execution_loop_guidance::is_non_terminal_finish_reason;
use super::input_transform::{build_current_input_items, to_message_item};
use super::AiClient;

impl AiClient {
    pub(in crate::services::agent_runtime::ai_client) async fn try_recover_from_terminal_empty_response(
        &mut self,
        ai_response: &AiResponse,
        session_id: Option<&String>,
        turn_id: Option<&String>,
        raw_input: &Value,
        stable_prefix_mode: bool,
        include_tool_items: bool,
        prefixed_input_items: &[Value],
        force_text_content: bool,
        terminal_empty_retry_count: &mut usize,
        max_terminal_empty_retries: usize,
        pending_tool_calls: Option<&Vec<Value>>,
        pending_tool_outputs: Option<&Vec<Value>>,
        stateless_context_items: &mut Option<Vec<Value>>,
        input: &mut Value,
        iteration: &mut i64,
    ) -> Result<bool, String> {
        let finish_reason = ai_response.finish_reason.as_deref();
        let has_content = text_has_content(ai_response.content.as_str());
        let has_reasoning = optional_text_has_content(ai_response.reasoning.as_deref());
        let has_tool_calls = ai_response
            .tool_calls
            .as_ref()
            .map(|tool_calls| crate::core::tool_call::tool_calls_value_has_items(Some(tool_calls)))
            .unwrap_or(false);

        if is_non_terminal_finish_reason(finish_reason)
            || has_content
            || has_reasoning
            || has_tool_calls
        {
            return Ok(false);
        }

        *terminal_empty_retry_count += 1;
        let response_id_for_log = ai_response
            .response_id
            .as_deref()
            .unwrap_or("none")
            .to_string();
        warn!(
            "[Agent Runtime] terminal empty response detected: session_id={}, turn_id={}, finish_reason={}, response_id={}, iteration={}, retry={}/{}",
            session_id.map(|value| value.as_str()).unwrap_or("n/a"),
            turn_id.map(|value| value.as_str()).unwrap_or("n/a"),
            finish_reason.unwrap_or("none"),
            response_id_for_log,
            *iteration,
            *terminal_empty_retry_count,
            max_terminal_empty_retries,
        );

        if *terminal_empty_retry_count > max_terminal_empty_retries {
            return Ok(false);
        }

        let mut stateless = self
            .build_stateless_from_raw_input(
                session_id,
                raw_input,
                force_text_content,
                stable_prefix_mode,
                include_tool_items,
                prefixed_input_items,
            )
            .await;
        merge_missing_tool_turn_items_from_pending(
            &mut stateless,
            include_tool_items,
            pending_tool_calls,
            pending_tool_outputs,
        );
        if !stateless.is_empty() {
            *stateless_context_items = Some(stateless.clone());
            *input = Value::Array(stateless);
        }

        let backoff_ms = 250_u64 * *terminal_empty_retry_count as u64;
        sleep(Duration::from_millis(backoff_ms)).await;
        *iteration += 1;
        Ok(true)
    }

    pub(in crate::services::agent_runtime::ai_client) async fn try_recover_from_non_terminal_empty_response(
        &mut self,
        ai_response: &AiResponse,
        session_id: Option<&String>,
        turn_id: Option<&String>,
        raw_input: &Value,
        stable_prefix_mode: bool,
        include_tool_items: bool,
        prefixed_input_items: &[Value],
        force_text_content: bool,
        non_terminal_empty_retry_count: &mut usize,
        max_non_terminal_empty_retries: usize,
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
            "[Agent Runtime] non-terminal empty response detected: session_id={}, turn_id={}, finish_reason={}, response_id={}, iteration={}, retry={}/{}",
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

        let stateless = if let Some(items) = stateless_context_items.clone() {
            items
        } else {
            self.build_stateless_from_raw_input(
                session_id,
                raw_input,
                force_text_content,
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

        let backoff_ms = 200_u64 * *non_terminal_empty_retry_count as u64;
        sleep(Duration::from_millis(backoff_ms)).await;
        *iteration += 1;
        Ok(true)
    }

    pub(in crate::services::agent_runtime::ai_client) async fn advance_after_tool_execution(
        &mut self,
        ai_response: &AiResponse,
        session_id: Option<&String>,
        raw_input: &Value,
        stable_prefix_mode: bool,
        force_text_content: bool,
        prefixed_input_items: &[Value],
        include_tool_items: bool,
        tool_call_items: &[Value],
        tool_outputs: &[Value],
        stateless_context_items: &mut Option<Vec<Value>>,
        pending_tool_calls: &mut Option<Vec<Value>>,
        pending_tool_outputs: &mut Option<Vec<Value>>,
    ) -> Value {
        *pending_tool_outputs = Some(tool_outputs.to_vec());
        *pending_tool_calls = Some(tool_call_items.to_vec());

        let assistant_item =
            build_assistant_response_item(ai_response.content.as_str(), force_text_content);

        let current_items = build_current_input_items(raw_input, force_text_content);
        let stateless = if session_id.is_some() {
            let mut rebuilt = self
                .build_stateless_items(
                    session_id.cloned(),
                    stable_prefix_mode,
                    force_text_content,
                    prefixed_input_items,
                    &current_items,
                    include_tool_items,
                )
                .await;
            merge_missing_tool_turn_items(
                &mut rebuilt,
                include_tool_items,
                tool_call_items,
                tool_outputs,
            );
            rebuilt
        } else {
            let mut local = if let Some(items) = stateless_context_items.clone() {
                items
            } else {
                self.build_stateless_items(
                    session_id.cloned(),
                    stable_prefix_mode,
                    force_text_content,
                    prefixed_input_items,
                    &current_items,
                    include_tool_items,
                )
                .await
            };
            append_tool_turn_items(
                &mut local,
                assistant_item.as_ref(),
                include_tool_items,
                tool_call_items,
                tool_outputs,
            );
            local
        };
        *stateless_context_items = Some(stateless.clone());

        Value::Array(stateless)
    }
}

fn merge_missing_tool_turn_items_from_pending(
    items: &mut Vec<Value>,
    include_tool_items: bool,
    pending_tool_calls: Option<&Vec<Value>>,
    pending_tool_outputs: Option<&Vec<Value>>,
) {
    if !include_tool_items {
        return;
    }

    let tool_call_items = pending_tool_calls
        .map(|items| items.as_slice())
        .unwrap_or(&[]);
    let tool_outputs = pending_tool_outputs
        .map(|items| items.as_slice())
        .unwrap_or(&[]);
    merge_missing_tool_turn_items(items, include_tool_items, tool_call_items, tool_outputs);
}

fn merge_missing_tool_turn_items(
    items: &mut Vec<Value>,
    include_tool_items: bool,
    tool_call_items: &[Value],
    tool_outputs: &[Value],
) {
    if !include_tool_items {
        return;
    }

    let mut existing_call_ids: HashSet<String> = items
        .iter()
        .filter(|item| item.get("type").and_then(|value| value.as_str()) == Some("function_call"))
        .filter_map(|item| {
            item.get("call_id")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .collect();
    let mut pending_call_ids = HashSet::new();

    for item in tool_call_items {
        let Some(call_id) = item.get("call_id").and_then(|value| value.as_str()) else {
            continue;
        };
        if call_id.is_empty() {
            continue;
        }
        pending_call_ids.insert(call_id.to_string());
        if existing_call_ids.insert(call_id.to_string()) {
            items.push(item.clone());
        }
    }

    let mut existing_output_ids: HashSet<String> = items
        .iter()
        .filter(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
        })
        .filter_map(|item| {
            item.get("call_id")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .collect();

    for item in tool_outputs {
        let Some(call_id) = item.get("call_id").and_then(|value| value.as_str()) else {
            continue;
        };
        if call_id.is_empty() || !pending_call_ids.contains(call_id) {
            continue;
        }
        if existing_output_ids.insert(call_id.to_string()) {
            items.push(item.clone());
        }
    }
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
