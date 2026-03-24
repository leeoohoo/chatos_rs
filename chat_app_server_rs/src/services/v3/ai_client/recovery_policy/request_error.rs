use serde_json::Value;
use tracing::warn;

use super::super::prev_context::{
    is_invalid_input_text_error, is_missing_tool_call_error, is_request_body_too_large_error,
    is_system_messages_not_allowed_error, is_unsupported_previous_response_id_error,
};
use super::super::{
    normalize_input_to_text_value, truncate_function_call_outputs_in_input, AiClient,
};
use super::support::{merge_pending_tool_items_into_stateless, replay_request_error_policy};

impl AiClient {
    pub(in crate::services::v3::ai_client) async fn try_recover_from_request_error(
        &mut self,
        err_msg: &str,
        session_id: Option<&String>,
        raw_input: &Value,
        stable_prefix_mode: bool,
        include_tool_items: bool,
        prefixed_input_items: &[Value],
        pending_tool_calls: Option<&Vec<Value>>,
        pending_tool_outputs: Option<&Vec<Value>>,
        use_prev_id: &mut bool,
        can_use_prev_id: &mut bool,
        force_text_content: &mut bool,
        adaptive_history_limit: &mut i64,
        previous_response_id: &mut Option<String>,
        no_system_messages: &mut bool,
        stateless_context_items: &mut Option<Vec<Value>>,
        input: &mut Value,
    ) -> bool {
        if !*no_system_messages && is_system_messages_not_allowed_error(err_msg) {
            warn!(
                "[AI_V3] provider rejected system-role input; retry with user-role compatibility mode"
            );
            *no_system_messages = true;
            if let Some(sid) = session_id {
                self.no_system_message_sessions.insert(sid.clone());
            }
            return true;
        }

        let request_replay =
            replay_request_error_policy(err_msg, *use_prev_id, *adaptive_history_limit);

        if request_replay.disable_prev_id && is_unsupported_previous_response_id_error(err_msg) {
            if let Some(sid) = session_id {
                self.prev_response_id_disabled_sessions.insert(sid.clone());
            }
            warn!("[AI_V3] previous_response_id unsupported; fallback to stateless mode");
            *can_use_prev_id = false;
            let stateless = self
                .build_stateless_from_raw_input(
                    session_id,
                    raw_input,
                    *force_text_content,
                    *adaptive_history_limit,
                    stable_prefix_mode,
                    include_tool_items,
                    prefixed_input_items,
                )
                .await;
            if !stateless.is_empty() {
                *use_prev_id = false;
                *previous_response_id = None;
                *stateless_context_items = Some(stateless.clone());
                *input = Value::Array(stateless);
                return true;
            }
        }

        if request_replay.disable_prev_id && is_missing_tool_call_error(err_msg) {
            if let Some(sid) = session_id {
                self.prev_response_id_disabled_sessions.insert(sid.clone());
            }
            warn!(
                "[AI_V3] function_call_output missing matching tool call in previous response; fallback to stateless mode"
            );
            *can_use_prev_id = false;
            let mut stateless = if let Some(items) = stateless_context_items.clone() {
                items
            } else {
                self.build_stateless_from_raw_input(
                    session_id,
                    raw_input,
                    *force_text_content,
                    *adaptive_history_limit,
                    stable_prefix_mode,
                    include_tool_items,
                    prefixed_input_items,
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

        if !*force_text_content && is_invalid_input_text_error(err_msg) {
            *force_text_content = true;
            if let Some(sid) = session_id {
                self.force_text_content_sessions.insert(sid.clone());
            }
            *input = normalize_input_to_text_value(input);
            return true;
        }

        if request_replay.input_must_be_list {
            warn!("[AI_V3] provider requires list input; retry with message-list payload");
            let normalized_items = if let Some(items) = input.as_array() {
                items.clone()
            } else {
                let mut items = prefixed_input_items.to_vec();
                items.extend(super::super::build_current_input_items(
                    input,
                    *force_text_content,
                ));
                items
            };
            *input = Value::Array(normalized_items.clone());
            *stateless_context_items = Some(normalized_items);
            return true;
        }

        let request_too_large = is_request_body_too_large_error(err_msg);
        if request_too_large {
            if let Some(trimmed_input) = truncate_function_call_outputs_in_input(input) {
                warn!(
                    "[AI_V3] request payload too large; retry with truncated function_call_output items"
                );
                *use_prev_id = false;
                *previous_response_id = None;
                *stateless_context_items = trimmed_input.as_array().cloned();
                *input = trimmed_input;
                return true;
            }
        }

        if let Some(next_limit) = request_replay.next_history_limit {
            warn!(
                "[AI_V3] context/payload overflow; reduce history_limit {} -> {}",
                *adaptive_history_limit, next_limit
            );
            *adaptive_history_limit = next_limit;
            *can_use_prev_id = false;
            let stateless = self
                .build_stateless_from_raw_input(
                    session_id,
                    raw_input,
                    *force_text_content,
                    *adaptive_history_limit,
                    stable_prefix_mode,
                    include_tool_items,
                    prefixed_input_items,
                )
                .await;
            if !stateless.is_empty() {
                *use_prev_id = false;
                *previous_response_id = None;
                *stateless_context_items = Some(stateless.clone());
                *input = Value::Array(stateless);
                return true;
            }
        }

        false
    }
}
