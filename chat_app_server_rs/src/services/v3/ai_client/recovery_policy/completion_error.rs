use serde_json::Value;
use tracing::warn;

use super::super::prev_context::{
    is_context_length_exceeded_error, is_missing_tool_call_error,
};
use super::super::AiClient;
use super::support::merge_pending_tool_items_into_stateless;
use crate::services::ai_client_common::AiClientCallbacks;

impl AiClient {
    pub(in crate::services::v3::ai_client) async fn try_recover_from_completion_error(
        &mut self,
        err_msg: &str,
        session_id: Option<&String>,
        raw_input: &Value,
        stable_prefix_mode: bool,
        include_tool_items: bool,
        prefixed_input_items: &[Value],
        pending_tool_calls: Option<&Vec<Value>>,
        pending_tool_outputs: Option<&Vec<Value>>,
        force_text_content: bool,
        adaptive_history_limit: &mut i64,
        use_prev_id: &mut bool,
        can_use_prev_id: &mut bool,
        previous_response_id: &mut Option<String>,
        remote_active_summary_attempted: &mut bool,
        stateless_context_items: &mut Option<Vec<Value>>,
        input: &mut Value,
        callbacks: &AiClientCallbacks,
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

        if is_context_length_exceeded_error(err_msg)
            && self
                .try_remote_active_summary_recovery(
                    session_id,
                    raw_input,
                    force_text_content,
                    *adaptive_history_limit,
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
            *use_prev_id = false;
            *can_use_prev_id = false;
            *previous_response_id = None;
            return true;
        }

        false
    }
}
