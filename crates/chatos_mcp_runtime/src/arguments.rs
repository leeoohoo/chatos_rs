// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub fn parse_json_loose(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }
    if let Some(stripped) = strip_markdown_fence(trimmed) {
        if let Ok(value) = serde_json::from_str::<Value>(stripped.as_str()) {
            return Some(value);
        }
    }
    extract_bracket_json(trimmed, '{', '}')
        .or_else(|| extract_bracket_json(trimmed, '[', ']'))
        .and_then(|candidate| serde_json::from_str::<Value>(candidate.as_str()).ok())
}

pub fn parse_tool_args(args: Value) -> Result<Value, serde_json::Error> {
    match args {
        Value::String(raw) => parse_tool_args_from_str(raw.as_str()),
        other => Ok(other),
    }
}

pub fn parse_json_tool_args(args: Value) -> Result<Value, serde_json::Error> {
    if let Some(raw) = args.as_str() {
        serde_json::from_str::<Value>(raw)
    } else {
        Ok(args)
    }
}

fn parse_tool_args_from_str(raw: &str) -> Result<Value, serde_json::Error> {
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        return parse_nested_json_string_value(value);
    }

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return serde_json::from_str::<Value>(raw);
    }

    let mut candidates = Vec::new();
    if let Some(stripped) = strip_markdown_fence(trimmed) {
        candidates.push(stripped);
    }
    if let Some(embedded) = extract_bracket_json(trimmed, '{', '}') {
        candidates.push(embedded);
    }
    if let Some(embedded) = extract_bracket_json(trimmed, '[', ']') {
        candidates.push(embedded);
    }
    candidates.push(trimmed.to_string());

    for candidate in candidates {
        if let Ok(value) = serde_json::from_str::<Value>(candidate.as_str()) {
            return parse_nested_json_string_value(value);
        }
        let repaired = remove_trailing_commas(candidate.as_str());
        if repaired != candidate {
            if let Ok(value) = serde_json::from_str::<Value>(repaired.as_str()) {
                return parse_nested_json_string_value(value);
            }
        }
    }

    serde_json::from_str::<Value>(raw)
}

fn parse_nested_json_string_value(value: Value) -> Result<Value, serde_json::Error> {
    let Some(inner) = value.as_str() else {
        return Ok(value);
    };
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    parse_tool_args_from_str(trimmed).or(Ok(value))
}

fn strip_markdown_fence(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return None;
    }
    let mut lines = trimmed.lines();
    let first_line = lines.next().unwrap_or_default();
    if !first_line.trim_start().starts_with("```") {
        return None;
    }

    let mut payload_lines = Vec::new();
    for line in lines {
        if line.trim_start().starts_with("```") {
            break;
        }
        payload_lines.push(line);
    }

    let joined = payload_lines.join("\n");
    let candidate = joined.trim();
    if candidate.is_empty() {
        None
    } else {
        Some(candidate.to_string())
    }
}

fn extract_bracket_json(raw: &str, open: char, close: char) -> Option<String> {
    let start = raw.find(open)?;
    let end = raw.rfind(close)?;
    if end <= start {
        return None;
    }
    let candidate = raw[start..=end].trim();
    if candidate.is_empty() {
        None
    } else {
        Some(candidate.to_string())
    }
}

fn remove_trailing_commas(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    while let Some(ch) = chars.next() {
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            out.push(ch);
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == ',' {
            let mut lookahead = chars.clone();
            let mut drop_comma = false;
            while let Some(next) = lookahead.peek() {
                if next.is_whitespace() {
                    lookahead.next();
                    continue;
                }
                if *next == '}' || *next == ']' {
                    drop_comma = true;
                }
                break;
            }
            if drop_comma {
                continue;
            }
        }

        out.push(ch);
    }

    out
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{parse_json_loose, parse_json_tool_args, parse_tool_args};

    #[test]
    fn loose_parser_extracts_fenced_and_embedded_json_without_repairing() {
        assert_eq!(
            parse_json_loose("before {\"title\":\"demo\"} after"),
            Some(json!({"title": "demo"}))
        );
        assert_eq!(
            parse_json_loose("```json\n[1, 2]\n```"),
            Some(json!([1, 2]))
        );
        assert!(parse_json_loose("{\"value\":1,}").is_none());
    }

    #[test]
    fn repaired_parser_accepts_nested_json_string() {
        let nested = serde_json::to_string("{\"title\":\"demo\"}").expect("nested json");
        let value = parse_tool_args(Value::String(nested)).expect("parse nested json");
        assert_eq!(value, json!({"title": "demo"}));
    }

    #[test]
    fn repaired_parser_accepts_markdown_fenced_json() {
        let raw = "```json\n{\"title\":\"demo\"}\n```".to_string();
        let value = parse_tool_args(Value::String(raw)).expect("parse fenced json");
        assert_eq!(value, json!({"title": "demo"}));
    }

    #[test]
    fn repaired_parser_accepts_trailing_commas() {
        let raw = "{\"tasks\":[{\"title\":\"a\",},],}".to_string();
        let value = parse_tool_args(Value::String(raw)).expect("parse repaired json");
        assert_eq!(value, json!({"tasks": [{"title": "a"}]}));
    }

    #[test]
    fn strict_parser_rejects_repaired_json_forms() {
        assert!(parse_json_tool_args(Value::String("```json\n{}\n```".to_string())).is_err());
        assert!(parse_json_tool_args(Value::String("{\"value\":1,}".to_string())).is_err());
    }
}
