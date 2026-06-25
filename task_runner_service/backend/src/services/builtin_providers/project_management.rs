use std::collections::HashMap;

use serde_json::{json, Value};

use crate::models::{normalize_project_id, PUBLIC_PROJECT_ID};

#[derive(Clone)]
pub(in crate::services) struct ProjectManagementBuiltinService {
    server_name: String,
    base_url: Option<String>,
    sync_secret: Option<String>,
    owner_user_id: Option<String>,
    project_id: Option<String>,
}

impl ProjectManagementBuiltinService {
    pub(in crate::services) fn new(options: ProjectManagementOptions) -> Self {
        Self {
            server_name: options.server_name,
            base_url: normalize_optional(options.base_url),
            sync_secret: normalize_optional(options.sync_secret),
            owner_user_id: normalize_optional(options.owner_user_id),
            project_id: normalize_optional(options.project_id),
        }
    }

    pub(in crate::services) fn list_tools(&self) -> Vec<Value> {
        tool_definitions()
    }

    pub(in crate::services) async fn call_tool(
        &self,
        name: &str,
        args: Value,
    ) -> Result<Value, String> {
        let base_url = self
            .base_url
            .as_deref()
            .ok_or_else(|| "project service base url is not configured".to_string())?;
        let sync_secret = self
            .sync_secret
            .as_deref()
            .ok_or_else(|| "project service sync secret is not configured".to_string())?;
        let owner_user_id = self.owner_user_id.as_deref().ok_or_else(|| {
            format!(
                "{} builtin missing owner user id",
                self.server_name.as_str()
            )
        })?;
        let project_id = normalize_project_id(self.project_id.clone());
        if project_id == PUBLIC_PROJECT_ID {
            return Err(format!(
                "{} builtin requires concrete project_id",
                self.server_name.as_str()
            ));
        }

        let mut headers = HashMap::new();
        headers.insert(
            "X-Project-Service-Sync-Secret".to_string(),
            sync_secret.to_string(),
        );
        headers.insert(
            "X-Task-Runner-Owner-User-Id".to_string(),
            owner_user_id.to_string(),
        );
        headers.insert("X-Chatos-Project-Id".to_string(), project_id);
        headers.insert(
            "X-Task-Runner-Task-Profile".to_string(),
            crate::models::TASK_PROFILE_CHATOS_PLAN.to_string(),
        );

        chatos_mcp_runtime::jsonrpc_http_call(
            format!("{}/mcp", base_url.trim_end_matches('/')).as_str(),
            Some(&headers),
            "tools/call",
            json!({
                "name": name,
                "arguments": args,
            }),
        )
        .await
    }

    pub(in crate::services) fn unavailable_tools(&self) -> Vec<(String, String)> {
        let project_id = normalize_project_id(self.project_id.clone());
        let reason = if self.base_url.is_none() {
            Some("project service base url is not configured")
        } else if self.sync_secret.is_none() {
            Some("project service sync secret is not configured")
        } else if self.owner_user_id.is_none() {
            Some("project management builtin missing owner user id")
        } else if project_id == PUBLIC_PROJECT_ID {
            Some("project management builtin requires concrete project_id")
        } else {
            None
        };
        let Some(reason) = reason else {
            return Vec::new();
        };
        tool_definitions()
            .into_iter()
            .filter_map(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .map(|name| (name.to_string(), reason.to_string()))
            })
            .collect()
    }
}

#[derive(Clone)]
pub(in crate::services) struct ProjectManagementOptions {
    pub(in crate::services) server_name: String,
    pub(in crate::services) base_url: Option<String>,
    pub(in crate::services) sync_secret: Option<String>,
    pub(in crate::services) owner_user_id: Option<String>,
    pub(in crate::services) project_id: Option<String>,
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn tool_definitions() -> Vec<Value> {
    vec![
        tool_definition(
            "get_project_overview",
            "Get the current project's base information and profile.",
            object_schema(vec![], vec![]),
        ),
        tool_definition(
            "initialize_project",
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
            "list_requirements",
            "List requirements for the current project.",
            object_schema(
                vec![
                    enum_field("status", "Optional requirement status filter.", requirement_status_values()),
                    optional_string_field("keyword", "Optional fuzzy keyword."),
                ],
                vec![],
            ),
        ),
        tool_definition(
            "create_requirement",
            "Create a requirement in the current project.",
            object_schema(
                vec![
                    string_field("title", "Requirement title."),
                    optional_string_field("parent_requirement_id", "Optional parent requirement id."),
                    enum_field("requirement_type", "Optional requirement type.", requirement_type_values()),
                    optional_string_field("summary", "Short requirement summary."),
                    optional_string_field("detail", "Detailed requirement description."),
                    optional_string_field("business_value", "Business value or why this matters."),
                    optional_string_field("acceptance_criteria", "Acceptance criteria."),
                    optional_string_field("source", "Requirement source."),
                    integer_field("priority", "Optional priority; higher means more important."),
                    enum_field("status", "Optional requirement status.", requirement_status_values()),
                    optional_string_field("assignee_user_id", "Optional assignee user id."),
                ],
                vec!["title"],
            ),
        ),
        tool_definition(
            "update_requirement",
            "Update a requirement and optionally replace its prerequisite requirement ids.",
            object_schema(
                vec![
                    string_field("requirement_id", "Requirement id to update."),
                    patch_field("patch", "Fields to update on the requirement."),
                    string_array_field("prerequisite_requirement_ids", "Optional full replacement list of prerequisite requirement ids."),
                ],
                vec!["requirement_id", "patch"],
            ),
        ),
        tool_definition(
            "set_requirement_dependencies",
            "Replace prerequisite requirements for one requirement.",
            object_schema(
                vec![
                    string_field("requirement_id", "Requirement id to update."),
                    string_array_field("prerequisite_requirement_ids", "Full replacement list of prerequisite requirement ids."),
                ],
                vec!["requirement_id", "prerequisite_requirement_ids"],
            ),
        ),
        tool_definition(
            "upsert_requirement_technical_overview",
            "Create or update the implementation technical overview document for a requirement.",
            object_schema(
                vec![
                    string_field("requirement_id", "Requirement id."),
                    optional_string_field("title", "Document title."),
                    optional_string_field("format", "Document format, usually markdown."),
                    string_field("content", "Document content."),
                ],
                vec!["requirement_id", "content"],
            ),
        ),
        tool_definition(
            "get_requirement_technical_overview",
            "Get the implementation technical overview document for a requirement.",
            object_schema(
                vec![string_field("requirement_id", "Requirement id.")],
                vec!["requirement_id"],
            ),
        ),
        tool_definition(
            "list_project_tasks",
            "List project-management tasks/work items for the current project.",
            object_schema(
                vec![
                    enum_field("status", "Optional project task status filter.", project_task_status_values()),
                    optional_string_field("keyword", "Optional fuzzy keyword."),
                ],
                vec![],
            ),
        ),
        tool_definition(
            "create_project_task",
            "Create a project-management task/work item under a requirement. The requirement must already have non-empty technical overview content.",
            object_schema(
                vec![
                    string_field("requirement_id", "Requirement id this project task belongs to."),
                    string_field("title", "Project task title."),
                    optional_string_field("description", "Project task description."),
                    enum_field("status", "Optional project task status.", project_task_status_values()),
                    integer_field("priority", "Optional priority; higher means more important."),
                    optional_string_field("assignee_user_id", "Optional assignee user id."),
                    integer_field("estimate_points", "Optional estimate points."),
                    optional_string_field("due_at", "Optional due time as string."),
                    integer_field("sort_order", "Optional sort order."),
                    string_array_field("tags", "Optional tags."),
                    string_array_field("prerequisite_project_task_ids", "Optional full list of prerequisite project task ids."),
                ],
                vec!["requirement_id", "title"],
            ),
        ),
        tool_definition(
            "update_project_task",
            "Update a project-management task/work item and optionally replace its prerequisite project task ids.",
            object_schema(
                vec![
                    string_field("project_task_id", "Project task/work item id to update."),
                    patch_field("patch", "Fields to update on the project task."),
                    string_array_field("prerequisite_project_task_ids", "Optional full replacement list of prerequisite project task ids."),
                ],
                vec!["project_task_id", "patch"],
            ),
        ),
        tool_definition(
            "set_project_task_dependencies",
            "Replace prerequisite project task ids for one project task.",
            object_schema(
                vec![
                    string_field("project_task_id", "Project task/work item id to update."),
                    string_array_field("prerequisite_project_task_ids", "Full replacement list of prerequisite project task ids."),
                ],
                vec!["project_task_id", "prerequisite_project_task_ids"],
            ),
        ),
        tool_definition(
            "get_project_dependency_graph",
            "Get the current project's dependency graph with requirements, project tasks, contains edges, and blocks edges.",
            object_schema(vec![], vec![]),
        ),
    ]
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

fn enum_field(
    name: &'static str,
    description: &'static str,
    values: Vec<&'static str>,
) -> (&'static str, Value) {
    (
        name,
        json!({
            "type": ["string", "null"],
            "enum": values.into_iter().map(Value::from).chain(std::iter::once(Value::Null)).collect::<Vec<_>>(),
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

fn patch_field(name: &'static str, description: &'static str) -> (&'static str, Value) {
    (
        name,
        json!({
            "type": "object",
            "description": description,
            "additionalProperties": true
        }),
    )
}

fn requirement_status_values() -> Vec<&'static str> {
    vec![
        "draft",
        "reviewing",
        "approved",
        "in_progress",
        "done",
        "cancelled",
        "archived",
    ]
}

fn requirement_type_values() -> Vec<&'static str> {
    vec!["requirement", "change", "bug_fix"]
}

fn project_task_status_values() -> Vec<&'static str> {
    vec![
        "todo",
        "ready",
        "in_progress",
        "blocked",
        "done",
        "cancelled",
        "archived",
    ]
}
