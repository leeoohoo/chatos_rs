// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

pub(crate) fn public_tool_name(server_name: &str, tool_name: &str) -> String {
    format!(
        "{}_{}",
        canonical_name_segment(server_name, "server"),
        canonical_name_segment(tool_name, "tool")
    )
}

fn canonical_name_segment(raw: &str, fallback: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut last_was_separator = false;
    for character in raw.trim().chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
            output.push(character);
            last_was_separator = false;
        } else if !last_was_separator {
            output.push('_');
            last_was_separator = true;
        }
    }
    let output = output.trim_matches('_');
    if output.is_empty() {
        fallback.to_string()
    } else {
        output.to_string()
    }
}

pub(crate) fn function_schema(tool: &Value, public_name: &str) -> Result<Value, String> {
    let name = tool
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "MCP tool definition has no name".to_string())?;
    if name.chars().any(char::is_control) {
        return Err("MCP tool name contains control characters".to_string());
    }
    let description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let parameters = tool
        .get("inputSchema")
        .cloned()
        .unwrap_or_else(|| json!({"type": "object", "properties": {}, "required": []}));
    Ok(json!({
        "type": "function",
        "name": public_name,
        "description": description,
        "parameters": normalize_json_schema(parameters),
    }))
}

fn normalize_json_schema(mut schema: Value) -> Value {
    fn visit(value: &mut Value) {
        if let Some(array) = value.as_array_mut() {
            for item in array {
                visit(item);
            }
            return;
        }
        let Some(object) = value.as_object_mut() else {
            return;
        };
        if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
            for property in properties.values_mut() {
                visit(property);
            }
        }
        if object.contains_key("properties") {
            object
                .entry("type".to_string())
                .or_insert_with(|| Value::String("object".to_string()));
        }
        if object.get("type").and_then(Value::as_str) == Some("object")
            || object.contains_key("properties")
        {
            object.insert("additionalProperties".to_string(), Value::Bool(false));
        }
        for key in ["items", "not", "additionalProperties", "if", "then", "else"] {
            if let Some(child) = object.get_mut(key) {
                visit(child);
            }
        }
        for key in ["anyOf", "oneOf", "allOf"] {
            if let Some(children) = object.get_mut(key).and_then(Value::as_array_mut) {
                for child in children {
                    visit(child);
                }
            }
        }
        for key in ["definitions", "$defs"] {
            if let Some(children) = object.get_mut(key).and_then(Value::as_object_mut) {
                for child in children.values_mut() {
                    visit(child);
                }
            }
        }
    }
    visit(&mut schema);
    schema
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{function_schema, public_tool_name};

    #[test]
    fn names_are_stable_and_schemas_are_closed() {
        let name = public_tool_name("plugin-design.canvas", "create frame");
        assert_eq!(name, "plugin-design_canvas_create_frame");
        let schema = function_schema(
            &json!({
                "name": "create frame",
                "description": "Create one frame",
                "inputSchema": {"properties": {"title": {"type": "string"}}}
            }),
            name.as_str(),
        )
        .expect("schema");
        assert_eq!(schema["name"], name);
        assert_eq!(schema["parameters"]["type"], "object");
        assert_eq!(schema["parameters"]["additionalProperties"], false);
    }
}
