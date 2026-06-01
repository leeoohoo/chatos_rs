use std::collections::HashSet;

use serde_json::Value;

use super::super::prev_context::{
    is_input_must_be_list_error, is_missing_tool_call_error,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RequestErrorReplay {
    pub(super) rebuild_stateless_on_missing_tool_call: bool,
    pub(super) input_must_be_list: bool,
}

pub(super) fn replay_request_error_policy(err_msg: &str) -> RequestErrorReplay {
    RequestErrorReplay {
        rebuild_stateless_on_missing_tool_call: is_missing_tool_call_error(err_msg),
        input_must_be_list: is_input_must_be_list_error(err_msg),
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
    fn ignores_prev_id_errors_in_stateless_mode() {
        let replay = replay_request_error_policy("unsupported parameter: prev_id");
        assert!(!replay.rebuild_stateless_on_missing_tool_call);
        assert!(!replay.input_must_be_list);
    }

    #[test]
    fn ignores_unresumable_prev_id_errors_in_stateless_mode() {
        let replay = replay_request_error_policy(
            "prev_id cannot be resumed with current session parameters; retry without prev_id",
        );
        assert!(!replay.rebuild_stateless_on_missing_tool_call);
        assert!(!replay.input_must_be_list);
    }

    #[test]
    fn replays_stateless_rebuild_for_missing_tool_call() {
        let replay = replay_request_error_policy(
            "No tool call found for function call output in previous response",
        );
        assert!(replay.rebuild_stateless_on_missing_tool_call);
        assert!(!replay.input_must_be_list);
    }

    #[test]
    fn replays_input_must_be_list_branch() {
        let replay = replay_request_error_policy("Bad Request: input must be a list");
        assert!(replay.input_must_be_list);
        assert!(!replay.rebuild_stateless_on_missing_tool_call);
    }
}
