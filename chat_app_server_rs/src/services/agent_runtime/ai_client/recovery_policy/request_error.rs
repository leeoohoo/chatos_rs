use serde_json::Value;
use tracing::warn;

use super::super::prev_context::{
    is_context_length_exceeded_error, is_invalid_input_text_error, is_missing_tool_call_error,
    is_system_messages_not_allowed_error,
};
use super::super::{normalize_input_to_text_value, AiClient};
use super::support::{merge_pending_tool_items_into_stateless, replay_request_error_policy};
use crate::services::ai_client_common::AiClientCallbacks;

impl AiClient {
    pub(in crate::services::agent_runtime::ai_client) async fn try_recover_from_request_error(
        &mut self,
        err_msg: &str,
        session_id: Option<&String>,
        raw_input: &Value,
        stable_prefix_mode: bool,
        include_tool_items: bool,
        prefixed_input_items: &[Value],
        pending_tool_calls: Option<&Vec<Value>>,
        pending_tool_outputs: Option<&Vec<Value>>,
        force_text_content: &mut bool,
        no_system_messages: &mut bool,
        remote_active_summary_attempted: &mut bool,
        stateless_context_items: &mut Option<Vec<Value>>,
        input: &mut Value,
        callbacks: &AiClientCallbacks,
    ) -> bool {
        if !*no_system_messages && is_system_messages_not_allowed_error(err_msg) {
            warn!(
                "[Agent Runtime] provider rejected system-role input; retry with user-role compatibility mode"
            );
            *no_system_messages = true;
            if let Some(sid) = session_id {
                self.no_system_message_sessions.insert(sid.clone());
            }
            return true;
        }

        let request_replay = replay_request_error_policy(err_msg);

        if is_context_length_exceeded_error(err_msg)
            && self
                .try_remote_active_summary_recovery(
                    session_id,
                    raw_input,
                    *force_text_content,
                    stable_prefix_mode,
                    include_tool_items,
                    prefixed_input_items,
                    remote_active_summary_attempted,
                    stateless_context_items,
                    input,
                    callbacks,
                )
                .await
        {
            return true;
        }

        if request_replay.rebuild_stateless_on_missing_tool_call
            && is_missing_tool_call_error(err_msg)
        {
            warn!(
                "[Agent Runtime] function_call_output missing matching tool call context; rebuild stateless input"
            );
            let mut stateless = if let Some(items) = stateless_context_items.clone() {
                items
            } else {
                self.build_stateless_from_raw_input(
                    session_id,
                    raw_input,
                    *force_text_content,
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
            warn!("[Agent Runtime] provider requires list input; retry with message-list payload");
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

        false
    }
}
