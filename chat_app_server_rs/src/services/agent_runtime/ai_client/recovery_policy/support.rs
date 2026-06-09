use serde_json::Value;

use chatos_ai_runtime::merge_pending_tool_turn_items as merge_shared_pending_tool_turn_items;
pub(super) use chatos_ai_runtime::replay_request_error_policy;

pub(super) fn merge_pending_tool_items_into_stateless(
    stateless: &mut Vec<Value>,
    pending_tool_calls: Option<&Vec<Value>>,
    pending_tool_outputs: Option<&Vec<Value>>,
) {
    let pending_tool_calls = pending_tool_calls.map(|items| items.as_slice());
    let pending_tool_outputs = pending_tool_outputs.map(|items| items.as_slice());
    merge_shared_pending_tool_turn_items(stateless, pending_tool_calls, pending_tool_outputs);
}
