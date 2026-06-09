#[cfg(test)]
use serde_json::Value;

#[allow(unused_imports)]
pub use chatos_ai_runtime::response_parse::join_stream_text;
#[allow(unused_imports)]
pub use chatos_ai_runtime::tool_call::{
    build_function_call_item, build_function_call_output_item, build_function_tool_call,
    clone_tool_call_arguments, collect_ordered_tool_calls, extract_message_tool_calls,
    extract_tool_call_id, extract_tool_call_name, merge_indexed_tool_call_parts,
    merge_tool_call_arguments_piece, merge_tool_call_name_piece, parse_tool_calls_value,
    remember_tool_call_index, resolve_tool_call_index, tool_call_arguments_text,
    tool_calls_value_has_items,
};

#[cfg(test)]
pub fn extract_message_tool_calls_from_value(message: &Value) -> Vec<Value> {
    extract_message_tool_calls(message.get("tool_calls"), message.get("metadata"))
}

#[cfg(test)]
pub fn message_has_tool_calls(message: &Value) -> bool {
    !extract_message_tool_calls_from_value(message).is_empty()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{
        build_function_call_item, build_function_call_output_item, build_function_tool_call,
        clone_tool_call_arguments, collect_ordered_tool_calls, extract_message_tool_calls,
        extract_message_tool_calls_from_value, extract_tool_call_id, extract_tool_call_name,
        join_stream_text, merge_indexed_tool_call_parts, merge_tool_call_arguments_piece,
        merge_tool_call_name_piece, message_has_tool_calls, parse_tool_calls_value,
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

        let message = json!({
            "role": "assistant",
            "tool_calls": raw
        });
        assert!(message_has_tool_calls(&message));
        assert_eq!(extract_message_tool_calls_from_value(&message).len(), 1);
        assert!(tool_calls_value_has_items(message.get("tool_calls")));
        assert!(!tool_calls_value_has_items(Some(&json!([]))));
        assert!(tool_calls_value_has_items(Some(&json!({"id": "call_1"}))));
        assert!(!tool_calls_value_has_items(None));
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
        assert_eq!(
            join_stream_text("你好世界ABCD", "好世界ABCD123"),
            "你好世界ABCD123"
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
}
