use std::collections::HashSet;

use serde_json::{json, Value};

use crate::model_config::{
    is_gpt_provider, normalize_provider, reasoning_effort_for_provider, thinking_mode_for_provider,
};
use crate::response_parse::{chat_message_content_to_text, tool_arguments_to_string};

pub const CHAT_PROMPT_CACHE_RETENTION: &str = "24h";

#[allow(clippy::too_many_arguments)]
pub fn build_responses_request_payload(
    input: Value,
    model: String,
    instructions: Option<String>,
    prompt_cache_key: Option<String>,
    tools: Option<Vec<Value>>,
    request_cwd: Option<String>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    provider: Option<String>,
    thinking_level: Option<String>,
    stream: bool,
    include_prompt_cache_retention: bool,
) -> Value {
    let mut payload = json!({
        "model": model,
        "input": input,
    });
    if let Some(instructions) = normalized_option(instructions.as_deref()) {
        payload["instructions"] = Value::String(instructions);
    }
    let mut has_prompt_cache_key = false;
    if let Some(cache_key) = normalized_option(prompt_cache_key.as_deref()) {
        payload["prompt_cache_key"] = Value::String(cache_key);
        has_prompt_cache_key = true;
    }
    if has_prompt_cache_key && include_prompt_cache_retention {
        payload["prompt_cache_retention"] = Value::String(CHAT_PROMPT_CACHE_RETENTION.to_string());
    }
    if let Some(tools) = tools.filter(|items| !items.is_empty()) {
        payload["tools"] = Value::Array(tools);
        payload["tool_choice"] = Value::String("auto".to_string());
    }
    if let Some(cwd) = normalized_option(request_cwd.as_deref()) {
        payload["cwd"] = Value::String(cwd);
    }
    if let Some(value) = temperature {
        payload["temperature"] = json!(value);
    }
    if let Some(value) = max_output_tokens {
        payload["max_output_tokens"] = json!(value);
    }
    if let Some(level) =
        reasoning_effort_for_provider(provider.as_deref(), thinking_level.as_deref())
    {
        let mut reasoning_payload = json!({ "effort": level });
        if is_gpt_provider(provider.as_deref().unwrap_or("gpt")) {
            reasoning_payload["summary"] = Value::String("auto".to_string());
        }
        payload["reasoning"] = reasoning_payload;
    }
    if stream {
        payload["stream"] = Value::Bool(true);
    }
    payload
}

#[allow(clippy::too_many_arguments)]
pub fn build_chat_completions_request_payload(
    input: Value,
    model: String,
    instructions: Option<String>,
    tools: Option<Vec<Value>>,
    temperature: Option<f64>,
    max_output_tokens: Option<i64>,
    provider: Option<String>,
    thinking_level: Option<String>,
    stream: bool,
) -> Value {
    let mut messages = input_to_chat_messages(input);
    if let Some(system) = normalized_option(instructions.as_deref()) {
        messages.insert(0, json!({ "role": "system", "content": system }));
    }
    let mut payload = json!({
        "model": model,
        "messages": messages,
    });
    if let Some(tools) = tools.filter(|items| !items.is_empty()) {
        payload["tools"] = Value::Array(
            tools
                .into_iter()
                .map(chat_completion_tool_definition)
                .collect(),
        );
        payload["tool_choice"] = Value::String("auto".to_string());
    }
    if should_send_chat_completions_temperature(provider.as_deref(), thinking_level.as_deref()) {
        if let Some(value) = temperature {
            payload["temperature"] = json!(value);
        }
    }
    if let Some(value) = max_output_tokens {
        payload["max_tokens"] = json!(value);
    }
    if let Some(level) =
        reasoning_effort_for_provider(provider.as_deref(), thinking_level.as_deref())
    {
        payload["reasoning_effort"] = Value::String(level);
    }
    if let Some(mode) = thinking_mode_for_provider(provider.as_deref(), thinking_level.as_deref()) {
        payload["thinking"] = json!({ "type": mode });
    }
    if stream {
        payload["stream"] = Value::Bool(true);
        payload["stream_options"] = json!({ "include_usage": true });
    }
    payload
}

pub fn input_to_chat_messages(input: Value) -> Vec<Value> {
    match input {
        Value::Array(items) => response_items_to_chat_messages(items),
        Value::String(text) => vec![json!({ "role": "user", "content": text })],
        other => vec![json!({ "role": "user", "content": other.to_string() })],
    }
}

pub fn response_items_to_chat_messages(items: Vec<Value>) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut index = 0;

    while index < items.len() {
        let item = &items[index];
        let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");

        if item_type == "message" {
            if let Some(mut message) = response_message_item_to_chat_message(item) {
                index += 1;
                if message.get("role").and_then(Value::as_str) == Some("assistant") {
                    let mut tool_calls = Vec::new();
                    while index < items.len()
                        && items[index].get("type").and_then(Value::as_str) == Some("function_call")
                    {
                        tool_calls.push(chat_function_call_item_to_tool_call(&items[index]));
                        index += 1;
                    }
                    if !tool_calls.is_empty() {
                        message["tool_calls"] = Value::Array(tool_calls);
                    }
                }
                messages.push(message);
                continue;
            }
        }

        if item_type == "function_call" {
            let mut tool_calls = Vec::new();
            while index < items.len()
                && items[index].get("type").and_then(Value::as_str) == Some("function_call")
            {
                tool_calls.push(chat_function_call_item_to_tool_call(&items[index]));
                index += 1;
            }
            if !tool_calls.is_empty() {
                messages.push(json!({
                    "role": "assistant",
                    "content": Value::Null,
                    "tool_calls": tool_calls,
                }));
            }
            continue;
        }

        if let Some(message) = response_item_to_chat_message(item.clone()) {
            messages.push(message);
        }
        index += 1;
    }

    drop_incomplete_tool_call_messages(messages)
}

pub fn should_send_chat_completions_temperature(
    provider: Option<&str>,
    thinking_level: Option<&str>,
) -> bool {
    let provider = normalize_provider(provider.unwrap_or("gpt"));
    let thinking_mode = thinking_mode_for_provider(Some(provider.as_str()), thinking_level);
    !(provider == "deepseek" && thinking_mode == Some("enabled"))
}

pub fn normalized_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn response_message_item_to_chat_message(item: &Value) -> Option<Value> {
    let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
    let content = item
        .get("content")
        .map(chat_message_content_to_value)
        .unwrap_or_else(|| Value::String(String::new()));
    let mut message = json!({
        "role": role,
        "content": content,
    });
    if role == "assistant" {
        if let Some(reasoning_content) = chat_message_reasoning_content(item) {
            message["reasoning_content"] = Value::String(reasoning_content);
        }
    }
    Some(message)
}

fn response_item_to_chat_message(item: Value) -> Option<Value> {
    match item.get("type").and_then(Value::as_str).unwrap_or("") {
        "message" => response_message_item_to_chat_message(&item),
        "function_call" => Some(json!({
            "role": "assistant",
            "content": Value::Null,
            "tool_calls": [chat_function_call_item_to_tool_call(&item)],
        })),
        "function_call_output" => {
            let call_id = item
                .get("call_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let output = item
                .get("output")
                .map(chat_message_content_to_text)
                .unwrap_or_default();
            Some(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output,
            }))
        }
        _ => None,
    }
}

fn drop_incomplete_tool_call_messages(messages: Vec<Value>) -> Vec<Value> {
    let mut output = Vec::with_capacity(messages.len());
    let mut index = 0;

    while index < messages.len() {
        let message = &messages[index];
        let tool_call_ids: Vec<String> = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("id").and_then(Value::as_str))
                    .filter(|id| !id.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();

        if tool_call_ids.is_empty() {
            output.push(message.clone());
            index += 1;
            continue;
        }

        let mut scan = index + 1;
        let mut seen_tool_ids = HashSet::new();
        while scan < messages.len()
            && messages[scan].get("role").and_then(Value::as_str) == Some("tool")
        {
            if let Some(id) = messages[scan]
                .get("tool_call_id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
            {
                seen_tool_ids.insert(id.to_string());
            }
            scan += 1;
        }

        if tool_call_ids.iter().all(|id| seen_tool_ids.contains(id)) {
            output.push(message.clone());
            for tool_message in messages.iter().take(scan).skip(index + 1) {
                output.push(tool_message.clone());
            }
        }

        index = scan;
    }

    output
}

fn chat_function_call_item_to_tool_call(item: &Value) -> Value {
    let call_id = item
        .get("call_id")
        .and_then(Value::as_str)
        .or_else(|| item.get("id").and_then(Value::as_str))
        .unwrap_or("");
    let name = item.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = item
        .get("arguments")
        .map(tool_arguments_to_string)
        .unwrap_or_else(|| "{}".to_string());
    json!({
        "id": call_id,
        "type": "function",
        "function": {
            "name": name,
            "arguments": arguments,
        }
    })
}

fn chat_completion_tool_definition(tool: Value) -> Value {
    if tool.get("function").and_then(Value::as_object).is_some() {
        return tool;
    }

    let name = tool
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let parameters = tool
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| json!({"type":"object","properties":{}}));

    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters,
        }
    })
}

fn chat_message_reasoning_content(item: &Value) -> Option<String> {
    item.get("reasoning_content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            item.get("reasoning")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            let parts = item.get("content")?.as_array()?;
            let chunks = parts
                .iter()
                .filter(|part| {
                    matches!(
                        part.get("type").and_then(Value::as_str),
                        Some("reasoning") | Some("reasoning_content")
                    )
                })
                .map(|part| {
                    part.get("text")
                        .or_else(|| part.get("content"))
                        .or_else(|| part.get("reasoning"))
                        .map(chat_message_content_to_text)
                        .unwrap_or_else(|| chat_message_content_to_text(part))
                })
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>();
            if chunks.is_empty() {
                None
            } else {
                Some(chunks.join(""))
            }
        })
}

fn chat_message_content_to_value(content: &Value) -> Value {
    match content {
        Value::String(text) => Value::String(text.clone()),
        Value::Array(parts) => {
            let normalized = parts
                .iter()
                .filter_map(chat_content_part_to_value)
                .collect::<Vec<_>>();
            if normalized.is_empty() {
                Value::String(chat_message_content_to_text(content))
            } else {
                Value::Array(normalized)
            }
        }
        other => Value::String(chat_message_content_to_text(other)),
    }
}

fn chat_content_part_to_value(part: &Value) -> Option<Value> {
    let part_type = part.get("type").and_then(Value::as_str).unwrap_or("");
    match part_type {
        "input_text" | "output_text" | "text" => Some(json!({
            "type": "text",
            "text": part
                .get("text")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| chat_message_content_to_text(part)),
        })),
        "input_image" | "image_url" => {
            let image_url = part
                .get("image_url")
                .and_then(|value| {
                    value.as_str().map(ToOwned::to_owned).or_else(|| {
                        value
                            .get("url")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                    })
                })
                .unwrap_or_default();
            if image_url.is_empty() {
                None
            } else {
                Some(json!({
                    "type": "image_url",
                    "image_url": {
                        "url": image_url,
                        "detail": part.get("detail").cloned().unwrap_or(Value::String("auto".to_string())),
                    }
                }))
            }
        }
        "reasoning" | "reasoning_content" => None,
        _ => {
            let text = chat_message_content_to_text(part);
            if text.is_empty() {
                None
            } else {
                Some(json!({
                    "type": "text",
                    "text": text,
                }))
            }
        }
    }
}
