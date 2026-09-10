// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TaskMcpSchemaChoice {
    pub value: String,
    pub title: String,
}

pub(crate) fn enrich_tool_schemas_with_task_mcp_choices(
    tools: &mut [Value],
    builtin_choices: &[TaskMcpSchemaChoice],
    external_choices: &[TaskMcpSchemaChoice],
    plugin_choices: &[TaskMcpSchemaChoice],
) {
    let builtin_schema = task_mcp_selection_schema(
        "Select the builtin MCP capabilities this task needs. Choose only the minimum sufficient subset exposed by Plugin Management for the target Task Runner Agent. Required capabilities are added automatically.",
        builtin_choices,
    );
    let external_schema = task_mcp_selection_schema(
        "Select the external MCP configurations this task needs. Values are MCP configuration ids exposed by Plugin Management for the target Task Runner Agent. Provider and project runtime routing are resolved by the program.",
        external_choices,
    );
    let plugin_hints_schema = task_plugin_hints_schema(plugin_choices);
    for tool in tools {
        let properties_pointer = match tool.get("name").and_then(Value::as_str) {
            Some("create_task") => "/inputSchema/properties",
            Some("create_tasks_with_prerequisites") => {
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
        properties.insert("plugin_hints".to_string(), plugin_hints_schema.clone());
    }
}

fn task_plugin_hints_schema(choices: &[TaskMcpSchemaChoice]) -> Value {
    let mut plugin_key_schema = json!({ "type": "string", "minLength": 1 });
    if !choices.is_empty() {
        plugin_key_schema["enum"] = Value::Array(
            choices
                .iter()
                .map(|choice| Value::String(choice.value.clone()))
                .collect(),
        );
        plugin_key_schema["oneOf"] = Value::Array(
            choices
                .iter()
                .map(|choice| json!({ "const": choice.value, "title": choice.title }))
                .collect(),
        );
        plugin_key_schema["x-enum-labels"] = Value::Array(
            choices
                .iter()
                .map(|choice| Value::String(choice.title.clone()))
                .collect(),
        );
    }
    let mut schema = json!({
        "type": "array",
        "maxItems": 16,
        "uniqueItems": true,
        "description": "Suggest Plugins required by this specific Task. Use only plugin_key values from the request-scoped Task Plugin catalog. Route by the actual interaction surface: use Computer Use for native desktop applications and operating-system UI; use Browser CDP only for websites in managed Chromium or an explicitly connected Chrome session. An app name such as Feishu/Lark, WeChat or DingTalk means the native desktop app unless the objective explicitly says web page, website, browser or Chrome. Do not select both merely as a fallback. These hints are non-authoritative; Task Runner resolves and validates the trusted Plugin ids, device installation and policy before saving the Task.",
        "items": {
            "type": "object",
            "properties": {
                "plugin_key": plugin_key_schema,
                "reason": {
                    "type": "string",
                    "maxLength": 1000,
                    "description": "Why this specific Task requires the Plugin."
                }
            },
            "required": ["plugin_key"],
            "additionalProperties": false
        }
    });
    if choices.is_empty() {
        schema["maxItems"] = Value::from(0);
        schema["description"] = Value::String(
            "No Task Plugins are selectable for this request context. Send an empty plugin_hints array; never invent a plugin_key."
                .to_string(),
        );
    }
    schema
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
    let mut schema = json!({
        "type": "array",
        "items": item_schema,
        "uniqueItems": true,
        "description": description
    });
    if choices.is_empty() {
        schema["maxItems"] = Value::from(0);
    }
    schema
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
            "plugin_hints": task_plugin_hints_schema(&[]),
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
                        "plugin_hints": task_plugin_hints_schema(&[]),
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
        "description": "Model selected explicitly by ChatOS for this task. Use the user's task-purpose description when available; otherwise choose from model identity and capabilities using your own knowledge."
    })
}

fn requires_execution_schema() -> Value {
    json!({
        "type": "boolean",
        "default": true,
        "description": "Whether the task needs an execution workspace. Set false only when no command, Git operation, test, build, runtime check, or file mutation is needed. Harness project reads remain available without a sandbox."
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
