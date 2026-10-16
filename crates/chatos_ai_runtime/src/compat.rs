// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use tracing::info;

use crate::input_transform::{content_parts_to_text, to_message_item};
use crate::response_parse::text_value_or_json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageSnapshot {
    pub input_tokens: i64,
    pub cached_tokens: i64,
    pub output_tokens: i64,
}

pub fn cap_tool_output_for_input(raw: &str) -> String {
    let budget = tool_output_item_max_chars();
    chatos_mcp_runtime::capability_description_budget_error(raw, budget)
        .unwrap_or_else(|| tool_output_budget_error(raw, budget))
}

#[cfg(test)]
mod capability_budget_tests {
    use super::*;

    #[test]
    fn final_request_budget_does_not_cut_tool_schemas_into_head_and_tail() {
        let page = serde_json::json!({"content":{"disclosure_version":2,"binding_schema_version":1,
            "plugin_option":"plugin_1","view":"tool","tools":[{"name":"browser_navigate",
                "input_schema":{"type":"object","description":"x".repeat(7_000)}}]}})
        .to_string();
        assert_eq!(cap_tool_output_for_input(&page), page);
        let oversized = page.replace(&"x".repeat(7_000), &"x".repeat(170_000));
        let input = serde_json::json!([{"type":"function_call_output","call_id":"describe","output":oversized}]);
        let transformed = truncate_function_call_outputs_in_input(&input).unwrap();
        let output: Value =
            serde_json::from_str(transformed[0]["output"].as_str().unwrap()).unwrap();
        assert_eq!(output["error"], "capability_description_budget_exceeded");
        assert!(!transformed[0]["output"]
            .as_str()
            .unwrap()
            .contains("...truncated"));
    }
}

pub fn truncate_function_call_outputs_in_input(input: &Value) -> Option<Value> {
    let items = input.as_array()?;
    let mut changed = false;
    let mut mapped = Vec::with_capacity(items.len());

    for item in items {
        let mut cloned = item.clone();
        let is_output_item =
            cloned.get("type").and_then(|value| value.as_str()) == Some("function_call_output");
        if is_output_item {
            if let Some(object) = cloned.as_object_mut() {
                if let Some(raw) = object
                    .get("output")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
                {
                    let truncated = cap_tool_output_for_input(raw.as_str());
                    if truncated != raw {
                        object.insert("output".to_string(), Value::String(truncated));
                        changed = true;
                    }
                }
            }
        }
        mapped.push(cloned);
    }

    if changed {
        Some(Value::Array(mapped))
    } else {
        None
    }
}

pub fn extract_usage_snapshot(usage: &Value) -> UsageSnapshot {
    let input_tokens = usage_value_i64(usage, "input_tokens")
        .or_else(|| usage_value_i64(usage, "prompt_tokens"))
        .unwrap_or(-1);
    let output_tokens = usage_value_i64(usage, "output_tokens")
        .or_else(|| usage_value_i64(usage, "completion_tokens"))
        .unwrap_or(-1);
    let cached_tokens = usage_nested_i64(usage, "input_tokens_details", "cached_tokens")
        .or_else(|| usage_nested_i64(usage, "prompt_tokens_details", "cached_tokens"))
        .unwrap_or(0);

    UsageSnapshot {
        input_tokens,
        cached_tokens,
        output_tokens,
    }
}

pub fn log_usage_snapshot(purpose: &str, usage: Option<&Value>) {
    let Some(usage) = usage else {
        return;
    };
    let snapshot = extract_usage_snapshot(usage);

    info!(
        "[Agent Runtime] usage snapshot: purpose={}, input_tokens={}, cached_tokens={}, output_tokens={}",
        purpose, snapshot.input_tokens, snapshot.cached_tokens, snapshot.output_tokens
    );
}

pub fn rewrite_system_messages_to_user(input: &Value, force_text_content: bool) -> Value {
    let Some(items) = input.as_array() else {
        return input.clone();
    };

    let mut changed = false;
    let mut mapped = Vec::with_capacity(items.len());

    for item in items {
        let item_type = item
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let role = item
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        if item_type == "message" && (role == "system" || role == "developer") {
            let content = response_content_to_text(item.get("content").unwrap_or(&Value::Null));
            let label = if role == "developer" {
                "开发者上下文"
            } else {
                "系统上下文"
            };
            let wrapped = if content.trim().is_empty() {
                format!("【{label}】")
            } else {
                format!("【{label}】\n{content}")
            };
            mapped.push(to_message_item(
                "user",
                &Value::String(wrapped),
                force_text_content,
            ));
            changed = true;
            continue;
        }

        mapped.push(item.clone());
    }

    if changed {
        Value::Array(mapped)
    } else {
        input.clone()
    }
}

fn tool_output_item_max_chars() -> usize {
    std::env::var("AI_AGENT_TOOL_OUTPUT_MAX_CHARS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(160_000)
}

fn tool_output_budget_error(raw: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let total = raw.chars().count();
    if total <= max_chars {
        return raw.to_string();
    }

    serde_json::json!({
        "error": "tool_result_budget_exceeded",
        "original_chars": total,
        "budget_chars": max_chars,
        "content_preserved": false,
        "guidance": "No partial head/tail excerpt was supplied. Use explicit pagination, a managed artifact, or a durable tool_result_read reference."
    })
    .to_string()
}

fn usage_value_i64(value: &Value, key: &str) -> Option<i64> {
    value
        .get(key)
        .and_then(|item| item.as_i64().or_else(|| item.as_u64().map(|v| v as i64)))
}

fn usage_nested_i64(value: &Value, parent: &str, key: &str) -> Option<i64> {
    value
        .get(parent)
        .and_then(|item| item.get(key))
        .and_then(|item| item.as_i64().or_else(|| item.as_u64().map(|v| v as i64)))
}

fn response_content_to_text(content: &Value) -> String {
    if content.is_array() {
        return content_parts_to_text(content);
    }

    text_value_or_json(content, &["text", "output_text", "output"])
        .unwrap_or_else(|| content.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        extract_usage_snapshot, response_content_to_text, rewrite_system_messages_to_user,
        truncate_function_call_outputs_in_input,
    };

    #[test]
    fn rewrites_system_and_developer_messages_to_user_role() {
        let input = json!([
            {
                "type": "message",
                "role": "system",
                "content": [{"type":"input_text","text":"system prompt"}]
            },
            {
                "type": "message",
                "role": "developer",
                "content": [{"type":"input_text","text":"developer notes"}]
            },
            {
                "type": "message",
                "role": "user",
                "content": [{"type":"input_text","text":"hello"}]
            }
        ]);

        let output = rewrite_system_messages_to_user(&input, false);
        let arr = output.as_array().expect("array output");
        assert_eq!(arr.len(), 3);
        assert_eq!(
            arr[0].get("role").and_then(|value| value.as_str()),
            Some("user")
        );
        assert_eq!(
            arr[1].get("role").and_then(|value| value.as_str()),
            Some("user")
        );
        assert_eq!(
            arr[2].get("role").and_then(|value| value.as_str()),
            Some("user")
        );

        let first_text = response_content_to_text(arr[0].get("content").unwrap_or(&Value::Null));
        let second_text = response_content_to_text(arr[1].get("content").unwrap_or(&Value::Null));
        assert!(first_text.contains("系统上下文"));
        assert!(first_text.contains("system prompt"));
        assert!(second_text.contains("开发者上下文"));
        assert!(second_text.contains("developer notes"));
    }

    #[test]
    fn keeps_input_unchanged_when_no_system_messages_exist() {
        let input = json!([
            {
                "type": "message",
                "role": "user",
                "content": [{"type":"input_text","text":"hello"}]
            }
        ]);

        let output = rewrite_system_messages_to_user(&input, false);
        assert_eq!(input, output);
    }

    #[test]
    fn truncates_large_function_call_output_items() {
        let long_output = "a".repeat(170_000);
        let input = json!([
            {
                "type": "function_call_output",
                "call_id": "call_1",
                "output": long_output
            }
        ]);

        let truncated = truncate_function_call_outputs_in_input(&input).expect("should truncate");
        let items = truncated.as_array().expect("array");
        let text = items[0]
            .get("output")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        assert!(text.len() < 170_000);
        assert!(text.contains("tool_result_budget_exceeded"));
    }

    #[test]
    fn keeps_small_function_call_output_items_unchanged() {
        let input = json!([
            {
                "type": "function_call_output",
                "call_id": "call_1",
                "output": "short"
            }
        ]);

        assert!(truncate_function_call_outputs_in_input(&input).is_none());
    }

    #[test]
    fn extracts_usage_snapshot_across_responses_and_chat_shapes() {
        let responses_usage = json!({
            "input_tokens": 100,
            "output_tokens": 25,
            "input_tokens_details": { "cached_tokens": 40 }
        });
        let chat_usage = json!({
            "prompt_tokens": 120,
            "completion_tokens": 30,
            "prompt_tokens_details": { "cached_tokens": 12 }
        });

        assert_eq!(
            extract_usage_snapshot(&responses_usage),
            super::UsageSnapshot {
                input_tokens: 100,
                cached_tokens: 40,
                output_tokens: 25
            }
        );
        assert_eq!(
            extract_usage_snapshot(&chat_usage),
            super::UsageSnapshot {
                input_tokens: 120,
                cached_tokens: 12,
                output_tokens: 30
            }
        );
    }
}
