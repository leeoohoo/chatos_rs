// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::tools;

pub const REQUIREMENT_STATUS_VALUES: &[&str] = &[
    "draft",
    "reviewing",
    "approved",
    "in_progress",
    "blocked",
    "failed",
    "done",
    "cancelled",
];
pub const REQUIREMENT_TYPE_VALUES: &[&str] = &["requirement", "change", "bug_fix"];
pub const PROJECT_TASK_STATUS_VALUES: &[&str] = &[
    "todo",
    "ready",
    "in_progress",
    "blocked",
    "failed",
    "done",
    "cancelled",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskRunnerExecutionSchemaOptions {
    pub model_config_ids: Vec<String>,
    pub default_model_config_id: Option<String>,
    pub tool_ids: Vec<String>,
    pub skill_ids: Vec<String>,
}

pub fn project_management_server_tool_definitions(
    execution_options: Option<&TaskRunnerExecutionSchemaOptions>,
) -> Vec<Value> {
    tool_definitions(execution_options, true)
}

pub fn task_runner_builtin_tool_definitions(
    execution_options: Option<&TaskRunnerExecutionSchemaOptions>,
) -> Vec<Value> {
    tool_definitions(execution_options, true)
}

pub fn requirement_status_values() -> Vec<&'static str> {
    REQUIREMENT_STATUS_VALUES.to_vec()
}

pub fn project_task_status_values() -> Vec<&'static str> {
    PROJECT_TASK_STATUS_VALUES.to_vec()
}

fn tool_definitions(
    execution_options: Option<&TaskRunnerExecutionSchemaOptions>,
    include_delete_tools: bool,
) -> Vec<Value> {
    let mut definitions = vec![
        tool_definition(
            tools::GET_PROJECT_OVERVIEW,
            "Get the current project's base information and profile.",
            object_schema(vec![], vec![]),
        ),
        tool_definition(
            tools::INITIALIZE_PROJECT,
            "Initialize or update the current project's base description and one-to-one profile fields such as background and introduction.",
            object_schema(
                vec![
                    optional_string_field("name", "Optional project name update."),
                    optional_string_field("root_path", "Optional repository or workspace root path."),
                    optional_string_field("git_url", "Optional git remote URL."),
                    optional_string_field("description", "Short project description on the base project record."),
                    optional_string_field("background", "Project background stored in project profile."),
                    optional_string_field("introduction", "Project introduction stored in project profile."),
                ],
                vec![],
            ),
        ),
        tool_definition(
            tools::LIST_REQUIREMENTS,
            "List requirements for the current project. Prefer keyword and pagination for large projects instead of reading every requirement at once.",
            object_schema(
                vec![
                    enum_field(
                        "status",
                        "Optional requirement status filter.",
                        REQUIREMENT_STATUS_VALUES,
                    ),
                    optional_string_field(
                        "keyword",
                        "Optional case-insensitive fuzzy keyword across id, title, summary, detail, business value, acceptance criteria, and source.",
                    ),
                    page_limit_field(),
                    page_offset_field(),
                ],
                vec![],
            ),
        ),
        tool_definition(
            tools::CREATE_REQUIREMENT,
            "Create a requirement in the current project. If the work is similar to a done requirement, create a new requirement instead of modifying or extending the done one. Do not attach new child requirements under a done parent requirement.",
            object_schema(
                vec![
                    string_field("title", "Requirement title."),
                    optional_string_field("parent_requirement_id", "Optional parent requirement id."),
                    enum_field(
                        "requirement_type",
                        "Optional requirement type.",
                        REQUIREMENT_TYPE_VALUES,
                    ),
                    optional_string_field("summary", "Short requirement summary."),
                    optional_string_field("detail", "Detailed requirement description."),
                    optional_string_field("business_value", "Business value or why this matters."),
                    optional_string_field("acceptance_criteria", "Acceptance criteria."),
                    optional_string_field("source", "Requirement source."),
                    integer_field("priority", "Optional priority; higher means more important."),
                    enum_field("status", "Optional requirement status.", REQUIREMENT_STATUS_VALUES),
                    optional_string_field("assignee_user_id", "Optional assignee user id."),
                ],
                vec!["title"],
            ),
        ),
        tool_definition(
            tools::UPDATE_REQUIREMENT,
            "Update a requirement and optionally replace its prerequisite requirement ids. Done requirements are immutable through MCP; create a new requirement for similar new work.",
            object_schema(
                vec![
                    string_field("requirement_id", "Requirement id to update."),
                    requirement_patch_field(),
                    string_array_field(
                        "prerequisite_requirement_ids",
                        "Optional full replacement list of prerequisite requirement ids.",
                    ),
                ],
                vec!["requirement_id", "patch"],
            ),
        ),
    ];

    if include_delete_tools {
        definitions.push(tool_definition(
            tools::DELETE_REQUIREMENT,
            "Delete a requirement and its child requirements, technical documents, project tasks, and dependency edges when none of the affected project tasks have been executed. Done requirements are immutable and cannot be deleted through MCP. Use this during planning to remove incorrectly created requirements instead of archiving or cancelling them.",
            object_schema(
                vec![string_field("requirement_id", "Requirement id to delete.")],
                vec!["requirement_id"],
            ),
        ));
    }

    definitions.extend([
        tool_definition(
            tools::SET_REQUIREMENT_DEPENDENCIES,
            "Replace prerequisite requirements for one requirement. Done requirements are immutable through MCP; do not change their dependencies.",
            object_schema(
                vec![
                    string_field("requirement_id", "Requirement id to update."),
                    string_array_field(
                        "prerequisite_requirement_ids",
                        "Full replacement list of prerequisite requirement ids.",
                    ),
                ],
                vec!["requirement_id", "prerequisite_requirement_ids"],
            ),
        ),
        tool_definition(
            tools::LIST_REQUIREMENT_TECHNICAL_DOCUMENTS,
            "List all technical documents for a requirement. Use this before reading or updating docs so long content can stay split across focused documents.",
            object_schema(
                vec![
                    string_field("requirement_id", "Requirement id."),
                    optional_string_field(
                        "doc_type",
                        "Optional document type filter. Recommended values include technical_overview, implementation_plan, ui_svg_preview, architecture_diagram, flowchart, sequence_diagram, api_design, data_model, risk_notes, and other.",
                    ),
                ],
                vec!["requirement_id"],
            ),
        ),
        tool_definition(
            tools::GET_REQUIREMENT_TECHNICAL_DOCUMENT,
            "Get one technical document by document_id for a requirement.",
            object_schema(
                vec![
                    string_field("requirement_id", "Requirement id."),
                    string_field("document_id", "Technical document id from list_requirement_technical_documents."),
                ],
                vec!["requirement_id", "document_id"],
            ),
        ),
        tool_definition(
            tools::UPSERT_REQUIREMENT_TECHNICAL_DOCUMENT,
            "Create a new typed technical document for a requirement, or update an existing one when document_id is provided. Done requirements are immutable through MCP, so create a new requirement for similar new work instead of editing completed docs. Keep each document focused and split long content by doc_type or title.",
            object_schema(
                vec![
                    string_field("requirement_id", "Requirement id."),
                    optional_string_field("document_id", "Existing document id to update; omit to create a new document."),
                    optional_string_field(
                        "doc_type",
                        "Document type. Recommended values include technical_overview, implementation_plan, ui_svg_preview, architecture_diagram, flowchart, sequence_diagram, api_design, data_model, risk_notes, and other.",
                    ),
                    optional_string_field("title", "Document title."),
                    optional_string_field("format", "Document format, usually markdown."),
                    string_field("content", "Document content."),
                ],
                vec!["requirement_id", "content"],
            ),
        ),
        tool_definition(
            tools::LIST_PROJECT_TASKS,
            "List project-management tasks/work items for the current project. Prefer requirement_id, keyword, and pagination for large projects instead of reading every task at once.",
            object_schema(
                vec![
                    enum_field(
                        "status",
                        "Optional project task status filter.",
                        PROJECT_TASK_STATUS_VALUES,
                    ),
                    optional_string_field(
                        "keyword",
                        "Optional case-insensitive fuzzy keyword across id, requirement_id, title, description, and tags.",
                    ),
                    optional_string_field(
                        "requirement_id",
                        "Optional requirement id filter for checking coverage under one requirement.",
                    ),
                    optional_boolean_field(
                        "is_planning_task",
                        "Optional filter for project tasks that are themselves planning/decomposition tasks.",
                    ),
                    page_limit_field(),
                    page_offset_field(),
                ],
                vec![],
            ),
        ),
        tool_definition(
            tools::CREATE_PROJECT_TASK,
            "Create a project-management task/work item under a requirement. The requirement must be open and must already have at least one non-empty technical document. If similar work was already done under another requirement, create a new task for this requirement instead of modifying the completed one.",
            object_schema(
                vec![
                    string_field("requirement_id", "Requirement id this project task belongs to."),
                    string_field("title", "Project task title."),
                    optional_string_field("description", "Project task description."),
                    task_runner_model_config_field(execution_options),
                    task_runner_tool_ids_field(execution_options),
                    task_runner_skill_ids_field(execution_options),
                    enum_field(
                        "status",
                        "Optional project task status.",
                        PROJECT_TASK_STATUS_VALUES,
                    ),
                    integer_field("priority", "Optional priority; higher means more important."),
                    optional_string_field("assignee_user_id", "Optional assignee user id."),
                    integer_field("estimate_points", "Optional estimate points."),
                    optional_string_field("due_at", "Optional due time as string."),
                    integer_field("sort_order", "Optional sort order."),
                    string_array_field("tags", "Optional tags."),
                    boolean_field(
                        "is_planning_task",
                        "Set true only when this project task is itself a planning/decomposition task whose execution should continue project planning through TaskRunner chatos_plan profile. Leave false for implementation, testing, documentation, deployment, or other concrete execution work.",
                    ),
                    string_array_field(
                        "prerequisite_project_task_ids",
                        "Optional full list of prerequisite project task ids.",
                    ),
                ],
                vec![
                    "requirement_id",
                    "title",
                    "task_runner_default_model_config_id",
                    "task_runner_enabled_tool_ids",
                ],
            ),
        ),
        tool_definition(
            tools::UPDATE_PROJECT_TASK,
            "Update a project-management task/work item and optionally replace its prerequisite project task ids. Done project tasks are immutable through MCP; create a new task for similar new work.",
            object_schema(
                vec![
                    string_field("project_task_id", "Project task/work item id to update."),
                    project_task_patch_field(),
                    string_array_field(
                        "prerequisite_project_task_ids",
                        "Optional full replacement list of prerequisite project task ids.",
                    ),
                ],
                vec!["project_task_id", "patch"],
            ),
        ),
    ]);

    if include_delete_tools {
        definitions.push(tool_definition(
            tools::DELETE_PROJECT_TASK,
            "Delete a project-management task/work item that has not been executed. Done project tasks are immutable and cannot be deleted through MCP. Use this during planning to remove incorrectly created project tasks instead of cancelling them.",
            object_schema(
                vec![string_field(
                    "project_task_id",
                    "Project task/work item id to delete.",
                )],
                vec!["project_task_id"],
            ),
        ));
    }

    definitions.extend([
        tool_definition(
            tools::SET_PROJECT_TASK_DEPENDENCIES,
            "Replace prerequisite project task ids for one project task. Done project tasks are immutable through MCP; do not change their dependencies.",
            object_schema(
                vec![
                    string_field("project_task_id", "Project task/work item id to update."),
                    string_array_field(
                        "prerequisite_project_task_ids",
                        "Full replacement list of prerequisite project task ids.",
                    ),
                ],
                vec!["project_task_id", "prerequisite_project_task_ids"],
            ),
        ),
        tool_definition(
            tools::GET_PROJECT_DEPENDENCY_GRAPH,
            "Get the current project's dependency graph with requirements, project tasks, contains edges, and blocks edges.",
            object_schema(vec![], vec![]),
        ),
    ]);

    definitions
}

fn tool_definition(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

fn object_schema(properties: Vec<(&'static str, Value)>, required: Vec<&'static str>) -> Value {
    let mut props = serde_json::Map::new();
    for (name, schema) in properties {
        props.insert(name.to_string(), schema);
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": props,
        "required": required
    })
}

fn string_field(name: &'static str, description: &'static str) -> (&'static str, Value) {
    (
        name,
        json!({ "type": "string", "description": description }),
    )
}

fn optional_string_field(name: &'static str, description: &'static str) -> (&'static str, Value) {
    (
        name,
        json!({ "type": ["string", "null"], "description": description }),
    )
}

fn integer_field(name: &'static str, description: &'static str) -> (&'static str, Value) {
    (
        name,
        json!({ "type": ["integer", "null"], "description": description }),
    )
}

fn boolean_field(name: &'static str, description: &'static str) -> (&'static str, Value) {
    (
        name,
        json!({ "type": "boolean", "default": false, "description": description }),
    )
}

fn optional_boolean_field(name: &'static str, description: &'static str) -> (&'static str, Value) {
    (
        name,
        json!({ "type": ["boolean", "null"], "description": description }),
    )
}

fn page_limit_field() -> (&'static str, Value) {
    (
        "limit",
        json!({
            "type": ["integer", "null"],
            "minimum": 1,
            "maximum": 100,
            "description": "Optional page size. Defaults to 50 and is capped at 100."
        }),
    )
}

fn page_offset_field() -> (&'static str, Value) {
    (
        "offset",
        json!({
            "type": ["integer", "null"],
            "minimum": 0,
            "description": "Optional zero-based page offset. Use next_offset from the previous result to continue."
        }),
    )
}

fn enum_field(
    name: &'static str,
    description: &'static str,
    values: &'static [&'static str],
) -> (&'static str, Value) {
    (
        name,
        json!({
            "type": ["string", "null"],
            "enum": values.iter().copied().map(Value::from).chain(std::iter::once(Value::Null)).collect::<Vec<_>>(),
            "description": description
        }),
    )
}

fn string_array_field(name: &'static str, description: &'static str) -> (&'static str, Value) {
    (
        name,
        json!({
            "type": ["array", "null"],
            "items": { "type": "string" },
            "description": description
        }),
    )
}

fn requirement_patch_field() -> (&'static str, Value) {
    (
        "patch",
        object_schema(
            vec![
                optional_string_field("parent_requirement_id", "Optional parent requirement id."),
                enum_field(
                    "requirement_type",
                    "Optional requirement type.",
                    REQUIREMENT_TYPE_VALUES,
                ),
                optional_string_field("title", "Optional requirement title."),
                optional_string_field("summary", "Optional requirement summary."),
                optional_string_field("detail", "Optional requirement detail."),
                optional_string_field("business_value", "Optional business value."),
                optional_string_field("acceptance_criteria", "Optional acceptance criteria."),
                optional_string_field("source", "Optional requirement source."),
                integer_field(
                    "priority",
                    "Optional priority; higher means more important.",
                ),
                enum_field(
                    "status",
                    "Optional requirement status.",
                    REQUIREMENT_STATUS_VALUES,
                ),
                optional_string_field("assignee_user_id", "Optional assignee user id."),
            ],
            vec![],
        ),
    )
}

fn project_task_patch_field() -> (&'static str, Value) {
    (
        "patch",
        object_schema(
            vec![
                optional_string_field(
                    "requirement_id",
                    "Optional target requirement id. The target requirement must belong to the current project.",
                ),
                optional_string_field("title", "Optional project task title."),
                optional_string_field("description", "Optional project task description."),
                enum_field(
                    "status",
                    "Optional project task status.",
                    PROJECT_TASK_STATUS_VALUES,
                ),
                integer_field("priority", "Optional priority; higher means more important."),
                optional_string_field("assignee_user_id", "Optional assignee user id."),
                integer_field("estimate_points", "Optional estimate points."),
                optional_string_field("due_at", "Optional due time as string."),
                integer_field("sort_order", "Optional sort order."),
                string_array_field("tags", "Optional tags."),
                optional_boolean_field(
                    "is_planning_task",
                    "Set true only when this project task should execute as a TaskRunner chatos_plan planning/decomposition task. Set false for concrete implementation or delivery work.",
                ),
            ],
            vec![],
        ),
    )
}

fn task_runner_model_config_field(
    execution_options: Option<&TaskRunnerExecutionSchemaOptions>,
) -> (&'static str, Value) {
    let mut schema = json!({
        "type": "string",
        "minLength": 1,
        "description": "Required execution model config id. Use one of the enum values when present; if multiple are available, choose the model best suited for the project task instead of asking the user for an internal id."
    });
    if let Some(options) = execution_options {
        if !options.model_config_ids.is_empty() {
            schema["enum"] = json!(&options.model_config_ids);
        }
        if let Some(default_id) = options.default_model_config_id.as_deref() {
            schema["default"] = json!(default_id);
        }
    }
    ("task_runner_default_model_config_id", schema)
}

fn task_runner_tool_ids_field(
    execution_options: Option<&TaskRunnerExecutionSchemaOptions>,
) -> (&'static str, Value) {
    let mut item_schema = json!({ "type": "string" });
    let mut description = "Required execution tool id multi-select. Use only visible tool ids. Choose tools according to the work item's execution needs; for code implementation tasks, include appropriate code reading and terminal tools when available."
        .to_string();
    if let Some(options) = execution_options {
        if !options.tool_ids.is_empty() {
            description.push_str(" Available tool ids are exposed in the item enum.");
            item_schema["enum"] = json!(&options.tool_ids);
        }
    }
    (
        "task_runner_enabled_tool_ids",
        json!({
            "type": "array",
            "items": item_schema,
            "minItems": 1,
            "uniqueItems": true,
            "description": description
        }),
    )
}

fn task_runner_skill_ids_field(
    execution_options: Option<&TaskRunnerExecutionSchemaOptions>,
) -> (&'static str, Value) {
    let mut item_schema = json!({ "type": "string" });
    let mut description = "Optional execution skill id multi-select. Use only visible skill ids. Choose skills that match the project task's execution workflow, such as document/PDF/spreadsheet/browser/image/review skills. Omit when no relevant skill is needed."
        .to_string();
    if let Some(options) = execution_options {
        if !options.skill_ids.is_empty() {
            description.push_str(" Available skill ids are exposed in the item enum.");
            item_schema["enum"] = json!(&options.skill_ids);
        }
    }
    (
        "task_runner_skill_ids",
        json!({
            "type": "array",
            "items": item_schema,
            "uniqueItems": true,
            "description": description
        }),
    )
}
