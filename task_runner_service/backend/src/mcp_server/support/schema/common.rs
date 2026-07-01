// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

pub(crate) fn tool_definition(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
    })
}

pub(crate) fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

pub(crate) fn required_object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

pub(crate) fn set_tool_property_description(tool: &mut Value, path: &[&str], description: String) {
    let mut current = tool;
    for segment in path {
        let Some(object) = current.as_object_mut() else {
            return;
        };
        let Some(next) = object.get_mut(*segment) else {
            return;
        };
        current = next;
    }
    let Some(object) = current.as_object_mut() else {
        return;
    };
    object.insert("description".to_string(), Value::String(description));
}

pub(crate) fn remove_tool_schema_property(tool: &mut Value, path: &[&str], property_name: &str) {
    let mut current = tool;
    for segment in path {
        let Some(object) = current.as_object_mut() else {
            return;
        };
        let Some(next) = object.get_mut(*segment) else {
            return;
        };
        current = next;
    }
    if let Some(object) = current.as_object_mut() {
        object.remove(property_name);
    }
}

pub(crate) fn set_schema_required_fields(tool: &mut Value, path: &[&str], required: &[&str]) {
    let mut current = tool;
    for segment in path {
        let Some(object) = current.as_object_mut() else {
            return;
        };
        let Some(next) = object.get_mut(*segment) else {
            return;
        };
        current = next;
    }
    *current = Value::Array(
        required
            .iter()
            .map(|value| Value::String((*value).to_string()))
            .collect(),
    );
}
