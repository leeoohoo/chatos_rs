// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_project_mcp_contract::{
    args::{
        CreateProjectTaskArgs, CreateRequirementArgs, InitProjectArgs, ListProjectTasksArgs,
        ListRequirementTechnicalDocumentsArgs, ListRequirementsArgs, ProjectTaskIdArgs,
        ProjectTaskStatus as McpProjectTaskStatus, RequirementIdArgs,
        RequirementStatus as McpRequirementStatus, RequirementTechnicalDocumentIdArgs,
        RequirementType as McpRequirementType, SetProjectTaskDependenciesArgs,
        SetRequirementDependenciesArgs, ToolCallParams, UpdateProjectTaskArgs,
        UpdateProjectTaskPatch, UpdateRequirementArgs, UpdateRequirementPatch,
        UpsertRequirementTechnicalDocumentArgs,
    },
    tools,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::domain::visibility::{
    ensure_project_task_queryable_for_mcp, ensure_project_task_status_queryable_for_mcp,
    ensure_requirement_queryable_for_mcp, ensure_requirement_status_queryable_for_mcp,
};
use crate::models::*;
use crate::services::dependency_graph;
use crate::state::AppState;
use crate::task_runner_api_client;

const DEFAULT_MCP_LIST_LIMIT: usize = 50;
const MAX_MCP_LIST_LIMIT: usize = 100;

#[derive(Debug, Clone, Copy)]
struct McpListPageRequest {
    limit: usize,
    offset: usize,
}

#[derive(Debug, Serialize)]
struct McpListPageMeta {
    limit: usize,
    offset: usize,
    returned: usize,
    has_more: bool,
    next_offset: Option<usize>,
}

pub(crate) async fn call_tool_from_value(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    value: Value,
) -> Result<Value, String> {
    let params: ToolCallParams = decode_value(value)?;
    call_tool(state, current_user, project_id, params).await
}

async fn call_tool(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    params: ToolCallParams,
) -> Result<Value, String> {
    match params.name.as_str() {
        tools::GET_PROJECT_OVERVIEW => {
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
        tools::INITIALIZE_PROJECT => {
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
        tools::LIST_REQUIREMENTS => {
            let args: ListRequirementsArgs = decode_value(params.arguments)?;
            let status = args.status.map(RequirementStatus::from);
            let page = mcp_list_page(args.limit, args.offset);
            require_project_access(state, project_id, current_user).await?;
            let mut requirements = state
                .store
                .list_requirements_page(
                    project_id,
                    status,
                    args.keyword,
                    false,
                    page.fetch_limit(),
                    page.offset,
                )
                .await?;
            let has_more = requirements.len() > page.limit;
            if has_more {
                requirements.truncate(page.limit);
            }
            Ok(tool_text_result(paginated_list_payload(
                requirements,
                page,
                has_more,
            )))
        }
        tools::CREATE_REQUIREMENT => {
            let args: CreateRequirementArgs = decode_value(params.arguments)?;
            let status = args.status.map(RequirementStatus::from);
            ensure_requirement_status_queryable_for_mcp(status)?;
            let project = require_project_access(state, project_id, current_user).await?;
            ensure_project_writable(&project)?;
            if let Some(parent_requirement_id) =
                normalized_optional(args.parent_requirement_id.clone())
            {
                let parent = require_requirement_in_project(
                    state,
                    &parent_requirement_id,
                    project_id,
                    current_user,
                )
                .await?;
                ensure_requirement_mutable_for_mcp(&parent)?;
            }
            let requirement = state
                .store
                .create_requirement(
                    project_id,
                    CreateRequirementRequest {
                        parent_requirement_id: args.parent_requirement_id,
                        requirement_type: args.requirement_type.map(RequirementType::from),
                        title: args.title,
                        summary: args.summary,
                        detail: args.detail,
                        business_value: args.business_value,
                        acceptance_criteria: args.acceptance_criteria,
                        source: args.source,
                        priority: args.priority,
                        status,
                        assignee_user_id: args.assignee_user_id,
                    },
                    current_user,
                )
                .await?;
            Ok(tool_text_result(json!(requirement)))
        }
        tools::UPDATE_REQUIREMENT => {
            let args: UpdateRequirementArgs = decode_value(params.arguments)?;
            let patch = UpdateRequirementRequest::from(args.patch);
            ensure_requirement_status_queryable_for_mcp(patch.status)?;
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
            ensure_requirement_mutable_for_mcp(&requirement)?;
            if let Some(parent_requirement_id) =
                normalized_optional(patch.parent_requirement_id.clone())
            {
                let parent = require_requirement_in_project(
                    state,
                    &parent_requirement_id,
                    project_id,
                    current_user,
                )
                .await?;
                ensure_requirement_mutable_for_mcp(&parent)?;
            }
            let requirement = state
                .store
                .update_requirement(&args.requirement_id, patch)
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
        tools::DELETE_REQUIREMENT => {
            let args: RequirementIdArgs = decode_value(params.arguments)?;
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
            ensure_requirement_mutable_for_mcp(&requirement)?;
            let deleted = state
                .store
                .delete_requirement(&args.requirement_id)
                .await?
                .ok_or_else(|| format!("需求不存在: {}", args.requirement_id))?;
            Ok(tool_text_result(json!({
                "deleted_requirement": deleted
            })))
        }
        tools::SET_REQUIREMENT_DEPENDENCIES => {
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
            ensure_requirement_mutable_for_mcp(&requirement)?;
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
        tools::LIST_REQUIREMENT_TECHNICAL_DOCUMENTS => {
            let args: ListRequirementTechnicalDocumentsArgs = decode_value(params.arguments)?;
            require_requirement_in_project(state, &args.requirement_id, project_id, current_user)
                .await?;
            let docs = state
                .store
                .list_requirement_documents(&args.requirement_id, args.doc_type)
                .await?;
            Ok(tool_text_result(json!(docs)))
        }
        tools::GET_REQUIREMENT_TECHNICAL_DOCUMENT => {
            let args: RequirementTechnicalDocumentIdArgs = decode_value(params.arguments)?;
            require_requirement_in_project(state, &args.requirement_id, project_id, current_user)
                .await?;
            let doc = state
                .store
                .get_requirement_document_by_id(&args.requirement_id, &args.document_id)
                .await?
                .ok_or_else(|| format!("需求技术文档不存在: {}", args.document_id))?;
            Ok(tool_text_result(json!(doc)))
        }
        tools::UPSERT_REQUIREMENT_TECHNICAL_DOCUMENT => {
            let args: UpsertRequirementTechnicalDocumentArgs = decode_value(params.arguments)?;
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
            ensure_requirement_mutable_for_mcp(&requirement)?;
            let doc = if let Some(document_id) = normalized_optional(args.document_id) {
                state
                    .store
                    .update_requirement_document(
                        &args.requirement_id,
                        &document_id,
                        UpdateRequirementDocumentRequest {
                            doc_type: args.doc_type,
                            title: args.title,
                            format: args.format,
                            content: Some(args.content),
                        },
                    )
                    .await?
            } else {
                state
                    .store
                    .create_requirement_document(
                        &args.requirement_id,
                        UpsertRequirementDocumentRequest {
                            doc_type: args.doc_type,
                            title: args.title,
                            format: args.format,
                            content: args.content,
                        },
                        current_user,
                    )
                    .await?
            };
            Ok(tool_text_result(json!(doc)))
        }
        tools::LIST_PROJECT_TASKS => {
            let args: ListProjectTasksArgs = decode_value(params.arguments)?;
            let status = args.status.map(ProjectWorkItemStatus::from);
            let page = mcp_list_page(args.limit, args.offset);
            require_project_access(state, project_id, current_user).await?;
            let requirement_id = normalized_optional(args.requirement_id);
            if let Some(requirement_id) = requirement_id.as_deref() {
                require_requirement_in_project(state, requirement_id, project_id, current_user)
                    .await?;
            }
            let mut items = state
                .store
                .list_work_items_by_project_page(
                    project_id,
                    status,
                    args.keyword,
                    requirement_id,
                    args.is_planning_task,
                    false,
                    page.fetch_limit(),
                    page.offset,
                )
                .await?;
            let has_more = items.len() > page.limit;
            if has_more {
                items.truncate(page.limit);
            }
            let items = dependency_graph::retain_project_tasks_with_visible_requirements(
                &state.store,
                project_id,
                items,
            )
            .await?;
            Ok(tool_text_result(paginated_list_payload(
                items, page, has_more,
            )))
        }
        tools::CREATE_PROJECT_TASK => {
            let args: CreateProjectTaskArgs = decode_value(params.arguments)?;
            let status = args.status.map(ProjectWorkItemStatus::from);
            ensure_project_task_status_queryable_for_mcp(status)?;
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
            ensure_requirement_mutable_for_mcp(&requirement)?;
            let owner_user_id = current_user.effective_owner_user_id().ok_or_else(|| {
                "project management MCP create_project_task requires owner user id for Task Runner model/tool validation".to_string()
            })?;
            let execution_options =
                task_runner_api_client::fetch_execution_options(&state.config, owner_user_id)
                    .await?;
            let task_runner_default_model_config_id = execution_options
                .validate_model_config_id(args.task_runner_default_model_config_id.as_str())?;
            let task_runner_enabled_tool_ids =
                task_runner_api_client::normalize_tool_ids(args.task_runner_enabled_tool_ids)?;
            let _ = execution_options.mcp_config_for_tool_ids(&task_runner_enabled_tool_ids)?;
            let task_runner_skill_ids =
                execution_options.validate_skill_ids(args.task_runner_skill_ids)?;
            let item = state
                .store
                .create_work_item(
                    &requirement,
                    CreateProjectWorkItemRequest {
                        title: args.title,
                        description: args.description,
                        task_runner_default_model_config_id,
                        task_runner_enabled_tool_ids,
                        task_runner_skill_ids,
                        status,
                        priority: args.priority,
                        assignee_user_id: args.assignee_user_id,
                        estimate_points: args.estimate_points,
                        due_at: args.due_at,
                        sort_order: args.sort_order,
                        tags: args.tags,
                        is_planning_task: args.is_planning_task,
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
        tools::UPDATE_PROJECT_TASK => {
            let args: UpdateProjectTaskArgs = decode_value(params.arguments)?;
            let patch = UpdateProjectWorkItemRequest::from(args.patch);
            ensure_project_task_status_queryable_for_mcp(patch.status)?;
            if let Some(requirement_id) = normalized_optional(patch.requirement_id.clone()) {
                let target_requirement = require_requirement_in_project(
                    state,
                    &requirement_id,
                    project_id,
                    current_user,
                )
                .await?;
                ensure_requirement_mutable_for_mcp(&target_requirement)?;
            }
            let item = require_project_task_in_project(
                state,
                &args.project_task_id,
                project_id,
                current_user,
            )
            .await?;
            ensure_project_task_mutable_for_mcp(&item)?;
            let current_requirement = require_requirement_in_project(
                state,
                &item.requirement_id,
                project_id,
                current_user,
            )
            .await?;
            ensure_requirement_mutable_for_mcp(&current_requirement)?;
            let project = require_project_access(state, &item.project_id, current_user).await?;
            ensure_project_writable(&project)?;
            let item = state
                .store
                .update_work_item(&args.project_task_id, patch)
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
        tools::DELETE_PROJECT_TASK => {
            let args: ProjectTaskIdArgs = decode_value(params.arguments)?;
            let item = require_project_task_in_project(
                state,
                &args.project_task_id,
                project_id,
                current_user,
            )
            .await?;
            ensure_project_task_mutable_for_mcp(&item)?;
            let requirement = require_requirement_in_project(
                state,
                &item.requirement_id,
                project_id,
                current_user,
            )
            .await?;
            ensure_requirement_mutable_for_mcp(&requirement)?;
            let project = require_project_access(state, &item.project_id, current_user).await?;
            ensure_project_writable(&project)?;
            let deleted = state
                .store
                .delete_work_item(&args.project_task_id)
                .await?
                .ok_or_else(|| format!("项目任务不存在: {}", args.project_task_id))?;
            Ok(tool_text_result(json!({
                "deleted_project_task": deleted
            })))
        }
        tools::SET_PROJECT_TASK_DEPENDENCIES => {
            let args: SetProjectTaskDependenciesArgs = decode_value(params.arguments)?;
            let item = require_project_task_in_project(
                state,
                &args.project_task_id,
                project_id,
                current_user,
            )
            .await?;
            ensure_project_task_mutable_for_mcp(&item)?;
            let requirement = require_requirement_in_project(
                state,
                &item.requirement_id,
                project_id,
                current_user,
            )
            .await?;
            ensure_requirement_mutable_for_mcp(&requirement)?;
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
        tools::GET_PROJECT_DEPENDENCY_GRAPH => {
            require_project_access(state, project_id, current_user).await?;
            let graph =
                dependency_graph::project_dependency_graph(&state.store, project_id, false).await?;
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
    ensure_requirement_queryable_for_mcp(&requirement)?;
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
    ensure_project_task_queryable_for_mcp(&item)?;
    let requirement = state
        .store
        .get_requirement(&item.requirement_id)
        .await?
        .ok_or_else(|| format!("项目任务不存在: {project_task_id}"))?;
    if requirement.project_id != project_id {
        return Err(format!("项目任务不存在: {project_task_id}"));
    }
    if ensure_requirement_queryable_for_mcp(&requirement).is_err() {
        return Err(format!("项目任务不存在: {project_task_id}"));
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

fn ensure_requirement_mutable_for_mcp(requirement: &RequirementRecord) -> Result<(), String> {
    if requirement.status == RequirementStatus::Done {
        Err(format!(
            "需求已完成，不能通过 MCP 修改、删除或追加内容；如有相似的新需求，请新建需求。requirement_id={}",
            requirement.id
        ))
    } else {
        Ok(())
    }
}

fn ensure_project_task_mutable_for_mcp(item: &ProjectWorkItemRecord) -> Result<(), String> {
    if item.status == ProjectWorkItemStatus::Done {
        Err(format!(
            "项目任务已完成，不能通过 MCP 修改、删除或调整依赖；如有相似的新工作，请新建项目任务。project_task_id={}",
            item.id
        ))
    } else {
        Ok(())
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

impl McpListPageRequest {
    fn fetch_limit(&self) -> usize {
        self.limit.saturating_add(1)
    }
}

fn mcp_list_page(limit: Option<usize>, offset: Option<usize>) -> McpListPageRequest {
    McpListPageRequest {
        limit: limit
            .unwrap_or(DEFAULT_MCP_LIST_LIMIT)
            .clamp(1, MAX_MCP_LIST_LIMIT),
        offset: offset.unwrap_or_default(),
    }
}

fn paginated_list_payload<T: Serialize>(
    items: Vec<T>,
    page: McpListPageRequest,
    has_more: bool,
) -> Value {
    let returned = items.len();
    json!({
        "items": items,
        "page": McpListPageMeta {
            limit: page.limit,
            offset: page.offset,
            returned,
            has_more,
            next_offset: has_more.then_some(page.offset.saturating_add(page.limit)),
        }
    })
}

fn decode_value<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|err| err.to_string())
}

impl From<McpRequirementStatus> for RequirementStatus {
    fn from(value: McpRequirementStatus) -> Self {
        match value {
            McpRequirementStatus::Draft => Self::Draft,
            McpRequirementStatus::Reviewing => Self::Reviewing,
            McpRequirementStatus::Approved => Self::Approved,
            McpRequirementStatus::InProgress => Self::InProgress,
            McpRequirementStatus::Blocked => Self::Blocked,
            McpRequirementStatus::Failed => Self::Failed,
            McpRequirementStatus::Done => Self::Done,
            McpRequirementStatus::Cancelled => Self::Cancelled,
            McpRequirementStatus::Archived => Self::Archived,
        }
    }
}

impl From<McpRequirementType> for RequirementType {
    fn from(value: McpRequirementType) -> Self {
        match value {
            McpRequirementType::Requirement => Self::Requirement,
            McpRequirementType::Change => Self::Change,
            McpRequirementType::BugFix => Self::BugFix,
        }
    }
}

impl From<McpProjectTaskStatus> for ProjectWorkItemStatus {
    fn from(value: McpProjectTaskStatus) -> Self {
        match value {
            McpProjectTaskStatus::Todo => Self::Todo,
            McpProjectTaskStatus::Ready => Self::Ready,
            McpProjectTaskStatus::InProgress => Self::InProgress,
            McpProjectTaskStatus::Blocked => Self::Blocked,
            McpProjectTaskStatus::Failed => Self::Failed,
            McpProjectTaskStatus::Done => Self::Done,
            McpProjectTaskStatus::Cancelled => Self::Cancelled,
            McpProjectTaskStatus::Archived => Self::Archived,
        }
    }
}

impl From<UpdateRequirementPatch> for UpdateRequirementRequest {
    fn from(value: UpdateRequirementPatch) -> Self {
        Self {
            parent_requirement_id: value.parent_requirement_id,
            requirement_type: value.requirement_type.map(RequirementType::from),
            title: value.title,
            summary: value.summary,
            detail: value.detail,
            business_value: value.business_value,
            acceptance_criteria: value.acceptance_criteria,
            source: value.source,
            priority: value.priority,
            status: value.status.map(RequirementStatus::from),
            assignee_user_id: value.assignee_user_id,
        }
    }
}

impl From<UpdateProjectTaskPatch> for UpdateProjectWorkItemRequest {
    fn from(value: UpdateProjectTaskPatch) -> Self {
        Self {
            requirement_id: value.requirement_id,
            title: value.title,
            description: value.description,
            status: value.status.map(ProjectWorkItemStatus::from),
            priority: value.priority,
            assignee_user_id: value.assignee_user_id,
            estimate_points: value.estimate_points,
            due_at: value.due_at,
            sort_order: value.sort_order,
            tags: value.tags,
            is_planning_task: value.is_planning_task,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::config::AppConfig;
    use crate::models::UserRole;

    async fn test_state() -> AppState {
        let database_path = std::env::temp_dir().join(format!(
            "project-management-mcp-tools-{}.db",
            Uuid::new_v4()
        ));
        AppState::new(AppConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            database_url: format!("sqlite://{}", database_path.display()),
            user_service_base_url: "http://127.0.0.1:1".to_string(),
            user_service_request_timeout: Duration::from_millis(300),
            task_runner_base_url: None,
            task_runner_request_timeout: Duration::from_millis(300),
            task_runner_internal_secret: None,
            sync_secret: None,
        })
        .await
        .expect("test state")
    }

    fn test_user() -> CurrentUser {
        CurrentUser {
            principal_type: "human_user".to_string(),
            id: "user-1".to_string(),
            username: "owner".to_string(),
            display_name: "Owner".to_string(),
            role: UserRole::Agent,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
        }
    }

    async fn create_project(state: &AppState, user: &CurrentUser) -> ProjectRecord {
        state
            .store
            .create_project(
                CreateProjectRequest {
                    name: "Project".to_string(),
                    root_path: None,
                    git_url: None,
                    description: None,
                    source_type: None,
                    cloud_import_source: None,
                    import_status: None,
                    source_git_url: None,
                },
                user,
            )
            .await
            .expect("create project")
    }

    async fn create_requirement(
        state: &AppState,
        user: &CurrentUser,
        project_id: &str,
        title: &str,
    ) -> RequirementRecord {
        state
            .store
            .create_requirement(
                project_id,
                CreateRequirementRequest {
                    parent_requirement_id: None,
                    requirement_type: None,
                    title: title.to_string(),
                    summary: None,
                    detail: None,
                    business_value: None,
                    acceptance_criteria: None,
                    source: None,
                    priority: None,
                    status: None,
                    assignee_user_id: None,
                },
                user,
            )
            .await
            .expect("create requirement")
    }

    async fn add_technical_document(state: &AppState, user: &CurrentUser, requirement_id: &str) {
        state
            .store
            .create_requirement_document(
                requirement_id,
                UpsertRequirementDocumentRequest {
                    doc_type: None,
                    title: None,
                    format: None,
                    content: "Technical overview".to_string(),
                },
                user,
            )
            .await
            .expect("create requirement document");
    }

    async fn create_project_task(
        state: &AppState,
        user: &CurrentUser,
        requirement: &RequirementRecord,
    ) -> ProjectWorkItemRecord {
        add_technical_document(state, user, &requirement.id).await;
        state
            .store
            .create_work_item(
                requirement,
                CreateProjectWorkItemRequest {
                    title: "Full Maven build".to_string(),
                    description: None,
                    task_runner_default_model_config_id: "model-config-test".to_string(),
                    task_runner_enabled_tool_ids: vec!["filesystem".to_string()],
                    task_runner_skill_ids: Vec::new(),
                    status: None,
                    priority: None,
                    assignee_user_id: None,
                    estimate_points: None,
                    due_at: None,
                    sort_order: None,
                    tags: None,
                    is_planning_task: false,
                },
                user,
            )
            .await
            .expect("create work item")
    }

    async fn call_test_tool(
        state: &AppState,
        user: &CurrentUser,
        project_id: &str,
        name: &str,
        arguments: Value,
    ) -> Result<Value, String> {
        call_tool(
            state,
            user,
            project_id,
            ToolCallParams {
                name: name.to_string(),
                arguments,
            },
        )
        .await
    }

    fn assert_done_record_rejected(result: Result<Value, String>) {
        let error = result.expect_err("done record mutation rejected");
        assert!(
            error.contains("已完成") || error.contains("done"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn mcp_rejects_mutating_done_requirement() {
        let state = test_state().await;
        let user = test_user();
        let project = create_project(&state, &user).await;
        let requirement = create_requirement(&state, &user, &project.id, "Requirement").await;
        state
            .store
            .update_requirement(
                &requirement.id,
                UpdateRequirementRequest {
                    status: Some(RequirementStatus::Done),
                    ..Default::default()
                },
            )
            .await
            .expect("mark requirement done");

        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::UPDATE_REQUIREMENT,
                json!({
                    "requirement_id": requirement.id,
                    "patch": { "title": "Changed" }
                }),
            )
            .await,
        );
        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::UPSERT_REQUIREMENT_TECHNICAL_DOCUMENT,
                json!({
                    "requirement_id": requirement.id,
                    "content": "Updated technical overview"
                }),
            )
            .await,
        );
        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::CREATE_PROJECT_TASK,
                json!({
                    "requirement_id": requirement.id,
                    "title": "Full Maven build",
                    "task_runner_default_model_config_id": "model-config-test",
                    "task_runner_enabled_tool_ids": ["filesystem"]
                }),
            )
            .await,
        );
        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::DELETE_REQUIREMENT,
                json!({ "requirement_id": requirement.id }),
            )
            .await,
        );
    }

    #[tokio::test]
    async fn mcp_rejects_attaching_new_records_to_done_requirement() {
        let state = test_state().await;
        let user = test_user();
        let project = create_project(&state, &user).await;
        let done_parent = create_requirement(&state, &user, &project.id, "Done parent").await;
        let child = create_requirement(&state, &user, &project.id, "Open child").await;
        state
            .store
            .update_requirement(
                &done_parent.id,
                UpdateRequirementRequest {
                    status: Some(RequirementStatus::Done),
                    ..Default::default()
                },
            )
            .await
            .expect("mark parent done");

        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::CREATE_REQUIREMENT,
                json!({
                    "parent_requirement_id": done_parent.id,
                    "title": "New child under done parent"
                }),
            )
            .await,
        );
        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::UPDATE_REQUIREMENT,
                json!({
                    "requirement_id": child.id,
                    "patch": { "parent_requirement_id": done_parent.id }
                }),
            )
            .await,
        );
    }

    #[tokio::test]
    async fn mcp_rejects_mutating_done_project_task() {
        let state = test_state().await;
        let user = test_user();
        let project = create_project(&state, &user).await;
        let requirement = create_requirement(&state, &user, &project.id, "Requirement").await;
        let item = create_project_task(&state, &user, &requirement).await;
        state
            .store
            .update_work_item(
                &item.id,
                UpdateProjectWorkItemRequest {
                    status: Some(ProjectWorkItemStatus::Done),
                    ..Default::default()
                },
            )
            .await
            .expect("mark work item done");

        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::UPDATE_PROJECT_TASK,
                json!({
                    "project_task_id": item.id,
                    "patch": { "title": "Changed" }
                }),
            )
            .await,
        );
        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::SET_PROJECT_TASK_DEPENDENCIES,
                json!({
                    "project_task_id": item.id,
                    "prerequisite_project_task_ids": []
                }),
            )
            .await,
        );
        assert_done_record_rejected(
            call_test_tool(
                &state,
                &user,
                &project.id,
                tools::DELETE_PROJECT_TASK,
                json!({ "project_task_id": item.id }),
            )
            .await,
        );
    }
}
