use serde_json::{json, Value};

pub fn extract_tool_call_id(tool_call: &Value) -> Option<&str> {
    ["id", "call_id", "tool_call_id", "toolCallId", "toolCallID"]
        .iter()
        .find_map(|key| tool_call.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn extract_tool_call_name(tool_call: &Value) -> Option<&str> {
    tool_call
        .get("function")
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .or_else(|| tool_call.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn clone_tool_call_arguments(tool_call: &Value) -> Value {
    tool_call
        .get("function")
        .and_then(|value| value.get("arguments"))
        .cloned()
        .or_else(|| tool_call.get("arguments").cloned())
        .unwrap_or_else(|| Value::String("{}".to_string()))
}

pub fn build_function_call_output_item(call_id: &str, output: &str) -> Value {
    json!({
        "type": "function_call_output",
        "call_id": call_id,
        "output": output
    })
}
