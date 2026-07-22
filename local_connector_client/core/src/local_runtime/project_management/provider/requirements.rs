// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_mcp::project_management_contract::args::{
    CreateRequirementArgs, ListRequirementsArgs, RequirementIdArgs, SetRequirementDependenciesArgs,
    UpdateRequirementArgs,
};
use serde_json::{json, Value};

use crate::local_runtime::project_management::{
    CreateLocalRequirementInput, UpdateLocalRequirementInput,
};

use super::requirement_support::{
    matches_keyword, require_mutable, required_text, requirement_status, requirement_type,
};
use super::{decode, normalized, page, LocalProjectManagementProvider};

pub(super) async fn list(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: ListRequirementsArgs = decode(arguments)?;
    let status = args.status.map(requirement_status);
    let keyword = normalized(args.keyword).map(|value| value.to_lowercase());
    let records = provider
        .database
        .list_local_requirements(
            provider.owner_user_id.as_str(),
            provider.project_id.as_str(),
            false,
        )
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|record| {
            status
                .as_deref()
                .is_none_or(|status| record.status == status)
        })
        .filter(|record| {
            keyword
                .as_deref()
                .is_none_or(|keyword| matches_keyword(record, keyword))
        })
        .collect::<Vec<_>>();
    let total = records.len();
    let (items, has_more) = page(records, args.limit, args.offset);
    Ok(json!({ "items": items, "total": total, "has_more": has_more }))
}

pub(super) async fn create(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: CreateRequirementArgs = decode(arguments)?;
    if let Some(parent_id) = normalized(args.parent_requirement_id.clone()) {
        require_mutable(provider, parent_id.as_str()).await?;
    }
    let record = provider
        .database
        .create_local_requirement(CreateLocalRequirementInput {
            project_id: provider.project_id.clone(),
            owner_user_id: provider.owner_user_id.clone(),
            parent_requirement_id: normalized(args.parent_requirement_id),
            requirement_type: args
                .requirement_type
                .map(requirement_type)
                .unwrap_or_else(|| "requirement".to_string()),
            title: required_text(args.title, "title")?,
            summary: normalized(args.summary),
            detail: normalized(args.detail),
            business_value: normalized(args.business_value),
            acceptance_criteria: normalized(args.acceptance_criteria),
            source: normalized(args.source),
            priority: args.priority.unwrap_or_default().clamp(-100, 100),
            status: args
                .status
                .map(requirement_status)
                .unwrap_or_else(|| "draft".to_string()),
            assignee_user_id: normalized(args.assignee_user_id),
        })
        .await
        .map_err(|error| error.to_string())?;
    serde_json::to_value(record).map_err(|error| error.to_string())
}

pub(super) async fn update(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: UpdateRequirementArgs = decode(arguments)?;
    require_mutable(provider, args.requirement_id.as_str()).await?;
    if let Some(parent_id) = normalized(args.patch.parent_requirement_id.clone()) {
        require_mutable(provider, parent_id.as_str()).await?;
    }
    let record = provider
        .database
        .update_local_requirement(
            provider.owner_user_id.as_str(),
            args.requirement_id.as_str(),
            UpdateLocalRequirementInput {
                parent_requirement_id: normalized(args.patch.parent_requirement_id),
                requirement_type: args.patch.requirement_type.map(requirement_type),
                title: normalized(args.patch.title),
                summary: normalized(args.patch.summary),
                detail: normalized(args.patch.detail),
                business_value: normalized(args.patch.business_value),
                acceptance_criteria: normalized(args.patch.acceptance_criteria),
                source: normalized(args.patch.source),
                priority: args.patch.priority.map(|value| value.clamp(-100, 100)),
                status: args.patch.status.map(requirement_status),
                assignee_user_id: normalized(args.patch.assignee_user_id),
            },
        )
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "local requirement was not found".to_string())?;
    let dependencies = if let Some(ids) = args.prerequisite_requirement_ids {
        Some(
            provider
                .database
                .set_local_requirement_dependencies(
                    provider.owner_user_id.as_str(),
                    provider.project_id.as_str(),
                    args.requirement_id.as_str(),
                    ids,
                )
                .await
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    Ok(json!({ "requirement": record, "dependencies": dependencies }))
}

pub(super) async fn archive(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: RequirementIdArgs = decode(arguments)?;
    require_mutable(provider, args.requirement_id.as_str()).await?;
    let record = provider
        .database
        .archive_local_requirement(
            provider.owner_user_id.as_str(),
            args.requirement_id.as_str(),
        )
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "local requirement was not found".to_string())?;
    Ok(json!({ "deleted_requirement": record }))
}

pub(super) async fn set_dependencies(
    provider: &LocalProjectManagementProvider,
    arguments: Value,
) -> Result<Value, String> {
    let args: SetRequirementDependenciesArgs = decode(arguments)?;
    require_mutable(provider, args.requirement_id.as_str()).await?;
    let records = provider
        .database
        .set_local_requirement_dependencies(
            provider.owner_user_id.as_str(),
            provider.project_id.as_str(),
            args.requirement_id.as_str(),
            args.prerequisite_requirement_ids,
        )
        .await
        .map_err(|error| error.to_string())?;
    serde_json::to_value(records).map_err(|error| error.to_string())
}
