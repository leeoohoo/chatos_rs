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
            "create_model_config",
            "Create a new Task Runner model config.",
            create_model_config_schema(),
        ),
        tool_definition(
            "update_model_config",
            "Update an existing Task Runner model config.",
            required_object_schema(
                json!({
                    "model_config_id": { "type": "string", "minLength": 1 },
                    "patch": update_model_config_schema()
                }),
                &["model_config_id", "patch"],
            ),
        ),
        tool_definition(
            "delete_model_config",
            "Delete a Task Runner model config by id.",
            required_object_schema(
                json!({
                    "model_config_id": { "type": "string", "minLength": 1 }
                }),
                &["model_config_id"],
            ),
        ),
        tool_definition(
            "test_model_config",
            "Test whether one Task Runner model config can call its upstream model service.",
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
