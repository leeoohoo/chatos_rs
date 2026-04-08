use serde_json::Value;

fn is_tool_message(message: &Value) -> bool {
    message.get("role").and_then(|v| v.as_str()) == Some("tool")
}

fn is_assistant_tool_call(message: &Value) -> bool {
    if message.get("role").and_then(|v| v.as_str()) != Some("assistant") {
        return false;
    }
    message
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .map(|calls| !calls.is_empty())
        .unwrap_or(false)
}

fn is_valid_split(messages: &[Value], index: usize) -> bool {
    if index == 0 || index >= messages.len() {
        return false;
    }

    let left = &messages[index - 1];
    let right = &messages[index];

    if is_assistant_tool_call(left) {
        return false;
    }
    if is_tool_message(right) {
        return false;
    }

    true
}

fn score_split(index: usize, midpoint: usize) -> usize {
    index.abs_diff(midpoint)
}

pub fn split_for_summary(
    messages: &[Value],
    min_chunk_messages: usize,
) -> Option<(Vec<Value>, Vec<Value>)> {
    if messages.len() < 2 {
        return None;
    }

    let midpoint = messages.len() / 2;
    let min_chunk = min_chunk_messages.min(messages.len() / 2);

    let mut preferred = Vec::new();
    let mut fallback = Vec::new();

    for index in 1..messages.len() {
        if !is_valid_split(messages, index) {
            continue;
        }

        let left_len = index;
        let right_len = messages.len() - index;
        let balanced = left_len >= min_chunk && right_len >= min_chunk;
        if balanced {
            preferred.push(index);
        } else {
            fallback.push(index);
        }
    }

    let choose = |candidates: &[usize]| {
        candidates
            .iter()
            .min_by_key(|idx| score_split(**idx, midpoint))
            .copied()
    };

    let split_idx = choose(&preferred).or_else(|| choose(&fallback))?;

    let left = messages[..split_idx].to_vec();
    let right = messages[split_idx..].to_vec();

    if left.is_empty() || right.is_empty() {
        return None;
    }

    Some((left, right))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::split_for_summary;

    #[test]
    fn avoids_splitting_between_tool_call_and_tool_output() {
        let messages = vec![
            json!({"role": "user", "content": "q1"}),
            json!({"role": "assistant", "content": "", "tool_calls": [{"id": "call_1"}]}),
            json!({"role": "tool", "tool_call_id": "call_1", "content": "tool out"}),
            json!({"role": "assistant", "content": "a1"}),
            json!({"role": "user", "content": "q2"}),
        ];

        let (left, right) = split_for_summary(&messages, 2).expect("should split");
        assert_ne!(
            left.last()
                .and_then(|m| m.get("role").and_then(|v| v.as_str())),
            Some("assistant")
        );
        assert_ne!(
            right
                .first()
                .and_then(|m| m.get("role").and_then(|v| v.as_str())),
            Some("tool")
        );
    }

    #[test]
    fn returns_none_when_messages_too_small() {
        let messages = vec![json!({"role": "user", "content": "q"})];
        assert!(split_for_summary(&messages, 2).is_none());
    }

    #[test]
    fn keeps_both_sides_non_empty() {
        let messages = vec![
            json!({"role": "user", "content": "a"}),
            json!({"role": "assistant", "content": "b"}),
            json!({"role": "user", "content": "c"}),
        ];

        let (left, right) = split_for_summary(&messages, 1).expect("should split");
        assert!(!left.is_empty());
        assert!(!right.is_empty());
    }
}
