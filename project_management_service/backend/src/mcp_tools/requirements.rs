// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::project_management_contract::args::{
    CreateRequirementArgs, ListRequirementsArgs, RequirementIdArgs, SetRequirementDependenciesArgs,
    UpdateRequirementArgs,
};
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::domain::status_policy::{
    ensure_requirement_create_status, ensure_requirement_user_update_status,
};
use crate::domain::visibility::ensure_requirement_status_queryable_for_mcp;
use crate::models::{
    CreateRequirementRequest, RequirementStatus, RequirementType, UpdateRequirementRequest,
};
use crate::state::AppState;

use super::pagination::{mcp_list_page, paginated_list_payload};
use super::{
    decode_value, ensure_project_writable, ensure_requirement_mutable_for_mcp, normalized_optional,
    require_project_access, require_requirement_in_project, tool_text_result,
};

pub(super) async fn list_requirements(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: ListRequirementsArgs = decode_value(arguments)?;
    let status = args.status.map(RequirementStatus::from);
    ensure_requirement_status_queryable_for_mcp(status)?;
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

pub(super) async fn create_requirement(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: CreateRequirementArgs = decode_value(arguments)?;
    let status = args.status.map(RequirementStatus::from);
    ensure_requirement_create_status(status)?;
    let project = require_project_access(state, project_id, current_user).await?;
    ensure_project_writable(&project)?;
    if let Some(parent_requirement_id) = normalized_optional(args.parent_requirement_id.clone()) {
        let parent =
            require_requirement_in_project(state, &parent_requirement_id, project_id, current_user)
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

pub(super) async fn update_requirement(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: UpdateRequirementArgs = decode_value(arguments)?;
    let patch = UpdateRequirementRequest::from(args.patch);
    ensure_requirement_user_update_status(patch.status)?;
    let requirement =
        require_requirement_in_project(state, &args.requirement_id, project_id, current_user)
            .await?;
    let project = require_project_access(state, &requirement.project_id, current_user).await?;
    ensure_project_writable(&project)?;
    ensure_requirement_mutable_for_mcp(&requirement)?;
    if let Some(parent_requirement_id) = normalized_optional(patch.parent_requirement_id.clone()) {
        let parent =
            require_requirement_in_project(state, &parent_requirement_id, project_id, current_user)
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

pub(super) async fn delete_requirement(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: RequirementIdArgs = decode_value(arguments)?;
    let requirement =
        require_requirement_in_project(state, &args.requirement_id, project_id, current_user)
            .await?;
    let project = require_project_access(state, &requirement.project_id, current_user).await?;
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

pub(super) async fn set_requirement_dependencies(
    state: &AppState,
    current_user: &CurrentUser,
    project_id: &str,
    arguments: Value,
) -> Result<Value, String> {
    let args: SetRequirementDependenciesArgs = decode_value(arguments)?;
    let requirement =
        require_requirement_in_project(state, &args.requirement_id, project_id, current_user)
            .await?;
    let project = require_project_access(state, &requirement.project_id, current_user).await?;
    ensure_project_writable(&project)?;
    ensure_requirement_mutable_for_mcp(&requirement)?;
    state
        .store
        .set_requirement_dependencies(&args.requirement_id, args.prerequisite_requirement_ids)
        .await?;
    let dependencies = state
        .store
        .list_requirement_dependencies(&args.requirement_id)
        .await?;
    Ok(tool_text_result(json!(dependencies)))
}
