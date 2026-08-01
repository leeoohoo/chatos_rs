// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

pub(super) fn mcp_provider_skills_prefixed_input_items(prompt: Option<String>) -> Vec<Value> {
    let Some(prompt) = prompt
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Vec::new();
    };
    vec![json!({
        "type": "message",
        "role": "system",
        "content": [{
            "type": "input_text",
            "text": prompt
        }]
    })]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_skills_prompt_is_added_as_a_system_input_item() {
        let items = mcp_provider_skills_prefixed_input_items(Some(
            "# MCP Provider Skills\n\nUse the issue tracker.".to_string(),
        ));

        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["role"].as_str(), Some("system"));
        assert_eq!(
            items[0].pointer("/content/0/text").and_then(Value::as_str),
            Some("# MCP Provider Skills\n\nUse the issue tracker.")
        );
    }
}
