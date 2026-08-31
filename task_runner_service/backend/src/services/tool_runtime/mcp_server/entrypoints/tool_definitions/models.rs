// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn model_tool_definitions() -> Vec<Value> {
    vec![
        tool_definition(
            "list_model_configs",
            "Administrative model config listing. Normal task creation binds a current-user model automatically and does not require model config ids.",
            empty_object_schema(),
        ),
        tool_definition(
            "get_model_config",
            "Get one Task Runner model config by id.",
            required_object_schema(
                json!({
                    "model_config_id": { "type": "string", "minLength": 1 }
                }),
                &["model_config_id"],
            ),
        ),
        tool_definition(
            "test_model_config",
            "Test whether one User Service model config can call its upstream model service.",
            required_object_schema(
                json!({
                    "model_config_id": { "type": "string", "minLength": 1 },
                    "prompt": { "type": "string" }
                }),
                &["model_config_id"],
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_tools_are_read_only_from_task_runner() {
        let names = model_tool_definitions()
            .into_iter()
            .filter_map(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>();

        assert!(names.iter().any(|name| name == "list_model_configs"));
        assert!(names.iter().any(|name| name == "get_model_config"));
        assert!(names.iter().any(|name| name == "test_model_config"));
        assert!(!names.iter().any(|name| name == "create_model_config"));
        assert!(!names.iter().any(|name| name == "update_model_config"));
        assert!(!names.iter().any(|name| name == "delete_model_config"));
    }
}
