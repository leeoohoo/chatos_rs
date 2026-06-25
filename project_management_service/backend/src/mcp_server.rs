use std::collections::BTreeSet;

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::models::*;
use crate::state::AppState;

const MCP_SERVER_NAME: &str = "project_management_service";
const MCP_ENDPOINT_PATH: &str = "/mcp";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub server_name: String,
    pub transports: Vec<String>,
    pub http_endpoint_path: String,
    pub tool_names: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ToolCallParams {
    name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Debug, Deserialize)]
struct RequirementIdArgs {
    requirement_id: String,
}

#[derive(Debug, Deserialize)]
struct InitProjectArgs {
    name: Option<String>,
    root_path: Option<String>,
    git_url: Option<String>,
    description: Option<String>,
    background: Option<String>,
    introduction: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateRequirementArgs {
    parent_requirement_id: Option<String>,
    requirement_type: Option<RequirementType>,
    title: String,
    summary: Option<String>,
    detail: Option<String>,
    business_value: Option<String>,
    acceptance_criteria: Option<String>,
    source: Option<String>,
    priority: Option<i64>,
    status: Option<RequirementStatus>,
    assignee_user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateRequirementArgs {
    requirement_id: String,
    patch: UpdateRequirementRequest,
    prerequisite_requirement_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct CreateProjectTaskArgs {
    requirement_id: String,
    title: String,
    description: Option<String>,
    status: Option<ProjectWorkItemStatus>,
    priority: Option<i64>,
    assignee_user_id: Option<String>,
    estimate_points: Option<i64>,
    due_at: Option<String>,
    sort_order: Option<i64>,
    tags: Option<Vec<String>>,
    prerequisite_project_task_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct UpdateProjectTaskArgs {
    project_task_id: String,
    patch: UpdateProjectWorkItemRequest,
    prerequisite_project_task_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct SetRequirementDependenciesArgs {
    requirement_id: String,
    prerequisite_requirement_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SetProjectTaskDependenciesArgs {
    project_task_id: String,
    prerequisite_project_task_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UpsertTechnicalOverviewArgs {
    requirement_id: String,
    title: Option<String>,
    format: Option<String>,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ListRequirementsArgs {
    status: Option<RequirementStatus>,
    keyword: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListProjectTasksArgs {
    status: Option<ProjectWorkItemStatus>,
    keyword: Option<String>,
}

pub fn server_info() -> McpServerInfo {
    let tools = tool_definitions();
    let tool_names = tools
        .iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect();
    McpServerInfo {
        server_name: MCP_SERVER_NAME.to_string(),
        transports: vec!["http-jsonrpc".to_string()],
        http_endpoint_path: MCP_ENDPOINT_PATH.to_string(),
        tool_names,
    }
}

pub fn tool_definitions() -> Vec<Value> {
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
                vec![
                    string_field("requirement_id", "Requirement id."),
                ],
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

pub async fn handle_jsonrpc(
    state: AppState,
    current_user: CurrentUser,
    project_id: Option<String>,
    request: JsonRpcRequest,
) -> JsonRpcResponse {
    let id = request.id.clone().unwrap_or(Value::Null);
    let result = match request.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": {
                "name": MCP_SERVER_NAME,
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "tools": {}
            }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => {
            let project_id = match project_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                Some(value) => value,
                None => {
                    return JsonRpcResponse {
                        jsonrpc: "2.0",
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32000,
                            message: "project management MCP requires current project context"
                                .to_string(),
                        }),
                    };
                }
            };
            match decode_value(request.params.unwrap_or_else(|| json!({}))) {
                Ok(params) => call_tool(&state, &current_user, project_id, params).await,
                Err(message) => Err(message),
            }
        }
        method => Err(format!("unsupported MCP method: {method}")),
    };
    match result {
        Ok(result) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        },
        Err(message) => JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32000,
                message,
            }),
        },
    }
}

async fn call_tool(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    params: ToolCallParams,
) -> Result<Value, String> {
    match params.name.as_str() {
        "get_project_overview" => {
            let project = require_project_access(state, project_id, current_user).await?;
            let profile = state
                .store
                .get_project_profile(project_id)
                .await?
                .unwrap_or_else(|| {
                    let now = now_rfc3339();
                    ProjectProfileRecord {
                        project_id: project_id.to_string(),
                        creator_user_id: None,
                        creator_username: None,
                        creator_display_name: None,
                        owner_user_id: None,
                        owner_username: None,
                        owner_display_name: None,
                        background: None,
                        introduction: None,
                        created_at: now.clone(),
                        updated_at: now,
                    }
                });
            Ok(tool_text_result(
                json!({ "project": project, "profile": profile }),
            ))
        }
        "initialize_project" => {
            let args: InitProjectArgs = decode_value(params.arguments)?;
            let project = require_project_access(state, project_id, current_user).await?;
            ensure_project_writable(&project)?;
            let project = state
                .store
                .update_project(
                    project_id,
                    UpdateProjectRequest {
                        name: args.name,
                        root_path: args.root_path,
                        git_url: args.git_url,
                        description: args.description,
                    },
                )
                .await?
                .ok_or_else(|| format!("项目不存在: {project_id}"))?;
            let existing_profile = state.store.get_project_profile(project_id).await?;
            let profile = state
                .store
                .upsert_project_profile(
                    project_id,
                    UpsertProjectProfileRequest {
                        background: args.background.or_else(|| {
                            existing_profile
                                .as_ref()
                                .and_then(|profile| profile.background.clone())
                        }),
                        introduction: args.introduction.or_else(|| {
                            existing_profile
                                .as_ref()
                                .and_then(|profile| profile.introduction.clone())
                        }),
                    },
                    current_user,
                )
                .await?;
            Ok(tool_text_result(
                json!({ "project": project, "profile": profile }),
            ))
        }
        "list_requirements" => {
            let args: ListRequirementsArgs = decode_value(params.arguments)?;
            require_project_access(state, project_id, current_user).await?;
            let requirements = state
                .store
                .list_requirements(project_id, args.status, args.keyword)
                .await?;
            let requirements = visible_requirements_for_mcp(requirements, args.status);
            Ok(tool_text_result(json!(requirements)))
        }
        "create_requirement" => {
            let args: CreateRequirementArgs = decode_value(params.arguments)?;
            let project = require_project_access(state, project_id, current_user).await?;
            ensure_project_writable(&project)?;
            let requirement = state
                .store
                .create_requirement(
                    project_id,
                    CreateRequirementRequest {
                        parent_requirement_id: args.parent_requirement_id,
                        requirement_type: args.requirement_type,
                        title: args.title,
                        summary: args.summary,
                        detail: args.detail,
                        business_value: args.business_value,
                        acceptance_criteria: args.acceptance_criteria,
                        source: args.source,
                        priority: args.priority,
                        status: args.status,
                        assignee_user_id: args.assignee_user_id,
                    },
                    current_user,
                )
                .await?;
            Ok(tool_text_result(json!(requirement)))
        }
        "update_requirement" => {
            let args: UpdateRequirementArgs = decode_value(params.arguments)?;
            let requirement = require_requirement_in_project(
                state,
                &args.requirement_id,
                project_id,
                current_user,
            )
            .await?;
            let project =
                require_project_access(state, &requirement.project_id, current_user).await?;
            ensure_project_writable(&project)?;
            let requirement = state
                .store
                .update_requirement(&args.requirement_id, args.patch)
                .await?
                .ok_or_else(|| format!("需求不存在: {}", args.requirement_id))?;
            let dependencies = if let Some(ids) = args.prerequisite_requirement_ids {
                state
                    .store
                    .set_requirement_dependencies(&args.requirement_id, ids)
                    .await?;
                Some(
                    state
                        .store
                        .list_requirement_dependencies(&args.requirement_id)
                        .await?,
                )
            } else {
                None
            };
            Ok(tool_text_result(json!({
                "requirement": requirement,
                "dependencies": dependencies
            })))
        }
        "set_requirement_dependencies" => {
            let args: SetRequirementDependenciesArgs = decode_value(params.arguments)?;
            let requirement = require_requirement_in_project(
                state,
                &args.requirement_id,
                project_id,
                current_user,
            )
            .await?;
            let project =
                require_project_access(state, &requirement.project_id, current_user).await?;
            ensure_project_writable(&project)?;
            state
                .store
                .set_requirement_dependencies(
                    &args.requirement_id,
                    args.prerequisite_requirement_ids,
                )
                .await?;
            let dependencies = state
                .store
                .list_requirement_dependencies(&args.requirement_id)
                .await?;
            Ok(tool_text_result(json!(dependencies)))
        }
        "upsert_requirement_technical_overview" => {
            let args: UpsertTechnicalOverviewArgs = decode_value(params.arguments)?;
            let requirement = require_requirement_in_project(
                state,
                &args.requirement_id,
                project_id,
                current_user,
            )
            .await?;
            let project =
                require_project_access(state, &requirement.project_id, current_user).await?;
            ensure_project_writable(&project)?;
            let doc = state
                .store
                .upsert_requirement_document(
                    &args.requirement_id,
                    UpsertRequirementDocumentRequest {
                        title: args.title,
                        format: args.format,
                        content: args.content,
                    },
                    current_user,
                )
                .await?;
            Ok(tool_text_result(json!(doc)))
        }
        "get_requirement_technical_overview" => {
            let args: RequirementIdArgs = decode_value(params.arguments)?;
            require_requirement_in_project(state, &args.requirement_id, project_id, current_user)
                .await?;
            let doc = state
                .store
                .get_requirement_document(&args.requirement_id)
                .await?;
            Ok(tool_text_result(json!(doc)))
        }
        "list_project_tasks" => {
            let args: ListProjectTasksArgs = decode_value(params.arguments)?;
            require_project_access(state, project_id, current_user).await?;
            let items = state
                .store
                .list_work_items_by_project(project_id, args.status, args.keyword)
                .await?;
            let items = visible_project_tasks_for_mcp(items, args.status);
            Ok(tool_text_result(json!(items)))
        }
        "create_project_task" => {
            let args: CreateProjectTaskArgs = decode_value(params.arguments)?;
            let requirement = require_requirement_in_project(
                state,
                &args.requirement_id,
                project_id,
                current_user,
            )
            .await?;
            let project =
                require_project_access(state, &requirement.project_id, current_user).await?;
            ensure_project_writable(&project)?;
            let item = state
                .store
                .create_work_item(
                    &requirement,
                    CreateProjectWorkItemRequest {
                        title: args.title,
                        description: args.description,
                        status: args.status,
                        priority: args.priority,
                        assignee_user_id: args.assignee_user_id,
                        estimate_points: args.estimate_points,
                        due_at: args.due_at,
                        sort_order: args.sort_order,
                        tags: args.tags,
                    },
                    current_user,
                )
                .await?;
            let dependencies = if let Some(ids) = args.prerequisite_project_task_ids {
                state
                    .store
                    .set_work_item_dependencies(&item.id, ids)
                    .await?;
                Some(state.store.list_work_item_dependencies(&item.id).await?)
            } else {
                None
            };
            Ok(tool_text_result(json!({
                "project_task": item,
                "dependencies": dependencies
            })))
        }
        "update_project_task" => {
            let args: UpdateProjectTaskArgs = decode_value(params.arguments)?;
            let item = require_project_task_in_project(
                state,
                &args.project_task_id,
                project_id,
                current_user,
            )
            .await?;
            let project = require_project_access(state, &item.project_id, current_user).await?;
            ensure_project_writable(&project)?;
            let item = state
                .store
                .update_work_item(&args.project_task_id, args.patch)
                .await?
                .ok_or_else(|| format!("项目任务不存在: {}", args.project_task_id))?;
            if item.project_id != project_id {
                return Err("项目任务不能移动到其他项目".to_string());
            }
            let dependencies = if let Some(ids) = args.prerequisite_project_task_ids {
                state
                    .store
                    .set_work_item_dependencies(&args.project_task_id, ids)
                    .await?;
                Some(
                    state
                        .store
                        .list_work_item_dependencies(&args.project_task_id)
                        .await?,
                )
            } else {
                None
            };
            Ok(tool_text_result(json!({
                "project_task": item,
                "dependencies": dependencies
            })))
        }
        "set_project_task_dependencies" => {
            let args: SetProjectTaskDependenciesArgs = decode_value(params.arguments)?;
            let item = require_project_task_in_project(
                state,
                &args.project_task_id,
                project_id,
                current_user,
            )
            .await?;
            let project = require_project_access(state, &item.project_id, current_user).await?;
            ensure_project_writable(&project)?;
            state
                .store
                .set_work_item_dependencies(
                    &args.project_task_id,
                    args.prerequisite_project_task_ids,
                )
                .await?;
            let dependencies = state
                .store
                .list_work_item_dependencies(&args.project_task_id)
                .await?;
            Ok(tool_text_result(json!(dependencies)))
        }
        "get_project_dependency_graph" => {
            require_project_access(state, project_id, current_user).await?;
            let graph = build_project_dependency_graph(state, project_id).await?;
            Ok(tool_text_result(json!(graph)))
        }
        name => Err(format!("unknown project management MCP tool: {name}")),
    }
}

async fn require_project_access(
    state: &AppState,
    project_id: &str,
    user: &CurrentUser,
) -> Result<ProjectRecord, String> {
    validate_required("project_id", project_id)?;
    let project = state
        .store
        .get_project(project_id)
        .await?
        .ok_or_else(|| format!("项目不存在: {project_id}"))?;
    if user.can_access_owned_resource(project.owner_user_id.as_deref()) {
        Ok(project)
    } else {
        Err("无权访问该项目".to_string())
    }
}

async fn require_requirement_in_project(
    state: &AppState,
    requirement_id: &str,
    project_id: &str,
    user: &CurrentUser,
) -> Result<RequirementRecord, String> {
    validate_required("project_id", project_id)?;
    validate_required("requirement_id", requirement_id)?;
    let requirement = state
        .store
        .get_requirement(requirement_id)
        .await?
        .ok_or_else(|| format!("需求不存在: {requirement_id}"))?;
    if requirement.project_id != project_id {
        return Err(format!(
            "需求不属于当前项目，requirement_id={requirement_id}"
        ));
    }
    require_project_access(state, project_id, user).await?;
    Ok(requirement)
}

async fn require_project_task_in_project(
    state: &AppState,
    project_task_id: &str,
    project_id: &str,
    user: &CurrentUser,
) -> Result<ProjectWorkItemRecord, String> {
    validate_required("project_id", project_id)?;
    validate_required("project_task_id", project_task_id)?;
    let item = state
        .store
        .get_work_item(project_task_id)
        .await?
        .ok_or_else(|| format!("项目任务不存在: {project_task_id}"))?;
    if item.project_id != project_id {
        return Err(format!(
            "项目任务不属于当前项目，project_task_id={project_task_id}"
        ));
    }
    require_project_access(state, project_id, user).await?;
    Ok(item)
}

fn ensure_project_writable(project: &ProjectRecord) -> Result<(), String> {
    if project.status == ProjectStatus::Archived {
        Err("项目已归档，不能继续写入".to_string())
    } else {
        Ok(())
    }
}

async fn build_project_dependency_graph(
    state: &AppState,
    project_id: &str,
) -> Result<DependencyGraphResponse, String> {
    let requirements = visible_requirements_for_mcp(
        state
            .store
            .list_requirements(project_id, None, None)
            .await?,
        None,
    );
    let work_items = visible_project_tasks_for_mcp(
        state
            .store
            .list_work_items_by_project(project_id, None, None)
            .await?,
        None,
    );
    let requirement_ids = requirements
        .iter()
        .map(|requirement| requirement.id.as_str())
        .collect::<BTreeSet<_>>();
    let work_item_ids = work_items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for requirement in &requirements {
        nodes.push(requirement_node(requirement));
        for dep in state
            .store
            .list_requirement_dependencies(&requirement.id)
            .await?
        {
            if requirement_ids.contains(dep.prerequisite_requirement_id.as_str()) {
                edges.push(DependencyGraphEdge {
                    from: format!("requirement:{}", dep.prerequisite_requirement_id),
                    to: format!("requirement:{}", dep.requirement_id),
                    edge_type: dep.relation_type,
                });
            }
        }
    }
    for item in &work_items {
        nodes.push(work_item_node(item));
        if requirement_ids.contains(item.requirement_id.as_str()) {
            edges.push(DependencyGraphEdge {
                from: format!("requirement:{}", item.requirement_id),
                to: format!("work_item:{}", item.id),
                edge_type: "contains".to_string(),
            });
        }
        for dep in state.store.list_work_item_dependencies(&item.id).await? {
            if work_item_ids.contains(dep.prerequisite_work_item_id.as_str()) {
                edges.push(DependencyGraphEdge {
                    from: format!("work_item:{}", dep.prerequisite_work_item_id),
                    to: format!("work_item:{}", dep.work_item_id),
                    edge_type: dep.relation_type,
                });
            }
        }
    }
    Ok(DependencyGraphResponse {
        root_id: Some(format!("project:{project_id}")),
        nodes,
        edges,
        blocked_by: Vec::new(),
        ready: true,
    })
}

fn visible_requirements_for_mcp(
    requirements: Vec<RequirementRecord>,
    requested_status: Option<RequirementStatus>,
) -> Vec<RequirementRecord> {
    if requested_status.is_some() {
        return requirements;
    }
    requirements
        .into_iter()
        .filter(|requirement| requirement.status != RequirementStatus::Archived)
        .collect()
}

fn visible_project_tasks_for_mcp(
    items: Vec<ProjectWorkItemRecord>,
    requested_status: Option<ProjectWorkItemStatus>,
) -> Vec<ProjectWorkItemRecord> {
    if requested_status.is_some() {
        return items;
    }
    items
        .into_iter()
        .filter(|item| item.status != ProjectWorkItemStatus::Archived)
        .collect()
}

fn requirement_node(requirement: &RequirementRecord) -> DependencyGraphNode {
    DependencyGraphNode {
        id: format!("requirement:{}", requirement.id),
        raw_id: requirement.id.clone(),
        node_type: "requirement".to_string(),
        label: requirement.title.clone(),
        status: requirement.status.as_str().to_string(),
        parent_id: requirement.parent_requirement_id.clone(),
    }
}

fn work_item_node(item: &ProjectWorkItemRecord) -> DependencyGraphNode {
    DependencyGraphNode {
        id: format!("work_item:{}", item.id),
        raw_id: item.id.clone(),
        node_type: "work_item".to_string(),
        label: item.title.clone(),
        status: item.status.as_str().to_string(),
        parent_id: Some(item.requirement_id.clone()),
    }
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

fn decode_value<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|err| err.to_string())
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

impl From<String> for JsonRpcResponse {
    fn from(message: String) -> Self {
        Self {
            jsonrpc: "2.0",
            id: Value::Null,
            result: None,
            error: Some(JsonRpcError {
                code: -32000,
                message,
            }),
        }
    }
}

pub fn jsonrpc_error_response(status: StatusCode, id: Value, message: String) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError {
            code: if status == StatusCode::UNAUTHORIZED {
                -32001
            } else if status == StatusCode::FORBIDDEN {
                -32003
            } else {
                -32000
            },
            message,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn project_scoped_tools_hide_project_context_id() {
        let expected_tools = BTreeSet::from([
            "get_project_overview",
            "initialize_project",
            "list_requirements",
            "create_requirement",
            "update_requirement",
            "set_requirement_dependencies",
            "upsert_requirement_technical_overview",
            "get_requirement_technical_overview",
            "list_project_tasks",
            "create_project_task",
            "update_project_task",
            "set_project_task_dependencies",
            "get_project_dependency_graph",
        ]);

        let tools = tool_definitions();
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<BTreeSet<_>>();
        assert_eq!(names, expected_tools);

        for tool in tools {
            let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
            let properties = tool
                .get("inputSchema")
                .and_then(|schema| schema.get("properties"))
                .and_then(Value::as_object)
                .unwrap_or_else(|| panic!("{name} missing properties object"));
            assert!(
                !properties.contains_key("project_id"),
                "{name} must not expose project_id in input schema"
            );
            let required = tool
                .get("inputSchema")
                .and_then(|schema| schema.get("required"))
                .and_then(Value::as_array)
                .unwrap_or_else(|| panic!("{name} missing required array"));
            assert!(
                !required
                    .iter()
                    .any(|value| value.as_str() == Some("project_id")),
                "{name} must not require project_id"
            );
        }
    }

    #[test]
    fn mcp_lists_hide_archived_items_by_default() {
        let requirements = vec![
            requirement_record("req-active", RequirementStatus::Draft),
            requirement_record("req-archived", RequirementStatus::Archived),
        ];
        let visible_requirements = visible_requirements_for_mcp(requirements.clone(), None);
        assert_eq!(visible_requirements.len(), 1);
        assert_eq!(visible_requirements[0].id, "req-active");

        let archived_requirements =
            visible_requirements_for_mcp(requirements, Some(RequirementStatus::Archived));
        assert_eq!(archived_requirements.len(), 2);

        let items = vec![
            work_item_record("item-active", ProjectWorkItemStatus::Todo),
            work_item_record("item-archived", ProjectWorkItemStatus::Archived),
        ];
        let visible_items = visible_project_tasks_for_mcp(items.clone(), None);
        assert_eq!(visible_items.len(), 1);
        assert_eq!(visible_items[0].id, "item-active");

        let archived_items =
            visible_project_tasks_for_mcp(items, Some(ProjectWorkItemStatus::Archived));
        assert_eq!(archived_items.len(), 2);
    }

    fn requirement_record(id: &str, status: RequirementStatus) -> RequirementRecord {
        RequirementRecord {
            id: id.to_string(),
            project_id: "project-1".to_string(),
            parent_requirement_id: None,
            requirement_type: RequirementType::Requirement,
            title: id.to_string(),
            summary: None,
            detail: None,
            business_value: None,
            acceptance_criteria: None,
            source: None,
            priority: 0,
            status,
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            assignee_user_id: None,
            created_at: "2026-06-24T00:00:00.000Z".to_string(),
            updated_at: "2026-06-24T00:00:00.000Z".to_string(),
            archived_at: None,
        }
    }

    fn work_item_record(id: &str, status: ProjectWorkItemStatus) -> ProjectWorkItemRecord {
        ProjectWorkItemRecord {
            id: id.to_string(),
            project_id: "project-1".to_string(),
            requirement_id: "req-active".to_string(),
            title: id.to_string(),
            description: None,
            status,
            priority: 0,
            assignee_user_id: None,
            estimate_points: None,
            due_at: None,
            sort_order: 0,
            tags: Vec::new(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            created_at: "2026-06-24T00:00:00.000Z".to_string(),
            updated_at: "2026-06-24T00:00:00.000Z".to_string(),
            archived_at: None,
        }
    }
}
