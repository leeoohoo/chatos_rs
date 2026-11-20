// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageSnapshot {
    pub input_tokens: i64,
    pub cached_tokens: i64,
    pub output_tokens: i64,
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::extract_usage_snapshot;

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
