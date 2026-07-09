// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

#[derive(Debug)]
pub(super) struct TextEditResult {
    pub(super) content: String,
    pub(super) info: Value,
}

pub(super) fn apply_text_edit(
    content: &str,
    args: &Value,
    old_text: &str,
    new_text: &str,
) -> Result<TextEditResult, String> {
    let start_line = args
        .get("start_line")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let end_line = args
        .get("end_line")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let before_context = args.get("before_context").and_then(Value::as_str);
    let after_context = args.get("after_context").and_then(Value::as_str);
    let expected_matches = args
        .get("expected_matches")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let mut matches = Vec::new();
    for (start, _) in content.match_indices(old_text) {
        let end = start + old_text.len();
        if let Some(min_line) = start_line {
            if byte_line_number(content, start) < min_line {
                continue;
            }
        }
        if let Some(max_line) = end_line {
            if byte_line_number(content, end) > max_line {
                continue;
            }
        }
        if let Some(before) = before_context {
            if !content[..start].ends_with(before) {
                continue;
            }
        }
        if let Some(after) = after_context {
            if !content[end..].starts_with(after) {
                continue;
            }
        }
        matches.push((start, end));
    }
    if let Some(expected) = expected_matches {
        if matches.len() != expected {
            return Err(format!(
                "expected_matches mismatch: expected {expected}, found {}",
                matches.len()
            ));
        }
    }
    if matches.is_empty() {
        return Err("old_text not found in file.".to_string());
    }
    if matches.len() > 1 {
        return Err(format!(
            "old_text matched {} locations; provide before_context/after_context or start_line/end_line",
            matches.len()
        ));
    }
    let (start, end) = matches[0];
    let mut next = String::with_capacity(content.len() - old_text.len() + new_text.len());
    next.push_str(&content[..start]);
    next.push_str(new_text);
    next.push_str(&content[end..]);
    Ok(TextEditResult {
        content: next,
        info: json!({
            "replacements": 1,
            "start_line": byte_line_number(content, start),
            "end_line": byte_line_number(content, end),
            "old_text_bytes": old_text.len(),
            "new_text_bytes": new_text.len()
        }),
    })
}

fn byte_line_number(content: &str, byte_idx: usize) -> usize {
    content[..byte_idx.min(content.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_requires_unique_match() {
        let args = json!({
            "old_text": "hello",
            "new_text": "hi"
        });
        let err = apply_text_edit("hello\nhello\n", &args, "hello", "hi").unwrap_err();
        assert!(err.contains("matched 2 locations"));
    }
}
