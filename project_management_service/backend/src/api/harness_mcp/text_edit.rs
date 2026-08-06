// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

#[derive(Debug)]
pub(super) struct TextEditResult {
    pub(super) content: String,
    pub(super) info: Value,
    pub(super) changed: bool,
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
            if !matches_before_context(&content[..start], before) {
                continue;
            }
        }
        if let Some(after) = after_context {
            if !matches_after_context(&content[end..], after) {
                continue;
            }
        }
        matches.push((start, end));
    }
    if matches.is_empty() {
        return already_applied_edit(
            content,
            new_text,
            start_line,
            end_line,
            before_context,
            after_context,
            expected_matches,
        );
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
            "new_text_bytes": new_text.len(),
            "already_applied": false
        }),
        changed: true,
    })
}

fn already_applied_edit(
    content: &str,
    new_text: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
    before_context: Option<&str>,
    after_context: Option<&str>,
    expected_matches: Option<usize>,
) -> Result<TextEditResult, String> {
    if new_text.is_empty() {
        return Err("old_text not found in file.".to_string());
    }
    let matches = content
        .match_indices(new_text)
        .filter_map(|(start, _)| {
            let end = start + new_text.len();
            if start_line.is_some_and(|min| byte_line_number(content, start) < min)
                || end_line.is_some_and(|max| byte_line_number(content, end) > max)
                || before_context
                    .is_some_and(|before| !matches_before_context(&content[..start], before))
                || after_context.is_some_and(|after| !matches_after_context(&content[end..], after))
            {
                return None;
            }
            Some((start, end))
        })
        .collect::<Vec<_>>();
    let expected = expected_matches.unwrap_or(1);
    if matches.len() != expected || matches.len() != 1 {
        return Err("old_text not found in file.".to_string());
    }
    let (start, end) = matches[0];
    Ok(TextEditResult {
        content: content.to_string(),
        info: json!({
            "replacements": 0,
            "start_line": byte_line_number(content, start),
            "end_line": byte_line_number(content, end),
            "old_text_bytes": 0,
            "new_text_bytes": new_text.len(),
            "already_applied": true
        }),
        changed: false,
    })
}

fn matches_before_context(prefix: &str, context: &str) -> bool {
    if prefix.ends_with(context) {
        return true;
    }
    if context.ends_with(['\n', '\r']) {
        return false;
    }

    prefix
        .strip_suffix("\r\n")
        .or_else(|| prefix.strip_suffix('\n'))
        .is_some_and(|value| value.ends_with(context))
}

fn matches_after_context(suffix: &str, context: &str) -> bool {
    if suffix.starts_with(context) {
        return true;
    }
    if context.starts_with(['\n', '\r']) {
        return false;
    }

    suffix
        .strip_prefix("\r\n")
        .or_else(|| suffix.strip_prefix('\n'))
        .is_some_and(|value| value.starts_with(context))
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

    #[test]
    fn repeated_edit_is_a_successful_noop() {
        let args = json!({
            "old_text": "old",
            "new_text": "new",
            "start_line": 2,
            "end_line": 2,
            "before_context": "alpha\n",
            "after_context": "\nomega",
            "expected_matches": 1
        });

        let result =
            apply_text_edit("alpha\nnew\nomega\n", &args, "old", "new").expect("already applied");

        assert!(!result.changed);
        assert_eq!(result.info["already_applied"], json!(true));
    }

    #[test]
    fn edit_accepts_surrounding_context_lines_without_boundary_newlines() {
        let args = json!({
            "old_text": "old line one\nold line two",
            "new_text": "new line one\nnew line two",
            "start_line": 2,
            "end_line": 3,
            "before_context": "before",
            "after_context": "after",
            "expected_matches": 1
        });

        let result = apply_text_edit(
            "before\nold line one\nold line two\nafter\n",
            &args,
            "old line one\nold line two",
            "new line one\nnew line two",
        )
        .expect("edit with line-oriented context");

        assert!(result.changed);
        assert_eq!(result.info["replacements"], json!(1));
    }
}
