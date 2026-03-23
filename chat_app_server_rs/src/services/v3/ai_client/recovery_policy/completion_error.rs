use serde_json::Value;
use tracing::warn;

use super::super::prev_context::{
    is_context_length_exceeded_error, is_missing_tool_call_error, is_request_body_too_large_error,
    reduce_history_limit,
};
use super::super::{truncate_function_call_outputs_in_input, AiClient};
use super::support::merge_pending_tool_items_into_stateless;

impl AiClient {
    pub(in crate::services::v3::ai_client) async fn try_recover_from_completion_error(
        &mut self,
        err_msg: &str,
        session_id: Option<&String>,
        raw_input: &Value,
        stable_prefix_mode: bool,
        include_tool_items: bool,
        pending_tool_calls: Option<&Vec<Value>>,
        pending_tool_outputs: Option<&Vec<Value>>,
        force_text_content: bool,
        adaptive_history_limit: &mut i64,
        use_prev_id: &mut bool,
        can_use_prev_id: &mut bool,
        previous_response_id: &mut Option<String>,
        stateless_context_items: &mut Option<Vec<Value>>,
        input: &mut Value,
    ) -> bool {
        if *use_prev_id && is_missing_tool_call_error(err_msg) {
            if let Some(sid) = session_id {
                self.prev_response_id_disabled_sessions.insert(sid.clone());
            }
            warn!(
                "[AI_V3] completion failed due to missing tool call context; fallback to stateless mode"
            );
            *can_use_prev_id = false;
            let mut stateless = if let Some(items) = stateless_context_items.clone() {
                items
            } else {
                self.build_stateless_from_raw_input(
                    session_id,
                    raw_input,
                    force_text_content,
                    *adaptive_history_limit,
                    stable_prefix_mode,
                    include_tool_items,
                )
                .await
            };
            if include_tool_items {
                merge_pending_tool_items_into_stateless(
                    &mut stateless,
                    pending_tool_calls,
                    pending_tool_outputs,
                );
            }
            if !stateless.is_empty() {
                *use_prev_id = false;
                *previous_response_id = None;
                *stateless_context_items = Some(stateless.clone());
                *input = Value::Array(stateless);
                return true;
            }
        }

        let request_too_large = is_request_body_too_large_error(err_msg);
        if request_too_large {
            if let Some(trimmed_input) = truncate_function_call_outputs_in_input(input) {
                warn!(
                    "[AI_V3] failed response due to payload size; retry with truncated function_call_output items"
                );
                *use_prev_id = false;
                *previous_response_id = None;
                *stateless_context_items = trimmed_input.as_array().cloned();
                *input = trimmed_input;
                return true;
            }
        }
        if is_context_length_exceeded_error(err_msg) || request_too_large {
            if let Some(next_limit) = reduce_history_limit(*adaptive_history_limit) {
                warn!(
                    "[AI_V3] failed response due to context/payload overflow; reduce history_limit {} -> {}",
                    *adaptive_history_limit, next_limit
                );
                *adaptive_history_limit = next_limit;
                *can_use_prev_id = false;
                *use_prev_id = false;
                *previous_response_id = None;
                let stateless = self
                    .build_stateless_from_raw_input(
                        session_id,
                        raw_input,
                        force_text_content,
                        *adaptive_history_limit,
                        stable_prefix_mode,
                        include_tool_items,
                    )
                    .await;
                if !stateless.is_empty() {
                    *stateless_context_items = Some(stateless.clone());
                    *input = Value::Array(stateless);
                    return true;
                }
            }
        }

        false
    }
}
