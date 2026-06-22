use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::response_parse::join_stream_text;

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

pub fn tool_call_arguments_text(tool_call: &Value) -> String {
    let arguments = clone_tool_call_arguments(tool_call);
    arguments
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| arguments.to_string())
}

pub fn build_function_tool_call(call_id: &str, name: &str, arguments: &str) -> Value {
    json!({
        "id": call_id,
        "type": "function",
        "function": {
            "name": name,
            "arguments": arguments
        }
    })
}

pub fn build_function_call_item(call_id: &str, name: &str, arguments: &str) -> Value {
    json!({
        "type": "function_call",
        "call_id": call_id,
        "name": name,
        "arguments": arguments
    })
}

pub fn build_function_call_output_item(call_id: &str, output: &str) -> Value {
    json!({
        "type": "function_call_output",
        "call_id": call_id,
        "output": output
    })
}

pub fn merge_tool_call_name_piece(entry: &mut Value, name: &str) {
    if name.trim().is_empty() {
        return;
    }
    let current = entry["function"]["name"].as_str().unwrap_or("").to_string();
    entry["function"]["name"] = Value::String(join_stream_text(current.as_str(), name));
}

pub fn merge_tool_call_arguments_piece(entry: &mut Value, arguments_piece: &str) {
    if arguments_piece.is_empty() {
        return;
    }
    let current = entry["function"]["arguments"]
        .as_str()
        .unwrap_or("")
        .to_string();
    entry["function"]["arguments"] =
        Value::String(join_stream_text(current.as_str(), arguments_piece));
}

pub fn merge_indexed_tool_call_parts(
    tool_calls_map: &mut BTreeMap<usize, Value>,
    index: usize,
    id: Option<&str>,
    call_id: Option<&str>,
    name_piece: Option<&str>,
    arguments_piece: Option<&str>,
) {
    let entry = tool_calls_map.entry(index).or_insert_with(
        || json!({"id":"","type":"function","function":{"name":"","arguments":""}}),
    );

    let preferred_call_id = call_id.map(str::trim).filter(|value| !value.is_empty());
    let fallback_item_id = id.map(str::trim).filter(|value| !value.is_empty());
    let current_id = entry
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if let Some(value) = preferred_call_id {
        entry["id"] = Value::String(value.to_string());
    } else if current_id.is_empty() {
        if let Some(value) = fallback_item_id {
            entry["id"] = Value::String(value.to_string());
        }
    }

    if let Some(name) = name_piece {
        merge_tool_call_name_piece(entry, name);
    }
    if let Some(arguments) = arguments_piece {
        merge_tool_call_arguments_piece(entry, arguments);
    }
}

pub fn remember_tool_call_index(
    index_map: &mut BTreeMap<String, usize>,
    index: usize,
    id: Option<&str>,
    call_id: Option<&str>,
) {
    if let Some(value) = id {
        let key = value.trim();
        if !key.is_empty() {
            index_map.insert(key.to_string(), index);
        }
    }
    if let Some(value) = call_id {
        let key = value.trim();
        if !key.is_empty() {
            index_map.insert(key.to_string(), index);
        }
    }
}

pub fn resolve_tool_call_index(
    event: &Value,
    item: Option<&Value>,
    index_map: &BTreeMap<String, usize>,
) -> Option<usize> {
    let event_index = event
        .get("output_index")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    if event_index.is_some() {
        return event_index;
    }

    let item_index = item
        .and_then(|inner| inner.get("output_index"))
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    if item_index.is_some() {
        return item_index;
    }

    let event_item_id = event
        .get("item_id")
        .or_else(|| event.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(value) = event_item_id {
        if let Some(index) = index_map.get(value) {
            return Some(*index);
        }
    }

    let event_call_id = event
        .get("call_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(value) = event_call_id {
        if let Some(index) = index_map.get(value) {
            return Some(*index);
        }
    }

    let item_id = item
        .and_then(|inner| inner.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(value) = item_id {
        if let Some(index) = index_map.get(value) {
            return Some(*index);
        }
    }

    let item_call_id = item
        .and_then(|inner| inner.get("call_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(value) = item_call_id {
        if let Some(index) = index_map.get(value) {
            return Some(*index);
        }
    }

    None
}

pub fn collect_ordered_tool_calls(tool_calls_map: &BTreeMap<usize, Value>) -> Option<Value> {
    if tool_calls_map.is_empty() {
        None
    } else {
        Some(Value::Array(tool_calls_map.values().cloned().collect()))
    }
}

pub fn parse_tool_calls_value(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items.clone(),
        Value::Object(_) => vec![value.clone()],
        Value::String(raw) => serde_json::from_str::<Value>(raw)
            .ok()
            .map(|parsed| parse_tool_calls_value(&parsed))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

pub fn extract_message_tool_calls(
    tool_calls: Option<&Value>,
    metadata: Option<&Value>,
) -> Vec<Value> {
    if let Some(tool_calls) = tool_calls {
        let parsed = parse_tool_calls_value(tool_calls);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    if let Some(Value::Object(map)) = metadata {
        if let Some(value) = map.get("toolCalls").or_else(|| map.get("tool_calls")) {
            let parsed = parse_tool_calls_value(value);
            if !parsed.is_empty() {
                return parsed;
            }
        }
    }

    Vec::new()
}

pub fn tool_calls_value_has_items(tool_calls: Option<&Value>) -> bool {
    tool_calls
        .map(parse_tool_calls_value)
        .map(|items| !items.is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{
        build_function_call_item, build_function_call_output_item, build_function_tool_call,
        clone_tool_call_arguments, collect_ordered_tool_calls, extract_message_tool_calls,
        extract_tool_call_id, extract_tool_call_name, merge_indexed_tool_call_parts,
        merge_tool_call_arguments_piece, merge_tool_call_name_piece, parse_tool_calls_value,
        remember_tool_call_index, resolve_tool_call_index, tool_call_arguments_text,
        tool_calls_value_has_items,
    };

    #[test]
    fn extracts_tool_call_fields_across_supported_shapes() {
        let tool_call = json!({
            "call_id": "call_1",
            "name": "demo.search",
            "arguments": {"q": "rust"}
        });

        assert_eq!(extract_tool_call_id(&tool_call), Some("call_1"));
        assert_eq!(extract_tool_call_name(&tool_call), Some("demo.search"));
        assert_eq!(clone_tool_call_arguments(&tool_call), json!({"q": "rust"}));
        assert_eq!(tool_call_arguments_text(&tool_call), "{\"q\":\"rust\"}");
    }

    #[test]
    fn builds_function_call_payloads_in_canonical_shapes() {
        assert_eq!(
            build_function_tool_call("call_1", "demo.search", "{}"),
            json!({
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "demo.search",
                    "arguments": "{}"
                }
            })
        );
        assert_eq!(
            build_function_call_item("call_1", "demo.search", "{}"),
            json!({
                "type": "function_call",
                "call_id": "call_1",
                "name": "demo.search",
                "arguments": "{}"
            })
        );
        assert_eq!(
            build_function_call_output_item("call_1", "done"),
            json!({
                "type": "function_call_output",
                "call_id": "call_1",
                "output": "done"
            })
        );
    }

    #[test]
    fn parses_tool_calls_from_string_or_metadata_fallback() {
        let raw =
            json!("[{\"id\":\"call_1\",\"function\":{\"name\":\"demo\",\"arguments\":\"{}\"}}]");
        let parsed = parse_tool_calls_value(&raw);
        assert_eq!(parsed.len(), 1);
        assert_eq!(extract_tool_call_id(&parsed[0]), Some("call_1"));

        let metadata = json!({
            "toolCalls": raw
        });
        let extracted = extract_message_tool_calls(None, Some(&metadata));
        assert_eq!(extracted.len(), 1);
        assert_eq!(extract_tool_call_name(&extracted[0]), Some("demo"));

        assert!(tool_calls_value_has_items(Some(&json!({"id": "call_1"}))));
        assert!(!tool_calls_value_has_items(Some(&json!([]))));
    }

    #[test]
    fn collects_ordered_tool_calls_and_merges_stream_pieces() {
        let mut map = BTreeMap::new();
        map.insert(3, json!({"id": "call_3"}));
        map.insert(1, json!({"id": "call_1"}));
        let ordered = collect_ordered_tool_calls(&map)
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        assert_eq!(ordered.len(), 2);
        assert_eq!(extract_tool_call_id(&ordered[0]), Some("call_1"));
        assert_eq!(extract_tool_call_id(&ordered[1]), Some("call_3"));

        let mut entry = build_function_tool_call("call_1", "search", "{\"q\":");
        merge_tool_call_name_piece(&mut entry, "_docs");
        merge_tool_call_arguments_piece(&mut entry, "\"rust\"}");
        assert_eq!(entry["function"]["name"].as_str(), Some("search_docs"));
        assert_eq!(
            entry["function"]["arguments"].as_str(),
            Some("{\"q\":\"rust\"}")
        );
    }

    #[test]
    fn resolves_tool_call_index_from_cached_aliases() {
        let mut index_map = BTreeMap::new();
        remember_tool_call_index(&mut index_map, 7, Some("item_7"), Some("call_7"));

        assert_eq!(
            resolve_tool_call_index(&json!({"item_id": "item_7"}), None, &index_map),
            Some(7)
        );
        assert_eq!(
            resolve_tool_call_index(&json!({"call_id": "call_7"}), None, &index_map),
            Some(7)
        );
        assert_eq!(
            resolve_tool_call_index(&json!({}), Some(&json!({"call_id": "call_7"})), &index_map),
            Some(7)
        );
    }

    #[test]
    fn merges_indexed_tool_call_parts_and_prefers_call_id() {
        let mut map = BTreeMap::new();
        merge_indexed_tool_call_parts(
            &mut map,
            0,
            Some("item_1"),
            None,
            Some("search"),
            Some("{\"q\":"),
        );
        merge_indexed_tool_call_parts(
            &mut map,
            0,
            Some("item_1"),
            Some("call_1"),
            Some("_docs"),
            Some("\"rust\"}"),
        );

        let ordered = collect_ordered_tool_calls(&map)
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        assert_eq!(ordered.len(), 1);
        assert_eq!(extract_tool_call_id(&ordered[0]), Some("call_1"));
        assert_eq!(ordered[0]["function"]["name"].as_str(), Some("search_docs"));
        assert_eq!(
            ordered[0]["function"]["arguments"].as_str(),
            Some("{\"q\":\"rust\"}")
        );
    }
}
