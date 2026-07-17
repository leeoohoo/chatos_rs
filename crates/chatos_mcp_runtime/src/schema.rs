// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::types::ParsedToolDefinition;

pub fn parse_tool_definition(tool: &Value) -> Option<ParsedToolDefinition> {
    parse_tool_definition_with_parameters_alias(tool, true)
}

pub fn parse_mcp_tool_definition(tool: &Value) -> Option<ParsedToolDefinition> {
    parse_tool_definition_with_parameters_alias(tool, false)
}

fn parse_tool_definition_with_parameters_alias(
    tool: &Value,
    allow_parameters_alias: bool,
) -> Option<ParsedToolDefinition> {
    let name = tool
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let parameters = tool
        .get("inputSchema")
        .cloned()
        .or_else(|| {
            allow_parameters_alias
                .then(|| tool.get("parameters").cloned())
                .flatten()
        })
        .unwrap_or_else(default_tool_parameters);

    Some(ParsedToolDefinition {
        name,
        description,
        parameters,
    })
}

pub fn build_function_tool_schema(name: &str, description: &str, parameters: &Value) -> Value {
    json!({
        "type": "function",
        "name": name,
        "description": description,
        "parameters": normalize_json_schema(parameters)
    })
}

fn default_tool_parameters() -> Value {
    json!({"type":"object","properties":{},"required":[]})
}

pub fn normalize_json_schema(schema: &Value) -> Value {
    let mut root = schema.clone();

    fn visit(node: &mut Value) {
        if node.is_null() {
            return;
        }
        if let Some(array) = node.as_array_mut() {
            for item in array {
                visit(item);
            }
            return;
        }

        let Some(object) = node.as_object_mut() else {
            return;
        };

        let mut property_keys = Vec::new();
        if let Some(properties_value) = object.get_mut("properties") {
            if let Some(properties) = properties_value.as_object_mut() {
                property_keys = properties.keys().cloned().collect();
                for value in properties.values_mut() {
                    visit(value);
                }
            }
        }

        if !property_keys.is_empty() {
            object
                .entry("type".to_string())
                .or_insert_with(|| Value::String("object".to_string()));

            let mut required: Vec<String> = object
                .get("required")
                .and_then(Value::as_array)
                .map(|array| {
                    array
                        .iter()
                        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                        .collect()
                })
                .unwrap_or_default();

            for key in property_keys {
                if !required.iter().any(|current| current == &key) {
                    required.push(key);
                }
            }

            object.insert(
                "required".to_string(),
                Value::Array(required.into_iter().map(Value::String).collect()),
            );
        }

        let is_object_schema = object
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|value| value == "object")
            || object.contains_key("properties");
        if is_object_schema {
            object.insert("additionalProperties".to_string(), Value::Bool(false));
        }

        for key in ["items", "not", "additionalProperties", "if", "then", "else"] {
            if let Some(value) = object.get_mut(key) {
                visit(value);
            }
        }

        for key in ["anyOf", "oneOf", "allOf"] {
            if let Some(array) = object.get_mut(key).and_then(Value::as_array_mut) {
                for value in array {
                    visit(value);
                }
            }
        }

        for key in ["definitions", "$defs"] {
            if let Some(definitions) = object.get_mut(key).and_then(Value::as_object_mut) {
                for value in definitions.values_mut() {
                    visit(value);
                }
            }
        }
    }

    visit(&mut root);
    root
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{parse_mcp_tool_definition, parse_tool_definition};

    #[test]
    fn generic_tool_definition_accepts_parameters_alias() {
        let parsed = parse_tool_definition(&json!({
            "name": "search",
            "parameters": {"type": "object", "properties": {"q": {"type": "string"}}}
        }))
        .expect("tool definition");

        assert!(parsed.parameters.get("properties").is_some());
    }

    #[test]
    fn mcp_tool_definition_uses_only_input_schema() {
        let parsed = parse_mcp_tool_definition(&json!({
            "name": "search",
            "parameters": {"type": "object", "properties": {"q": {"type": "string"}}}
        }))
        .expect("tool definition");

        assert_eq!(
            parsed.parameters,
            json!({"type": "object", "properties": {}, "required": []})
        );
    }
}
