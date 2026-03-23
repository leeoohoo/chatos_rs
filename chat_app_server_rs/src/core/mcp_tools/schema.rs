use serde_json::{json, Value};

use super::{ParsedToolDefinition, ToolSchemaFormat};

pub fn parse_tool_definition(tool: &Value) -> Option<ParsedToolDefinition> {
    let name = tool
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let description = tool
        .get("description")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let parameters = tool
        .get("inputSchema")
        .cloned()
        .unwrap_or_else(default_tool_parameters);

    Some(ParsedToolDefinition {
        name,
        description,
        parameters,
    })
}

pub fn build_function_tool_schema(
    name: &str,
    description: &str,
    parameters: &Value,
    format: ToolSchemaFormat,
) -> Value {
    match format {
        ToolSchemaFormat::LegacyChatCompletions => json!({
            "type": "function",
            "function": {
                "name": name,
                "description": description,
                "parameters": parameters
            }
        }),
        ToolSchemaFormat::ResponsesStrict => json!({
            "type": "function",
            "name": name,
            "description": description,
            "parameters": normalize_json_schema(parameters)
        }),
    }
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

        let object = match node.as_object_mut() {
            Some(object) => object,
            None => return,
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
            if !object.contains_key("type") {
                object.insert("type".to_string(), Value::String("object".to_string()));
            }

            let mut required: Vec<String> = object
                .get("required")
                .and_then(|value| value.as_array())
                .map(|array| {
                    array
                        .iter()
                        .filter_map(|value| value.as_str().map(|raw| raw.to_string()))
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
            .and_then(|value| value.as_str())
            .map(|value| value == "object")
            .unwrap_or(false)
            || object.contains_key("properties");
        if is_object_schema {
            object.insert("additionalProperties".to_string(), Value::Bool(false));
        }

        if let Some(items) = object.get_mut("items") {
            visit(items);
        }
        if let Some(any_of) = object
            .get_mut("anyOf")
            .and_then(|value| value.as_array_mut())
        {
            for value in any_of {
                visit(value);
            }
        }
        if let Some(one_of) = object
            .get_mut("oneOf")
            .and_then(|value| value.as_array_mut())
        {
            for value in one_of {
                visit(value);
            }
        }
        if let Some(all_of) = object
            .get_mut("allOf")
            .and_then(|value| value.as_array_mut())
        {
            for value in all_of {
                visit(value);
            }
        }
        if let Some(not) = object.get_mut("not") {
            visit(not);
        }
        if let Some(additional) = object.get_mut("additionalProperties") {
            visit(additional);
        }
        if let Some(definitions) = object
            .get_mut("definitions")
            .and_then(|value| value.as_object_mut())
        {
            for value in definitions.values_mut() {
                visit(value);
            }
        }
        if let Some(definitions) = object
            .get_mut("$defs")
            .and_then(|value| value.as_object_mut())
        {
            for value in definitions.values_mut() {
                visit(value);
            }
        }
        if let Some(value) = object.get_mut("if") {
            visit(value);
        }
        if let Some(value) = object.get_mut("then") {
            visit(value);
        }
        if let Some(value) = object.get_mut("else") {
            visit(value);
        }
    }

    visit(&mut root);
    root
}
