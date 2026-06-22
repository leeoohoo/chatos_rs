use std::collections::HashSet;

use serde_json::Value;

use crate::input_transform::{to_message_item, to_message_item_with_reasoning};
use crate::tool_call::{
    build_function_call_item, build_function_call_output_item, extract_message_tool_calls,
    extract_tool_call_id, extract_tool_call_name, tool_call_arguments_text,
};

#[derive(Debug, Clone, Default)]
pub struct StatelessHistoryMessage {
    pub role: String,
    pub content: String,
    pub reasoning: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub metadata: Option<Value>,
    pub skip_in_input: bool,
}

pub fn build_stateless_history_items(
    leading_prefixed_input_items: &[Value],
    trailing_prefixed_input_items: &[Value],
    summary_text: Option<&str>,
    history: &[StatelessHistoryMessage],
    current_input_items: &[Value],
    include_tool_items: bool,
    force_text: bool,
) -> Vec<Value> {
    build_stateless_history_items_with_output_cap(
        leading_prefixed_input_items,
        trailing_prefixed_input_items,
        summary_text,
        history,
        current_input_items,
        include_tool_items,
        force_text,
        |raw| raw.to_string(),
    )
}

pub fn build_stateless_history_items_with_output_cap<F>(
    leading_prefixed_input_items: &[Value],
    trailing_prefixed_input_items: &[Value],
    summary_text: Option<&str>,
    history: &[StatelessHistoryMessage],
    current_input_items: &[Value],
    include_tool_items: bool,
    force_text: bool,
    cap_tool_output: F,
) -> Vec<Value>
where
    F: Fn(&str) -> String,
{
    let mut items = Vec::new();
    let mut tool_call_ids: HashSet<String> = HashSet::new();
    let mut tool_output_ids: HashSet<String> = HashSet::new();

    items.extend(leading_prefixed_input_items.iter().cloned());

    if let Some(summary_text) = summary_text
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        items.push(to_message_item(
            "system",
            &Value::String(summary_text.to_string()),
            force_text,
        ));
    }

    if include_tool_items {
        for msg in history {
            if msg.role == "tool" {
                if let Some(call_id) = msg
                    .tool_call_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    tool_output_ids.insert(call_id.to_string());
                }
            }
        }
    }

    for msg in history {
        if msg.skip_in_input {
            continue;
        }

        if matches!(
            msg.role.as_str(),
            "user" | "assistant" | "system" | "developer"
        ) {
            let content = Value::String(msg.content.clone());
            let message_item = if msg.role == "assistant" {
                to_message_item_with_reasoning(
                    msg.role.as_str(),
                    &content,
                    msg.reasoning.as_deref(),
                    force_text,
                )
            } else {
                to_message_item(msg.role.as_str(), &content, force_text)
            };
            items.push(message_item);

            if include_tool_items && msg.role == "assistant" {
                for tc in extract_message_tool_calls(msg.tool_calls.as_ref(), msg.metadata.as_ref())
                {
                    let call_id = extract_tool_call_id(&tc).unwrap_or("").to_string();
                    if call_id.is_empty() || !tool_output_ids.contains(&call_id) {
                        continue;
                    }
                    let name = extract_tool_call_name(&tc).unwrap_or("").to_string();
                    let args_str = tool_call_arguments_text(&tc);
                    tool_call_ids.insert(call_id.clone());
                    items.push(build_function_call_item(
                        call_id.as_str(),
                        name.as_str(),
                        args_str.as_str(),
                    ));
                }
            }
            continue;
        }

        if msg.role == "tool" && include_tool_items {
            if let Some(call_id) = msg
                .tool_call_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                if tool_call_ids.contains(call_id) {
                    let output = cap_tool_output(msg.content.as_str());
                    items.push(build_function_call_output_item(call_id, output.as_str()));
                }
            }
        }
    }

    splice_current_input_items(&mut items, current_input_items);
    items.extend(trailing_prefixed_input_items.iter().cloned());
    items
}

pub fn splice_current_input_items(items: &mut Vec<Value>, current_input_items: &[Value]) {
    if current_input_items.is_empty() {
        return;
    }

    if let Some(index) = items.iter().rposition(|item| {
        item.get("type").and_then(|value| value.as_str()) == Some("message")
            && item.get("role").and_then(|value| value.as_str()) == Some("user")
    }) {
        items.splice(index..=index, current_input_items.iter().cloned());
        return;
    }

    items.extend_from_slice(current_input_items);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        build_stateless_history_items_with_output_cap, splice_current_input_items,
        StatelessHistoryMessage,
    };

    #[test]
    fn splice_current_input_items_replaces_latest_user_in_place() {
        let mut items = vec![
            json!({
                "type": "message",
                "role": "system",
                "content": [{"type": "input_text", "text": "[summary]"}]
            }),
            json!({
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "old user"}]
            }),
            json!({
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "calling tool"}]
            }),
            json!({
                "type": "function_call",
                "call_id": "call_1"
            }),
            json!({
                "type": "function_call_output",
                "call_id": "call_1"
            }),
        ];

        splice_current_input_items(
            &mut items,
            &[json!({
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "new user"}]
            })],
        );

        assert_eq!(
            items,
            vec![
                json!({
                    "type": "message",
                    "role": "system",
                    "content": [{"type": "input_text", "text": "[summary]"}]
                }),
                json!({
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "new user"}]
                }),
                json!({
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "calling tool"}]
                }),
                json!({
                    "type": "function_call",
                    "call_id": "call_1"
                }),
                json!({
                    "type": "function_call_output",
                    "call_id": "call_1"
                }),
            ]
        );
    }

    #[test]
    fn splice_current_input_items_appends_when_history_has_no_user() {
        let mut items = vec![json!({
            "type": "message",
            "role": "system",
            "content": [{"type": "input_text", "text": "[summary]"}]
        })];

        splice_current_input_items(
            &mut items,
            &[json!({
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "hello"}]
            })],
        );

        assert_eq!(items.len(), 2);
        assert_eq!(
            items[1].get("role").and_then(|value| value.as_str()),
            Some("user")
        );
    }

    #[test]
    fn build_stateless_history_items_reconstructs_tool_exchange() {
        let history = vec![
            StatelessHistoryMessage {
                role: "assistant".to_string(),
                content: "calling tool".to_string(),
                tool_calls: Some(json!([{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "demo.search", "arguments": "{\"q\":\"rust\"}"}
                }])),
                ..StatelessHistoryMessage::default()
            },
            StatelessHistoryMessage {
                role: "tool".to_string(),
                content: "tool result long".to_string(),
                tool_call_id: Some("call_1".to_string()),
                ..StatelessHistoryMessage::default()
            },
        ];

        let items = build_stateless_history_items_with_output_cap(
            &[],
            &[],
            Some("summary"),
            &history,
            &[json!({
                "type":"message",
                "role":"user",
                "content":[{"type":"input_text","text":"hello"}]
            })],
            true,
            false,
            |_| "trimmed".to_string(),
        );

        assert!(items.iter().any(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call")
                && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
        }));
        assert!(items.iter().any(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                && item.get("output").and_then(|value| value.as_str()) == Some("trimmed")
        }));
    }
}
