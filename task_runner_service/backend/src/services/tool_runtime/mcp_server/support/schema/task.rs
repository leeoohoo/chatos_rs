// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;

use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskMcpSchemaChoice {
    pub value: String,
    pub title: String,
}

pub(crate) fn enrich_project_execution_task_scope_schema(
    tools: &mut [Value],
    expected_project_task_ids: &BTreeSet<String>,
) {
    if expected_project_task_ids.is_empty() {
        return;
    }
    let allowed_refs = chatos_project_execution::build_project_task_scope_refs(
        expected_project_task_ids.iter().map(String::as_str),
    )
    .into_values()
    .map(Value::String)
    .collect::<Vec<_>>();
    for tool in tools {
        if tool.get("name").and_then(Value::as_str) != Some("create_project_execution_tasks") {
            continue;
        }
        let Some(schema) = tool
            .pointer_mut("/inputSchema/properties/tasks/items/properties/project_task_ref")
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        schema.insert("enum".to_string(), Value::Array(allowed_refs.clone()));
        schema.insert(
            "description".to_string(),
            Value::String(
                "Select the request-scoped project task reference shown in selected_project_tasks. Task Runner resolves the reference to the internal project task id; never send UUID values."
                    .to_string(),
            ),
        );
    }
}

pub(crate) fn enrich_tool_schemas_with_task_mcp_choices(
    tools: &mut [Value],
    builtin_choices: &[TaskMcpSchemaChoice],
    external_choices: &[TaskMcpSchemaChoice],
) {
    let builtin_schema = task_mcp_selection_schema(
        "Select the builtin MCP capabilities this task needs. Choose only the minimum sufficient subset exposed by Plugin Management for the target Task Runner Agent. Required capabilities are added automatically.",
        builtin_choices,
    );
    let external_schema = task_mcp_selection_schema(
        "Select the external MCP configurations this task needs. Values are MCP configuration ids exposed by Plugin Management for the target Task Runner Agent. Provider and project runtime routing are resolved by the program.",
        external_choices,
    );
    for tool in tools {
        let properties_pointer = match tool.get("name").and_then(Value::as_str) {
            Some("create_task") => "/inputSchema/properties",
            Some("create_tasks_with_prerequisites") | Some("create_project_execution_tasks") => {
                "/inputSchema/properties/tasks/items/properties"
            }
            _ => continue,
        };
        let Some(properties) = tool
            .pointer_mut(properties_pointer)
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        properties.insert("enabled_builtin_kinds".to_string(), builtin_schema.clone());
        properties.insert(
            "external_mcp_config_ids".to_string(),
            external_schema.clone(),
        );
    }
}

fn task_mcp_selection_schema(description: &str, choices: &[TaskMcpSchemaChoice]) -> Value {
    let mut item_schema = json!({ "type": "string", "minLength": 1 });
    if !choices.is_empty() {
        item_schema["enum"] = Value::Array(
            choices
                .iter()
                .map(|choice| Value::String(choice.value.clone()))
                .collect(),
        );
        item_schema["oneOf"] = Value::Array(
            choices
                .iter()
                .map(|choice| json!({ "const": choice.value, "title": choice.title }))
                .collect(),
        );
        item_schema["x-enum-labels"] = Value::Array(
            choices
                .iter()
                .map(|choice| Value::String(choice.title.clone()))
                .collect(),
        );
    }
    json!({
        "type": "array",
        "items": item_schema,
        "uniqueItems": true,
        "description": description
    })
}

pub(crate) fn create_task_schema() -> Value {
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
            "requires_execution": requires_execution_schema(),
            "enabled_builtin_kinds": task_mcp_selection_schema(
                "Select the builtin MCP capabilities this task needs from the target Agent binding.",
                &[],
            ),
            "external_mcp_config_ids": task_mcp_selection_schema(
                "Select the external MCP configuration ids this task needs from the target Agent binding.",
                &[],
            ),
            "schedule": { "type": "object", "description": "Optional task schedule configuration." },
            "prerequisite_task_ids": prerequisite_task_ids_schema()
        },
        "required": ["title", "objective", "requires_execution", "enabled_builtin_kinds"],
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
            "prerequisite_task_ids": prerequisite_task_ids_schema()
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
                        "requires_execution": requires_execution_schema(),
                        "enabled_builtin_kinds": task_mcp_selection_schema(
                            "Select the builtin MCP capabilities this task needs from the target Agent binding.",
                            &[],
                        ),
                        "external_mcp_config_ids": task_mcp_selection_schema(
                            "Select the external MCP configuration ids this task needs from the target Agent binding.",
                            &[],
                        ),
                        "owned_paths": {
                            "type": "array",
                            "maxItems": 200,
                            "items": { "type": "string", "minLength": 1 },
                            "uniqueItems": true,
                            "description": "Structured repository-relative files or directories owned by this execution task. Use an empty array only for a genuinely read-only verification task. Parallel tasks must not own overlapping paths; add a hard prerequisite edge when ownership must be sequential."
                        },
                        "schedule": { "type": "object" },
                        "prerequisite_refs": {
                            "type": "array",
                            "items": { "type": "string", "minLength": 1 },
                            "uniqueItems": true,
                            "description": "References to other client_ref values from the same create_tasks_with_prerequisites request."
                        },
                        "context_refs": {
                            "type": "array",
                            "items": { "type": "string", "minLength": 1 },
                            "uniqueItems": true,
                            "description": "Non-blocking context relationships to other client_ref values. They are preserved for explanation and graph display but never delay scheduling."
                        },
                        "prerequisite_task_ids": prerequisite_task_ids_schema()
                    },
                    "required": ["client_ref", "title", "objective", "requires_execution", "enabled_builtin_kinds"],
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
        "description": "Whether the task needs an execution workspace. Set false only when no command, Git operation, test, build, runtime check, or file mutation is needed. Harness project reads remain available without a sandbox."
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
                        "project_task_ref": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Program-generated reference for the project-management task/work item this execution task contributes to."
                        },
                        "title": { "type": "string", "minLength": 1, "description": "Execution-task title in the current user's language." },
                        "description": { "type": "string", "description": "Execution-task description in the current user's language." },
                        "objective": { "type": "string", "minLength": 1, "description": "Execution objective in the current user's language; preserve code, commands, paths, APIs, and product names." },
                        "acceptance_criteria": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 100,
                            "items": { "type": "string", "minLength": 1 },
                            "uniqueItems": true,
                            "description": "Every hard acceptance criterion this execution task must prove. Outcome Review requires a one-to-one structured evidence mapping before succeeded is allowed."
                        },
                        "task_role": {
                            "type": "string",
                            "enum": ["implementation", "verification"],
                            "description": "Programmatic task boundary. Verification tasks are read-only for project files and must create a repair task instead of modifying product code."
                        },
                        "input_payload": {},
                        "priority": { "type": "integer" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "default_model_config_id": {
                            "type": "string",
                            "description": "Optional Task Runner execution model config id. Omit to use the current user's default."
                        },
                        "requires_execution": requires_execution_schema(),
                        "enabled_builtin_kinds": task_mcp_selection_schema(
                            "Select the builtin MCP capabilities this execution task needs from the Task Runner execution Agent binding.",
                            &[],
                        ),
                        "external_mcp_config_ids": task_mcp_selection_schema(
                            "Select the external MCP configuration ids this execution task needs from the Task Runner execution Agent binding.",
                            &[],
                        ),
                        "prerequisite_refs": {
                            "type": "array",
                            "items": { "type": "string", "minLength": 1 },
                            "uniqueItems": true,
                            "description": "Direct hard-blocking references to other client_ref values from this request. Do not include a relation already implied by another prerequisite path."
                        },
                        "context_refs": {
                            "type": "array",
                            "items": { "type": "string", "minLength": 1 },
                            "uniqueItems": true,
                            "description": "Non-blocking relationship references used only to pass context and render the complete graph."
                        },
                        "prerequisite_task_ids": prerequisite_task_ids_schema()
                    },
                    "required": ["client_ref", "project_task_ref", "title", "objective", "acceptance_criteria", "task_role", "requires_execution", "enabled_builtin_kinds", "owned_paths"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["project_id", "requirement_id", "tasks"],
        "additionalProperties": false
    })
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
