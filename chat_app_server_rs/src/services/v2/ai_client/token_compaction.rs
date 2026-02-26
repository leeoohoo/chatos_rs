use serde_json::Value;

use crate::services::summary::token_budget::{
    estimate_message_tokens, estimate_tokens_value, is_context_overflow_error,
    token_budget_from_context_overflow_error, truncate_message_content,
};

pub(super) fn estimate_delta_stats(messages: &[Value]) -> (i64, i64) {
    let mut tokens = 0i64;
    let mut count = 0i64;

    for message in messages {
        let role = message
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if role != "system" && role != "user" {
            count += 1;
        }

        let content = message.get("content").unwrap_or(&Value::Null);
        tokens += estimate_tokens_value(content);
    }

    (tokens, count)
}

pub(super) fn is_token_limit_error(err: &str) -> bool {
    is_context_overflow_error(err)
}

pub(super) fn token_limit_budget_from_error(err: &str) -> Option<i64> {
    token_budget_from_context_overflow_error(err)
}

pub(super) fn truncate_messages_by_tokens(
    messages: &[Value],
    max_tokens: i64,
) -> (Vec<Value>, bool) {
    if max_tokens <= 0 || messages.is_empty() {
        return (messages.to_vec(), false);
    }

    let mut system_prefix = Vec::new();
    let mut index = 0usize;
    while index < messages.len() {
        if messages[index].get("role").and_then(|value| value.as_str()) == Some("system") {
            system_prefix.push(messages[index].clone());
            index += 1;
            continue;
        }
        break;
    }

    let mut tokens: i64 = system_prefix.iter().map(estimate_message_tokens).sum();
    if tokens >= max_tokens {
        let mut output = Vec::new();
        let mut remaining = max_tokens;
        for message in system_prefix {
            if remaining <= 0 {
                break;
            }
            let message_tokens = estimate_message_tokens(&message);
            if message_tokens <= remaining {
                output.push(message);
                remaining -= message_tokens;
            } else {
                output.push(truncate_message_content(&message, remaining));
                break;
            }
        }
        return (output, true);
    }

    let mut tail_reversed: Vec<Value> = Vec::new();
    for message in messages[index..].iter().rev() {
        let token_count = estimate_message_tokens(message);
        if tokens + token_count > max_tokens {
            if tail_reversed.is_empty() {
                let remaining = max_tokens - tokens;
                if remaining > 0 {
                    tail_reversed.push(truncate_message_content(message, remaining));
                }
            }
            break;
        }

        tokens += token_count;
        tail_reversed.push(message.clone());
    }
    tail_reversed.reverse();

    let mut output = system_prefix;
    output.extend(tail_reversed);
    let changed = output.len() < messages.len();
    (output, changed)
}
