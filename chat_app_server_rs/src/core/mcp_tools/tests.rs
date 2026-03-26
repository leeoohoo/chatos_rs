use serde_json::json;

use super::{
    build_function_tool_schema, normalize_json_schema, parse_tool_definition, truncate_tool_text,
    ToolSchemaFormat,
};

#[test]
fn parse_tool_definition_rejects_blank_name() {
    let tool = json!({
        "name": "   ",
        "description": "desc",
        "inputSchema": {"type": "object"}
    });

    assert!(parse_tool_definition(&tool).is_none());
}

#[test]
fn build_legacy_function_tool_schema_matches_expected_shape() {
    let parameters = json!({"type": "object", "properties": {"q": {"type": "string"}}});
    let schema = build_function_tool_schema(
        "search",
        "search docs",
        &parameters,
        ToolSchemaFormat::LegacyChatCompletions,
    );

    assert_eq!(
        schema.get("type").and_then(|v| v.as_str()),
        Some("function")
    );
    assert_eq!(
        schema
            .get("function")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str()),
        Some("search")
    );
    assert_eq!(
        schema
            .get("function")
            .and_then(|v| v.get("parameters"))
            .cloned(),
        Some(parameters)
    );
}

#[test]
fn normalize_json_schema_enforces_required_and_no_additional_properties() {
    let raw = json!({
        "properties": {
            "query": {"type": "string"},
            "nested": {
                "type": "object",
                "properties": {
                    "limit": {"type": "integer"}
                }
            }
        }
    });

    let normalized = normalize_json_schema(&raw);
    let required = normalized
        .get("required")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    assert!(required.contains(&json!("query")));
    assert!(required.contains(&json!("nested")));
    assert_eq!(
        normalized
            .get("additionalProperties")
            .and_then(|v| v.as_bool()),
        Some(false)
    );

    let nested = normalized
        .get("properties")
        .and_then(|v| v.get("nested"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        nested.get("additionalProperties").and_then(|v| v.as_bool()),
        Some(false)
    );
    let nested_required = nested
        .get("required")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(nested_required.contains(&json!("limit")));
}

#[test]
fn truncate_tool_text_keeps_head_and_tail() {
    let input = format!("{}{}", "a".repeat(200), "z".repeat(200));
    let out = truncate_tool_text(input.as_str(), 120);
    assert!(out.chars().count() <= 120);
    assert!(out.contains("truncated"));
    assert!(out.starts_with("a"));
    assert!(out.ends_with("z"));
}
