use std::collections::HashSet;

use serde_json::Value;

use super::super::prev_context::{
    is_context_length_exceeded_error, is_input_must_be_list_error, is_missing_tool_call_error,
    is_request_body_too_large_error, is_unsupported_previous_response_id_error,
    reduce_history_limit,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RequestErrorReplay {
    pub(super) disable_prev_id: bool,
    pub(super) input_must_be_list: bool,
    pub(super) next_history_limit: Option<i64>,
}

pub(super) fn replay_request_error_policy(
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

pub(super) fn merge_pending_tool_items_into_stateless(
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
