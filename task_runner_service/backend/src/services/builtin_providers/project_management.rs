use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::models::{normalize_project_id, PUBLIC_PROJECT_ID};

#[derive(Clone)]
pub(in crate::services) struct ProjectManagementBuiltinService {
    server_name: String,
    base_url: Option<String>,
    sync_secret: Option<String>,
    owner_user_id: Option<String>,
    project_id: Option<String>,
    execution_options: ProjectManagementExecutionOptions,
}

impl ProjectManagementBuiltinService {
    pub(in crate::services) fn new(options: ProjectManagementOptions) -> Self {
        Self {
            server_name: options.server_name,
            base_url: normalize_optional(options.base_url),
            sync_secret: normalize_optional(options.sync_secret),
            owner_user_id: normalize_optional(options.owner_user_id),
            project_id: normalize_optional(options.project_id),
            execution_options: options.execution_options.unwrap_or_default(),
        }
    }

    pub(in crate::services) fn list_tools(&self) -> Vec<Value> {
        tool_definitions(Some(&self.execution_options))
    }

    pub(in crate::services) async fn call_tool(
        &self,
        name: &str,
        args: Value,
    ) -> Result<Value, String> {
        if let Some(result) = archived_status_short_circuit(name, &args)? {
            return Ok(result);
        }
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
        if let Some(access_token) = crate::auth::get_current_access_token() {
            headers.insert(
                "X-Chatos-User-Authorization".to_string(),
                format!("Bearer {access_token}"),
            );
        }

        let result = chatos_mcp_runtime::jsonrpc_http_call(
            format!("{}/mcp", base_url.trim_end_matches('/')).as_str(),
            Some(&headers),
            "tools/call",
            json!({
                "name": name,
                "arguments": args,
            }),
        )
        .await?;
        filter_archived_tool_result(name, result)
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
        tool_definitions(Some(&self.execution_options))
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
    pub(in crate::services) execution_options: Option<ProjectManagementExecutionOptions>,
}

#[derive(Clone, Debug, Default)]
pub(in crate::services) struct ProjectManagementExecutionOptions {
    pub(in crate::services) model_config_ids: Vec<String>,
    pub(in crate::services) preferred_model_config_id: Option<String>,
    pub(in crate::services) tool_ids: Vec<String>,
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn tool_definitions(execution_options: Option<&ProjectManagementExecutionOptions>) -> Vec<Value> {
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
                    task_runner_model_config_field(execution_options),
                    task_runner_tool_ids_field(execution_options),
                    enum_field("status", "Optional project task status.", project_task_status_values()),
                    integer_field("priority", "Optional priority; higher means more important."),
                    optional_string_field("assignee_user_id", "Optional assignee user id."),
                    integer_field("estimate_points", "Optional estimate points."),
                    optional_string_field("due_at", "Optional due time as string."),
                    integer_field("sort_order", "Optional sort order."),
                    string_array_field("tags", "Optional tags."),
                    string_array_field("prerequisite_project_task_ids", "Optional full list of prerequisite project task ids."),
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

fn task_runner_model_config_field(
    execution_options: Option<&ProjectManagementExecutionOptions>,
) -> (&'static str, Value) {
    let mut schema = json!({
        "type": "string",
        "minLength": 1,
        "description": "Required Task Runner model config id. Use one of the enum values when present; if multiple are available, choose the model best suited for the project task instead of asking the user for an internal id."
    });
    if let Some(options) = execution_options {
        if !options.model_config_ids.is_empty() {
            schema["enum"] = json!(&options.model_config_ids);
        }
        if let Some(default_id) = options.preferred_model_config_id.as_deref() {
            schema["default"] = json!(default_id);
        } else if options.model_config_ids.len() == 1 {
            schema["default"] = json!(options.model_config_ids[0].as_str());
        }
    }
    ("task_runner_default_model_config_id", schema)
}

fn task_runner_tool_ids_field(
    execution_options: Option<&ProjectManagementExecutionOptions>,
) -> (&'static str, Value) {
    let mut item_schema = json!({ "type": "string" });
    let mut description = "Required Task Runner tool id multi-select. Use only visible tool ids. Choose tools according to the work item's execution needs; for code implementation tasks, include appropriate code reading and terminal tools when available."
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

fn requirement_status_values() -> Vec<&'static str> {
    vec![
        "draft",
        "reviewing",
        "approved",
        "in_progress",
        "done",
        "cancelled",
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
    ]
}

fn archived_status_short_circuit(name: &str, args: &Value) -> Result<Option<Value>, String> {
    let status = args.get("status").and_then(Value::as_str);
    let patch_status = args
        .get("patch")
        .and_then(Value::as_object)
        .and_then(|patch| patch.get("status"))
        .and_then(Value::as_str);
    let has_archived_status = status == Some("archived") || patch_status == Some("archived");
    if !has_archived_status {
        return Ok(None);
    }

    match name {
        "list_requirements" | "list_project_tasks" => Ok(Some(tool_text_result(json!([])))),
        "create_requirement" | "update_requirement" => {
            Err("Project Management MCP 不允许访问归档需求".to_string())
        }
        "create_project_task" | "update_project_task" => {
            Err("Project Management MCP 不允许访问归档项目任务".to_string())
        }
        _ => Ok(None),
    }
}

fn filter_archived_tool_result(name: &str, result: Value) -> Result<Value, String> {
    match name {
        "list_requirements" | "list_project_tasks" => {
            transform_tool_text_payload(result, filter_archived_array)
        }
        "get_project_dependency_graph" => {
            transform_tool_text_payload(result, filter_archived_dependency_graph)
        }
        _ => Ok(result),
    }
}

fn transform_tool_text_payload(
    mut result: Value,
    transform: fn(Value) -> Value,
) -> Result<Value, String> {
    let Some(content) = result.get_mut("content").and_then(Value::as_array_mut) else {
        return Ok(result);
    };
    for item in content {
        if item.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        let Some(text) = item.get("text").and_then(Value::as_str) else {
            continue;
        };
        let Ok(payload) = serde_json::from_str::<Value>(text) else {
            continue;
        };
        let filtered = transform(payload);
        item["text"] = Value::String(
            serde_json::to_string_pretty(&filtered).unwrap_or_else(|_| filtered.to_string()),
        );
        break;
    }
    Ok(result)
}

fn filter_archived_array(payload: Value) -> Value {
    let Value::Array(items) = payload else {
        return payload;
    };
    Value::Array(
        items
            .into_iter()
            .filter(|item| item.get("status").and_then(Value::as_str) != Some("archived"))
            .collect(),
    )
}

fn filter_archived_dependency_graph(mut payload: Value) -> Value {
    let Some(object) = payload.as_object_mut() else {
        return payload;
    };
    let nodes = object
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut visible_requirement_ids = HashSet::new();
    for node in &nodes {
        if node.get("status").and_then(Value::as_str) == Some("archived") {
            continue;
        }
        if node.get("node_type").and_then(Value::as_str) == Some("requirement") {
            if let Some(raw_id) = node.get("raw_id").and_then(Value::as_str) {
                visible_requirement_ids.insert(raw_id.to_string());
            }
        }
    }

    let filtered_nodes = nodes
        .into_iter()
        .filter(|node| {
            if node.get("status").and_then(Value::as_str) == Some("archived") {
                return false;
            }
            if node.get("node_type").and_then(Value::as_str) == Some("work_item") {
                return node
                    .get("parent_id")
                    .and_then(Value::as_str)
                    .is_some_and(|parent_id| visible_requirement_ids.contains(parent_id));
            }
            true
        })
        .collect::<Vec<_>>();
    let visible_node_ids = filtered_nodes
        .iter()
        .filter_map(|node| {
            node.get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<HashSet<_>>();
    object.insert("nodes".to_string(), Value::Array(filtered_nodes));

    if let Some(edges) = object.get("edges").and_then(Value::as_array).cloned() {
        object.insert(
            "edges".to_string(),
            Value::Array(
                edges
                    .into_iter()
                    .filter(|edge| {
                        let from_visible = edge
                            .get("from")
                            .and_then(Value::as_str)
                            .is_some_and(|id| visible_node_ids.contains(id));
                        let to_visible = edge
                            .get("to")
                            .and_then(Value::as_str)
                            .is_some_and(|id| visible_node_ids.contains(id));
                        from_visible && to_visible
                    })
                    .collect(),
            ),
        );
    }
    payload
}

fn tool_text_result(payload: Value) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
            }
        ],
        "isError": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_project_task_schema_exposes_execution_options() {
        let execution_options = ProjectManagementExecutionOptions {
            model_config_ids: vec!["model-1".to_string(), "model-2".to_string()],
            preferred_model_config_id: Some("model-2".to_string()),
            tool_ids: vec![
                "CodeMaintainerRead".to_string(),
                "TerminalController".to_string(),
                "external-tool-1".to_string(),
            ],
        };
        let tools = tool_definitions(Some(&execution_options));
        let create_task = tools
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("create_project_task"))
            .expect("create_project_task tool");
        let properties = create_task
            .pointer("/inputSchema/properties")
            .and_then(Value::as_object)
            .expect("properties");

        let model_schema = properties
            .get("task_runner_default_model_config_id")
            .expect("model schema");
        assert_eq!(
            model_schema.get("default").and_then(Value::as_str),
            Some("model-2")
        );
        assert_eq!(
            model_schema
                .get("enum")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            vec![json!("model-1"), json!("model-2")]
        );

        let tool_enum = properties
            .get("task_runner_enabled_tool_ids")
            .and_then(|schema| schema.get("items"))
            .and_then(|items| items.get("enum"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(tool_enum.contains(&json!("CodeMaintainerRead")));
        assert!(tool_enum.contains(&json!("TerminalController")));
        assert!(tool_enum.contains(&json!("external-tool-1")));
    }

    #[test]
    fn status_schemas_do_not_advertise_archived() {
        assert!(!requirement_status_values().contains(&"archived"));
        assert!(!project_task_status_values().contains(&"archived"));
    }

    #[test]
    fn archived_status_queries_are_short_circuited() {
        let result =
            archived_status_short_circuit("list_requirements", &json!({ "status": "archived" }))
                .expect("short circuit")
                .expect("empty result");
        let text = result
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .expect("text result");
        assert_eq!(serde_json::from_str::<Value>(text).unwrap(), json!([]));

        assert!(archived_status_short_circuit(
            "update_project_task",
            &json!({ "patch": { "status": "archived" } }),
        )
        .is_err());
    }

    #[test]
    fn dependency_graph_filter_removes_archived_nodes_and_edges() {
        let graph = json!({
            "root_id": "project:project-1",
            "nodes": [
                {
                    "id": "requirement:req-visible",
                    "node_type": "requirement",
                    "raw_id": "req-visible",
                    "status": "approved"
                },
                {
                    "id": "requirement:req-archived",
                    "node_type": "requirement",
                    "raw_id": "req-archived",
                    "status": "archived"
                },
                {
                    "id": "work_item:item-visible",
                    "node_type": "work_item",
                    "raw_id": "item-visible",
                    "status": "todo",
                    "parent_id": "req-visible"
                },
                {
                    "id": "work_item:item-under-archived",
                    "node_type": "work_item",
                    "raw_id": "item-under-archived",
                    "status": "todo",
                    "parent_id": "req-archived"
                },
                {
                    "id": "work_item:item-archived",
                    "node_type": "work_item",
                    "raw_id": "item-archived",
                    "status": "archived",
                    "parent_id": "req-visible"
                }
            ],
            "edges": [
                {
                    "from": "requirement:req-visible",
                    "to": "work_item:item-visible",
                    "edge_type": "contains"
                },
                {
                    "from": "requirement:req-archived",
                    "to": "work_item:item-under-archived",
                    "edge_type": "contains"
                },
                {
                    "from": "work_item:item-visible",
                    "to": "work_item:item-archived",
                    "edge_type": "blocks"
                }
            ]
        });

        let filtered = filter_archived_dependency_graph(graph);
        let nodes = filtered
            .get("nodes")
            .and_then(Value::as_array)
            .expect("nodes");
        let node_ids = nodes
            .iter()
            .filter_map(|node| node.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(
            node_ids,
            vec!["requirement:req-visible", "work_item:item-visible"]
        );

        let edges = filtered
            .get("edges")
            .and_then(Value::as_array)
            .expect("edges");
        assert_eq!(edges.len(), 1);
        assert_eq!(
            edges[0].get("to").and_then(Value::as_str),
            Some("work_item:item-visible")
        );
    }
}
