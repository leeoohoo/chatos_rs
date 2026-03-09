use std::collections::HashSet;

use serde_json::Value;
use tracing::warn;

use super::prev_context::{
    is_context_length_exceeded_error, is_input_must_be_list_error, is_invalid_input_text_error,
    is_missing_tool_call_error, is_request_body_too_large_error,
    is_system_messages_not_allowed_error, is_unsupported_previous_response_id_error,
    reduce_history_limit,
};
use super::{
    build_current_input_items, normalize_input_to_text_value,
    truncate_function_call_outputs_in_input, AiClient,
};

impl AiClient {
    pub(super) async fn try_recover_from_request_error(
        &mut self,
        err_msg: &str,
        session_id: Option<&String>,
        sub_agent_run_id: Option<&String>,
        raw_input: &Value,
        stable_prefix_mode: bool,
        include_tool_items: bool,
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
                    sub_agent_run_id,
                    raw_input,
                    *force_text_content,
                    *adaptive_history_limit,
                    stable_prefix_mode,
                    include_tool_items,
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
                    sub_agent_run_id,
                    raw_input,
                    *force_text_content,
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
                build_current_input_items(input, *force_text_content)
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
                    sub_agent_run_id,
                    raw_input,
                    *force_text_content,
                    *adaptive_history_limit,
                    stable_prefix_mode,
                    include_tool_items,
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

    pub(super) async fn try_recover_from_completion_error(
        &mut self,
        err_msg: &str,
        session_id: Option<&String>,
        sub_agent_run_id: Option<&String>,
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
                    sub_agent_run_id,
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
                        sub_agent_run_id,
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

    async fn build_stateless_from_raw_input(
        &self,
        session_id: Option<&String>,
        sub_agent_run_id: Option<&String>,
        raw_input: &Value,
        force_text_content: bool,
        history_limit: i64,
        stable_prefix_mode: bool,
        include_tool_items: bool,
    ) -> Vec<Value> {
        let current_items = build_current_input_items(raw_input, force_text_content);
        self.build_stateless_items(
            session_id.cloned(),
            history_limit,
            stable_prefix_mode,
            force_text_content,
            &current_items,
            include_tool_items,
            sub_agent_run_id.cloned(),
        )
        .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RequestErrorReplay {
    disable_prev_id: bool,
    input_must_be_list: bool,
    next_history_limit: Option<i64>,
}

fn replay_request_error_policy(
    err_msg: &str,
    use_prev_id: bool,
    adaptive_history_limit: i64,
) -> RequestErrorReplay {
    let request_too_large = is_request_body_too_large_error(err_msg);
    let disable_prev_id = use_prev_id
        && (is_unsupported_previous_response_id_error(err_msg)
            || is_missing_tool_call_error(err_msg));
    let next_history_limit = if is_context_length_exceeded_error(err_msg) || request_too_large {
        reduce_history_limit(adaptive_history_limit)
    } else {
        None
    };

    RequestErrorReplay {
        disable_prev_id,
        input_must_be_list: is_input_must_be_list_error(err_msg),
        next_history_limit,
    }
}

fn merge_pending_tool_items_into_stateless(
    stateless: &mut Vec<Value>,
    pending_tool_calls: Option<&Vec<Value>>,
    pending_tool_outputs: Option<&Vec<Value>>,
) {
    let mut call_ids: HashSet<String> = HashSet::new();
    let mut existing_call_ids: HashSet<String> = stateless
        .iter()
        .filter(|item| item.get("type").and_then(|v| v.as_str()) == Some("function_call"))
        .filter_map(|item| {
            item.get("call_id")
                .and_then(|v| v.as_str())
                .map(|value| value.to_string())
        })
        .collect();
    let mut existing_output_ids: HashSet<String> = stateless
        .iter()
        .filter(|item| item.get("type").and_then(|v| v.as_str()) == Some("function_call_output"))
        .filter_map(|item| {
            item.get("call_id")
                .and_then(|v| v.as_str())
                .map(|value| value.to_string())
        })
        .collect();

    if let Some(calls) = pending_tool_calls {
        for c in calls {
            if let Some(id) = c.get("call_id").and_then(|v| v.as_str()) {
                if !id.is_empty() {
                    call_ids.insert(id.to_string());
                    if existing_call_ids.insert(id.to_string()) {
                        stateless.push(c.clone());
                    }
                }
            }
        }
    }

    if let Some(outputs) = pending_tool_outputs {
        if call_ids.is_empty() {
            return;
        }
        for output in outputs {
            let Some(id) = output
                .get("call_id")
                .and_then(|v| v.as_str())
                .map(|value| value.to_string())
            else {
                continue;
            };
            if !call_ids.contains(id.as_str()) {
                continue;
            }
            if existing_output_ids.insert(id) {
                stateless.push(output.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{merge_pending_tool_items_into_stateless, replay_request_error_policy};

    #[test]
    fn merges_unique_calls_and_matching_outputs_only() {
        let mut stateless = vec![
            json!({"type":"function_call","call_id":"call_1","name":"foo","arguments":"{}"}),
            json!({"type":"function_call_output","call_id":"call_1","output":"ok"}),
        ];
        let pending_calls = vec![
            json!({"type":"function_call","call_id":"call_1","name":"foo","arguments":"{}"}),
            json!({"type":"function_call","call_id":"call_2","name":"bar","arguments":"{}"}),
        ];
        let pending_outputs = vec![
            json!({"type":"function_call_output","call_id":"call_2","output":"done"}),
            json!({"type":"function_call_output","call_id":"call_3","output":"skip"}),
        ];

        merge_pending_tool_items_into_stateless(
            &mut stateless,
            Some(&pending_calls),
            Some(&pending_outputs),
        );

        assert!(stateless
            .iter()
            .any(
                |item| item.get("call_id").and_then(|v| v.as_str()) == Some("call_2")
                    && item.get("type").and_then(|v| v.as_str()) == Some("function_call")
            ));
        assert!(stateless
            .iter()
            .any(
                |item| item.get("call_id").and_then(|v| v.as_str()) == Some("call_2")
                    && item.get("type").and_then(|v| v.as_str()) == Some("function_call_output")
            ));
        assert!(!stateless
            .iter()
            .any(|item| item.get("call_id").and_then(|v| v.as_str()) == Some("call_3")));
    }

    #[test]
    fn skips_outputs_when_no_pending_calls() {
        let mut stateless = vec![];
        let pending_outputs =
            vec![json!({"type":"function_call_output","call_id":"call_2","output":"done"})];

        merge_pending_tool_items_into_stateless(&mut stateless, None, Some(&pending_outputs));

        assert!(stateless.is_empty());
    }

    #[test]
    fn replays_prev_id_disable_when_provider_rejects_previous_response_id() {
        let replay =
            replay_request_error_policy("unsupported parameter: previous_response_id", true, 20);
        assert!(replay.disable_prev_id);
        assert!(!replay.input_must_be_list);
        assert_eq!(replay.next_history_limit, None);

        let no_prev_replay =
            replay_request_error_policy("unsupported parameter: previous_response_id", false, 20);
        assert!(!no_prev_replay.disable_prev_id);
    }

    #[test]
    fn replays_history_limit_reduction_for_context_overflow_samples() {
        let replay = replay_request_error_policy(
            "context_length_exceeded: input exceeds the context window",
            false,
            20,
        );
        assert_eq!(replay.next_history_limit, Some(10));
        assert!(!replay.disable_prev_id);
        assert!(!replay.input_must_be_list);
    }

    #[test]
    fn replays_input_must_be_list_branch() {
        let replay = replay_request_error_policy("Bad Request: input must be a list", false, 20);
        assert!(replay.input_must_be_list);
        assert!(!replay.disable_prev_id);
        assert_eq!(replay.next_history_limit, None);
    }
}
