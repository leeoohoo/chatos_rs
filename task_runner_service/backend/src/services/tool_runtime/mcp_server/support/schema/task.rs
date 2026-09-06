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
                            "description": "Every hard acceptance criterion this execution task must prove. The runtime records a one-to-one evidence mapping from successful paths, validation commands, and acceptance tools before succeeded is allowed."
                        },
                        "task_role": {
                            "type": "string",
                            "enum": ["implementation", "verification"],
                            "description": "Programmatic task boundary. Verification tasks are read-only for project files and must create a repair task instead of modifying product code."
                        },
                        "input_payload": {},
                        "priority": { "type": "integer" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "default_model_config_id": default_model_config_id_schema(),
                        "requires_execution": requires_execution_schema(),
                        "enabled_builtin_kinds": task_mcp_selection_schema(
                            "Select the builtin MCP capabilities this execution task needs from the Task Runner execution Agent binding.",
                            &[],
                        ),
                        "external_mcp_config_ids": task_mcp_selection_schema(
                            "Select the external MCP configuration ids this execution task needs from the Task Runner execution Agent binding.",
                            &[],
                        ),
                        "plugin_hints": task_plugin_hints_schema(&[]),
                        "owned_paths": {
                            "type": "array",
                            "maxItems": 200,
                            "items": { "type": "string", "minLength": 1 },
                            "uniqueItems": true,
                            "description": "Structured repository-relative files or directories owned by this execution task. Implementation tasks that select CodeMaintainerWrite must declare at least one path. Verification tasks must use an empty array."
                        },
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
                    "additionalProperties": false,
                    "examples": [{
                        "client_ref": "implementation_001",
                        "project_task_ref": "project_task_001",
                        "title": "实现项目任务",
                        "objective": "完成项目任务并验证结果",
                        "acceptance_criteria": ["目标功能通过验证"],
                        "task_role": "implementation",
                        "requires_execution": true,
                        "enabled_builtin_kinds": ["CodeMaintainerWrite", "TerminalController"],
                        "owned_paths": ["src"]
                    }]
                }
            }
        },
        "required": ["project_id", "requirement_id", "tasks"],
        "additionalProperties": false
    })
}

pub(crate) fn validate_create_project_execution_tasks_arguments(
    value: &Value,
) -> Result<(), String> {
    let mut errors = Vec::new();
    let Some(root) = value.as_object() else {
        return Err(
            "create_project_execution_tasks 参数校验失败:\n- arguments 必须是对象".to_string(),
        );
    };
    collect_unknown_fields(
        root,
        "arguments",
        &["project_id", "requirement_id", "tasks"],
        &mut errors,
    );
    collect_required_non_empty_string(root, "project_id", "project_id", &mut errors);
    collect_required_non_empty_string(root, "requirement_id", "requirement_id", &mut errors);
    match root.get("tasks") {
        None => errors.push("tasks 缺失".to_string()),
        Some(Value::Array(tasks)) => {
            if tasks.is_empty() {
                errors.push("tasks 至少需要 1 项".to_string());
            }
            if tasks.len() > 50 {
                errors.push("tasks 最多允许 50 项".to_string());
            }
            for (index, task) in tasks.iter().enumerate() {
                collect_project_execution_task_errors(task, index, &mut errors);
            }
        }
        Some(_) => errors.push("tasks 必须是数组".to_string()),
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "create_project_execution_tasks 参数校验失败:\n- {}",
            errors.join("\n- ")
        ))
    }
}

fn collect_project_execution_task_errors(value: &Value, index: usize, errors: &mut Vec<String>) {
    let path = format!("tasks[{index}]");
    let Some(task) = value.as_object() else {
        errors.push(format!("{path} 必须是对象"));
        return;
    };
    collect_unknown_fields(
        task,
        path.as_str(),
        &[
            "client_ref",
            "project_task_ref",
            "title",
            "description",
            "objective",
            "acceptance_criteria",
            "task_role",
            "input_payload",
            "priority",
            "tags",
            "default_model_config_id",
            "requires_execution",
            "enabled_builtin_kinds",
            "external_mcp_config_ids",
            "plugin_hints",
            "owned_paths",
            "prerequisite_refs",
            "context_refs",
            "prerequisite_task_ids",
        ],
        errors,
    );
    for field in ["client_ref", "project_task_ref", "title", "objective"] {
        collect_required_non_empty_string(task, field, format!("{path}.{field}"), errors);
    }
    if let Some(description) = task.get("description") {
        if !description.is_string() {
            errors.push(format!("{path}.description 必须是字符串"));
        }
    }
    if let Some(priority) = task.get("priority") {
        if !priority.is_i64() && !priority.is_u64() {
            errors.push(format!("{path}.priority 必须是整数"));
        }
    }
    if let Some(default_model_config_id) = task.get("default_model_config_id") {
        if default_model_config_id
            .as_str()
            .is_none_or(|value| value.trim().is_empty())
        {
            errors.push(format!("{path}.default_model_config_id 必须是非空字符串"));
        }
    }
    match task.get("task_role") {
        None => errors.push(format!("{path}.task_role 缺失")),
        Some(Value::String(role))
            if matches!(
                role.trim().to_ascii_lowercase().as_str(),
                "implementation" | "verification"
            ) => {}
        Some(Value::String(_)) => errors.push(format!(
            "{path}.task_role 必须是 implementation 或 verification"
        )),
        Some(_) => errors.push(format!("{path}.task_role 必须是字符串")),
    }
    match task.get("requires_execution") {
        None => errors.push(format!("{path}.requires_execution 缺失")),
        Some(Value::Bool(_)) => {}
        Some(_) => errors.push(format!("{path}.requires_execution 必须是布尔值")),
    }
    collect_required_string_array(
        task,
        "acceptance_criteria",
        format!("{path}.acceptance_criteria"),
        true,
        errors,
    );
    collect_required_string_array(
        task,
        "enabled_builtin_kinds",
        format!("{path}.enabled_builtin_kinds"),
        false,
        errors,
    );
    collect_required_string_array(
        task,
        "owned_paths",
        format!("{path}.owned_paths"),
        false,
        errors,
    );
    for field in [
        "tags",
        "external_mcp_config_ids",
        "prerequisite_refs",
        "context_refs",
        "prerequisite_task_ids",
    ] {
        if let Some(value) = task.get(field) {
            collect_string_array(value, format!("{path}.{field}"), false, errors);
        }
    }
}

fn collect_unknown_fields(
    object: &serde_json::Map<String, Value>,
    path: &str,
    allowed: &[&str],
    errors: &mut Vec<String>,
) {
    let allowed = allowed.iter().copied().collect::<BTreeSet<_>>();
    for field in object
        .keys()
        .filter(|field| !allowed.contains(field.as_str()))
    {
        errors.push(format!("{path}.{field} 是未知字段"));
    }
}

fn collect_required_non_empty_string(
    object: &serde_json::Map<String, Value>,
    field: &str,
    path: impl Into<String>,
    errors: &mut Vec<String>,
) {
    let path = path.into();
    match object.get(field) {
        None => errors.push(format!("{path} 缺失")),
        Some(Value::String(value)) if !value.trim().is_empty() => {}
        Some(Value::String(_)) => errors.push(format!("{path} 不能为空")),
        Some(_) => errors.push(format!("{path} 必须是字符串")),
    }
}

fn collect_required_string_array(
    object: &serde_json::Map<String, Value>,
    field: &str,
    path: String,
    require_non_empty: bool,
    errors: &mut Vec<String>,
) {
    match object.get(field) {
        None => errors.push(format!("{path} 缺失")),
        Some(value) => collect_string_array(value, path, require_non_empty, errors),
    }
}

fn collect_string_array(
    value: &Value,
    path: String,
    require_non_empty: bool,
    errors: &mut Vec<String>,
) {
    let Some(items) = value.as_array() else {
        errors.push(format!("{path} 必须是字符串数组"));
        return;
    };
    if require_non_empty && items.is_empty() {
        errors.push(format!("{path} 至少需要 1 项"));
    }
    for (index, item) in items.iter().enumerate() {
        if item.as_str().is_none_or(|value| value.trim().is_empty()) {
            errors.push(format!("{path}[{index}] 必须是非空字符串"));
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
