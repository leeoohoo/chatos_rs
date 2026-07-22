// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::models::{mcp_builtin_kind_guide, mcp_builtin_kind_values};
use chatos_mcp_runtime::builtin_kind_by_any;

pub(crate) fn create_task_schema() -> Value {
    let enabled_builtin_kinds_description = builtin_mcp_kind_schema_description();
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "minLength": 1, "description": "Task title in the current user's language; preserve technical identifiers and proper nouns." },
            "description": { "type": "string", "description": "Task background or context in the current user's language." },
            "objective": { "type": "string", "minLength": 1, "description": "Concrete execution objective in the current user's language; preserve code, commands, paths, APIs, and product names." },
            "input_payload": { "description": "Structured JSON input, references, or material needed for execution." },
            "priority": { "type": "integer", "description": "Higher numbers mean higher priority." },
            "tags": { "type": "array", "items": { "type": "string" }, "description": "Task tags." },
            "default_model_config_id": default_model_config_id_schema(),
            "is_planning_task": planning_task_schema(),
            "requires_execution": requires_execution_schema(),
            "schedule": { "type": "object", "description": "Optional task schedule configuration." },
            "prerequisite_task_ids": prerequisite_task_ids_schema(),
            "enabled_builtin_kinds": {
                "type": "array",
                "items": builtin_mcp_kind_item_schema(),
                "uniqueItems": true,
                "description": enabled_builtin_kinds_description
            },
            "external_mcp_config_ids": external_mcp_config_ids_schema()
            ,"selected_skill_ids": selected_skill_ids_schema()
        },
        "required": ["title", "objective"],
        "additionalProperties": false
    })
}

pub(crate) fn update_task_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "description": { "type": "string" },
            "objective": { "type": "string" },
            "input_payload": {},
            "priority": { "type": "integer" },
            "tags": { "type": "array", "items": { "type": "string" } },
            "schedule": { "type": "object" },
            "prerequisite_task_ids": prerequisite_task_ids_schema(),
            "mcp_config": task_mcp_config_schema()
        },
        "additionalProperties": false
    })
}

pub(crate) fn prerequisite_task_ids_schema() -> Value {
    json!({
        "type": "array",
        "items": { "type": "string", "minLength": 1 },
        "uniqueItems": true,
        "description": "Existing task ids that must complete successfully before this task runs. Use only real task ids returned by Task Runner tools."
    })
}

pub(crate) fn create_tasks_with_prerequisites_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tasks": {
                "type": "array",
                "minItems": 1,
                "maxItems": 50,
                "items": {
                    "type": "object",
                    "properties": {
                        "client_ref": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Temporary reference within this tool call. Task Runner returns real task ids."
                        },
                        "title": { "type": "string", "minLength": 1, "description": "Task title in the current user's language." },
                        "description": { "type": "string", "description": "Task description in the current user's language." },
                        "objective": { "type": "string", "minLength": 1, "description": "Task objective in the current user's language; preserve technical identifiers and proper nouns." },
                        "input_payload": {},
                        "priority": { "type": "integer" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "default_model_config_id": default_model_config_id_schema(),
                        "is_planning_task": planning_task_schema(),
                        "requires_execution": requires_execution_schema(),
                        "schedule": { "type": "object" },
                        "enabled_builtin_kinds": {
                            "type": "array",
                            "items": builtin_mcp_kind_item_schema(),
                            "uniqueItems": true,
                            "description": builtin_mcp_kind_schema_description()
                        },
                        "external_mcp_config_ids": external_mcp_config_ids_schema(),
                        "selected_skill_ids": selected_skill_ids_schema(),
                        "prerequisite_refs": {
                            "type": "array",
                            "items": { "type": "string", "minLength": 1 },
                            "uniqueItems": true,
                            "description": "References to other client_ref values from the same create_tasks_with_prerequisites request."
                        },
                        "prerequisite_task_ids": prerequisite_task_ids_schema()
                    },
                    "required": ["client_ref", "title", "objective"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["tasks"],
        "additionalProperties": false
    })
}

fn default_model_config_id_schema() -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "description": "Optional single Task Runner execution model config id. The available choices are injected dynamically for the current user. Choose the model whose usage scenario best matches this task; omit the field to let Task Runner select automatically."
    })
}

fn requires_execution_schema() -> Value {
    json!({
        "type": "boolean",
        "default": true,
        "description": "Whether the task must run or validate the project. Set false for file-only inspection or editing: Task Runner uses the default sandbox image and does not require the project's initialized runtime image."
    })
}

fn planning_task_schema() -> Value {
    json!({
        "type": "boolean",
        "default": false,
        "description": "Whether the task itself is planning, requirement decomposition, or project-management maintenance. Set false for coding, testing, fixing, documentation delivery, deployment, and other implementation work. This is independent from requires_execution."
    })
}

pub(crate) fn create_project_execution_tasks_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project_id": {
                "type": "string",
                "minLength": 1,
                "description": "Project id for this requirement execution."
            },
            "requirement_id": {
                "type": "string",
                "minLength": 1,
                "description": "Requirement id being executed."
            },
            "execution_group_id": {
                "type": "string",
                "description": "Execution group id. Use the source_user_message_id provided by Chatos when available."
            },
            "tasks": {
                "type": "array",
                "minItems": 1,
                "maxItems": 50,
                "items": {
                    "type": "object",
                    "properties": {
                        "client_ref": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Temporary reference within this tool call. Task Runner returns real task ids."
                        },
                        "project_task_id": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Project-management task/work item id this execution task contributes to."
                        },
                        "title": { "type": "string", "minLength": 1, "description": "Execution-task title in the current user's language." },
                        "description": { "type": "string", "description": "Execution-task description in the current user's language." },
                        "objective": { "type": "string", "minLength": 1, "description": "Execution objective in the current user's language; preserve code, commands, paths, APIs, and product names." },
                        "input_payload": {},
                        "priority": { "type": "integer" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "default_model_config_id": {
                            "type": "string",
                            "description": "Optional Task Runner execution model config id. Omit to use the current user's default."
                        },
                        "is_planning_task": planning_task_schema(),
                        "requires_execution": requires_execution_schema(),
                        "enabled_builtin_kinds": {
                            "type": "array",
                            "items": builtin_mcp_kind_item_schema(),
                            "uniqueItems": true,
                            "description": builtin_mcp_kind_schema_description()
                        },
                        "external_mcp_config_ids": external_mcp_config_ids_schema(),
                        "selected_skill_ids": selected_skill_ids_schema(),
                        "prerequisite_refs": {
                            "type": "array",
                            "items": { "type": "string", "minLength": 1 },
                            "uniqueItems": true,
                            "description": "References to other client_ref values from this same request."
                        },
                        "prerequisite_task_ids": prerequisite_task_ids_schema()
                    },
                    "required": ["client_ref", "project_task_id", "title", "objective", "is_planning_task"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["project_id", "requirement_id", "tasks"],
        "additionalProperties": false
    })
}

pub(crate) fn task_mcp_config_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "enabled": { "type": "boolean", "description": "Whether MCP is enabled for this task." },
            "requires_execution": requires_execution_schema(),
            "builtin_prompt_mode": {
                "type": "string",
                "enum": ["effective", "configured"],
                "description": "MCP prompt generation mode."
            },
            "builtin_prompt_locale": {
                "type": "string",
                "enum": ["zh-CN", "en-US"],
                "description": "MCP prompt locale."
            },
            "enabled_builtin_kinds": {
                "type": "array",
                "items": builtin_mcp_kind_item_schema(),
                "uniqueItems": true,
                "description": builtin_mcp_kind_schema_description()
            },
            "external_mcp_config_ids": external_mcp_config_ids_schema(),
            "selected_skill_ids": selected_skill_ids_schema()
        },
        "additionalProperties": false
    })
}

pub(crate) fn external_mcp_config_ids_schema() -> Value {
    json!({
        "type": "array",
        "items": { "type": "string", "minLength": 1 },
        "uniqueItems": true,
        "description": "External MCP config ids to load during task execution. Use only ids returned by list_external_mcp_configs."
    })
}

pub(crate) fn selected_skill_ids_schema() -> Value {
    json!({
        "type": "array",
        "items": { "type": "string", "minLength": 1 },
        "uniqueItems": true,
        "description": "Local Connector Skill ids to prepare for task execution. Use only ids returned by list_available_skills."
    })
}

pub(crate) fn builtin_mcp_kind_item_schema() -> Value {
    json!({
        "type": "string",
        "enum": mcp_builtin_kind_values()
    })
}

pub(crate) fn builtin_mcp_kind_schema_description() -> String {
    let mut lines = vec![
        "Optional builtin MCP capability ids. Select only capabilities needed during execution; call list_mcp_builtin_catalog when unsure."
            .to_string(),
        "Constraint: CodeMaintainerWrite depends on CodeMaintainerRead; the backend also completes this dependency automatically."
            .to_string(),
    ];
    for value in mcp_builtin_kind_values() {
        if let Some(kind) = builtin_kind_by_any(value.as_str()) {
            let guide = mcp_builtin_kind_guide(kind);
            lines.push(format!(
                "- {}: {} Use cases: {}. Capabilities: {}.",
                value,
                guide.description,
                guide.use_cases.join(", "),
                guide.capabilities.join(", ")
            ));
        }
    }
    lines.join("\n")
}

pub(crate) fn normalize_mcp_builtin_kind_names(values: Vec<String>) -> Result<Vec<String>, String> {
    let allowed = mcp_builtin_kind_values();
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let kind = builtin_kind_by_any(trimmed).ok_or_else(|| {
            format!(
                "unknown builtin MCP kind: {trimmed}. Allowed values: {}",
                allowed.join(", ")
            )
        })?;
        if !out.contains(&kind) {
            out.push(kind);
        }
    }
    Ok(out
        .into_iter()
        .map(|kind| kind.kind_name().to_string())
        .collect())
}

#[cfg(test)]
mod language_tests {
    use super::*;

    #[test]
    fn project_execution_task_schema_requires_user_language_fields() {
        let schema = create_project_execution_tasks_schema();
        for field in ["title", "description", "objective"] {
            let description = schema
                .pointer(&format!(
                    "/properties/tasks/items/properties/{field}/description"
                ))
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("{field} description"));
            assert!(description.contains("current user's language"), "{field}");
        }
    }
}

pub(crate) fn task_status_values() -> Vec<&'static str> {
    vec![
        "draft",
        "ready",
        "queued",
        "running",
        "succeeded",
        "failed",
        "blocked",
        "cancelled",
        "archived",
    ]
}

pub(crate) fn prompt_status_values() -> Vec<&'static str> {
    vec!["pending", "submitted", "cancelled", "timed_out", "failed"]
}

pub(crate) fn restrict_task_capability_selection_schemas(
    tools: &mut [Value],
    selectable_builtin_kinds: &[String],
    selectable_external_mcp_ids: &[String],
    selectable_skill_ids: &[String],
) {
    for tool in tools {
        let Some(name) = tool.get("name").and_then(Value::as_str) else {
            continue;
        };
        let properties_pointer = match name {
            "create_task" => Some("/inputSchema/properties"),
            "create_tasks_with_prerequisites" => {
                Some("/inputSchema/properties/tasks/items/properties")
            }
            "update_task" => Some("/inputSchema/properties/patch/properties/mcp_config/properties"),
            _ => None,
        };
        let Some(properties_pointer) = properties_pointer else {
            continue;
        };
        restrict_optional_selection_property(
            tool,
            properties_pointer,
            "enabled_builtin_kinds",
            selectable_builtin_kinds,
            "Optional builtin MCP capabilities available for this task. Required and unavailable capabilities are managed by Task Runner and are not selectable.",
        );
        restrict_optional_selection_property(
            tool,
            properties_pointer,
            "selected_skill_ids",
            selectable_skill_ids,
            "Optional Local Connector Skill ids enabled by this user and available on the active client. Use only values from this field or list_available_skills.",
        );
        restrict_optional_selection_property(
            tool,
            properties_pointer,
            "external_mcp_config_ids",
            selectable_external_mcp_ids,
            "Optional external MCP resource ids available for this task. Use only values from this field or list_external_mcp_configs.",
        );
    }
}

fn restrict_optional_selection_property(
    tool: &mut Value,
    properties_pointer: &str,
    property_name: &str,
    allowed_values: &[String],
    description: &str,
) {
    let Some(properties) = tool
        .pointer_mut(properties_pointer)
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    if allowed_values.is_empty() {
        properties.remove(property_name);
        return;
    }
    let Some(property) = properties
        .get_mut(property_name)
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    property.insert("description".to_string(), json!(description));
    let items = property
        .entry("items".to_string())
        .or_insert_with(|| json!({ "type": "string" }));
    if let Some(items) = items.as_object_mut() {
        items.insert("type".to_string(), json!("string"));
        items.insert("enum".to_string(), json!(allowed_values));
    }
}

#[cfg(test)]
mod capability_schema_tests {
    use super::*;

    #[test]
    fn ai_selection_schema_contains_only_optional_values() {
        let mut tools = vec![json!({
            "name": "create_task",
            "inputSchema": create_task_schema()
        })];
        restrict_task_capability_selection_schemas(
            &mut tools,
            &["CodeMaintainerRead".to_string()],
            &["user-mcp-1".to_string()],
            &["skill-1".to_string()],
        );
        assert_eq!(
            tools[0]
                .pointer("/inputSchema/properties/enabled_builtin_kinds/items/enum")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            vec![json!("CodeMaintainerRead")]
        );
        assert_eq!(
            tools[0]
                .pointer("/inputSchema/properties/external_mcp_config_ids/items/enum")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            vec![json!("user-mcp-1")]
        );
        assert_eq!(
            tools[0]
                .pointer("/inputSchema/properties/selected_skill_ids/items/enum")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            vec![json!("skill-1")]
        );
    }

    #[test]
    fn empty_optional_sets_remove_ai_selection_fields() {
        let mut tools = vec![json!({
            "name": "create_task",
            "inputSchema": create_task_schema()
        })];
        restrict_task_capability_selection_schemas(&mut tools, &[], &[], &[]);
        assert!(tools[0]
            .pointer("/inputSchema/properties/enabled_builtin_kinds")
            .is_none());
        assert!(tools[0]
            .pointer("/inputSchema/properties/external_mcp_config_ids")
            .is_none());
        assert!(tools[0]
            .pointer("/inputSchema/properties/selected_skill_ids")
            .is_none());
    }
}
