// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;

pub(crate) fn tool_result_text_max_chars() -> usize {
    std::env::var("MCP_TOOL_RESULT_MAX_CHARS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(160_000)
}

pub fn truncate_tool_text(text: &str, max_chars: usize) -> String {
    if let Some(diagnostic) = capability_description_budget_error(text, max_chars) {
        return diagnostic;
    }
    if max_chars == 0 {
        return String::new();
    }
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    serde_json::json!({
        "error": "tool_result_budget_exceeded",
        "original_chars": total,
        "budget_chars": max_chars,
        "content_preserved": false,
        "guidance": "The tool result was not returned as a damaged head/tail excerpt. Use the tool's offset/limit pagination, a managed artifact, or the Host tool_result_read reference when available."
    })
    .to_string()
}

/// Machine-readable capability definitions must never become damaged JSON.
/// Small definitions pass through; over-budget definitions get a recoverable,
/// explicit diagnostic rather than a surviving unrelated head/tail tool list.
pub fn capability_description_budget_error(text: &str, max_chars: usize) -> Option<String> {
    if text.chars().count() <= max_chars {
        return None;
    }
    let value: Value = serde_json::from_str(text).ok()?;
    let descriptor = value
        .get("content")
        .filter(|value| value.is_object())
        .unwrap_or(&value);
    if descriptor
        .get("binding_schema_version")
        .and_then(Value::as_u64)
        != Some(1)
        && descriptor.get("disclosure_version").and_then(Value::as_u64) != Some(2)
    {
        return None;
    }
    let plugin = descriptor.get("plugin_option").and_then(Value::as_str)?;
    Some(serde_json::json!({
        "error":"capability_description_budget_exceeded",
        "plugin_option":plugin,
        "response_chars":text.chars().count(),
        "budget_chars":max_chars,
        "guidance":"No tool schema was truncated. Use capability_describe with query or smaller limit for an index, then a single tool_option. For a paged schema use its next_schema_offset. Follow only parameters advertised by this Run. If these fields are unavailable, create a new Run with the current capability policy; there is no fallback to full-catalog disclosure. Do not guess missing arguments or substitute unrelated raw tools."
    }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_catalog_overflow_is_valid_explicit_error_not_a_broken_definition() {
        for wrapped in [false, true] {
            let descriptor = serde_json::json!({"binding_schema_version":1,"plugin_option":"plugin_1",
                "provider_id":"plugin:test","tools":[{"name":"browser_navigate",
                    "input_schema":{"description":"中😀".repeat(15_000)}}]});
            let value = if wrapped {
                serde_json::json!({"content":descriptor,"is_error":false})
            } else {
                descriptor
            };
            let output = truncate_tool_text(&value.to_string(), 16_000);
            let error: Value = serde_json::from_str(&output).expect("valid JSON diagnostic");
            assert_eq!(error["error"], "capability_description_budget_exceeded");
            assert_eq!(error["plugin_option"], "plugin_1");
            assert!(output.chars().count() < 16_000);
            assert!(!output.contains("...[truncated"));
        }
    }

    #[test]
    fn small_pages_are_preserved_and_unrelated_overflow_is_an_explicit_error() {
        let page = serde_json::json!({"disclosure_version":2,"plugin_option":"plugin_1",
            "view":"tool","tools":[{"input_schema":{"type":"object"}}]})
        .to_string();
        assert_eq!(truncate_tool_text(&page, 16_000), page);
        let overflow: Value =
            serde_json::from_str(&truncate_tool_text(&"x".repeat(20_000), 16_000)).unwrap();
        assert_eq!(overflow["error"], "tool_result_budget_exceeded");
        assert_eq!(overflow["original_chars"], 20_000);
    }
}
