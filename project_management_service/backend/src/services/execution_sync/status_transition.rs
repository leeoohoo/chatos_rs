// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use crate::models::{
    ProjectWorkItemRecord, ProjectWorkItemStatus, RequirementRecord, RequirementStatus,
    UpdateRequirementRequest,
};
use crate::store::AppStore;

use super::ExecutionSyncError;

pub(super) async fn fail_related_requirements_if_work_item_failed(
    store: &AppStore,
    work_item: &ProjectWorkItemRecord,
) -> Result<Vec<RequirementRecord>, ExecutionSyncError> {
    if work_item.status != ProjectWorkItemStatus::Failed {
        return Ok(Vec::new());
    }

    update_related_requirements_from_work_item(
        store,
        work_item,
        RequirementStatus::Failed,
        requirement_status_can_fail_from_work_items,
    )
    .await
}

pub(super) async fn block_related_requirements_if_work_item_blocked(
    store: &AppStore,
    work_item: &ProjectWorkItemRecord,
) -> Result<Vec<RequirementRecord>, ExecutionSyncError> {
    if work_item.status != ProjectWorkItemStatus::Blocked {
        return Ok(Vec::new());
    }

    update_related_requirements_from_work_item(
        store,
        work_item,
        RequirementStatus::Blocked,
        requirement_status_can_block_from_work_items,
    )
    .await
}

async fn update_related_requirements_from_work_item(
    store: &AppStore,
    work_item: &ProjectWorkItemRecord,
    next_status: RequirementStatus,
    can_update: fn(RequirementStatus) -> bool,
) -> Result<Vec<RequirementRecord>, ExecutionSyncError> {
    let requirements = store
        .list_requirements(&work_item.project_id, None, None)
        .await
        .map_err(ExecutionSyncError::bad_request)?;
    let requirement_by_id = requirements
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut updated_requirements = Vec::new();
    let mut seen = HashSet::new();
    let mut current_id = Some(work_item.requirement_id.as_str());
    while let Some(requirement_id) = current_id {
        if !seen.insert(requirement_id.to_string()) {
            break;
        }
        let Some(requirement) = requirement_by_id.get(requirement_id) else {
            break;
        };
        current_id = requirement.parent_requirement_id.as_deref();
        if !can_update(requirement.status) {
            continue;
        }
        if let Some(updated_requirement) = store
            .update_requirement(
                requirement.id.as_str(),
                UpdateRequirementRequest {
                    status: Some(next_status),
                    ..UpdateRequirementRequest::default()
                },
            )
            .await
            .map_err(ExecutionSyncError::bad_request)?
        {
            updated_requirements.push(updated_requirement);
        }
    }

    Ok(updated_requirements)
}

pub(super) async fn complete_related_requirements_if_work_items_done(
    store: &AppStore,
    work_item: &ProjectWorkItemRecord,
) -> Result<Vec<RequirementRecord>, ExecutionSyncError> {
    if work_item.status != ProjectWorkItemStatus::Done {
        return Ok(Vec::new());
    }

    let requirements = store
        .list_requirements(&work_item.project_id, None, None)
        .await
        .map_err(ExecutionSyncError::bad_request)?;
    let requirement_by_id = requirements
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut candidate_ids = Vec::new();
    let mut seen = HashSet::new();
    let mut current_id = Some(work_item.requirement_id.as_str());
    while let Some(requirement_id) = current_id {
        if !seen.insert(requirement_id.to_string()) {
            break;
        }
        let Some(requirement) = requirement_by_id.get(requirement_id) else {
            break;
        };
        candidate_ids.push(requirement.id.clone());
        current_id = requirement.parent_requirement_id.as_deref();
    }

    if candidate_ids.is_empty() {
        return Ok(Vec::new());
    }

    let project_work_items = store
        .list_work_items_by_project(&work_item.project_id, None, None, None)
        .await
        .map_err(ExecutionSyncError::bad_request)?;
    let mut updated_requirements = Vec::new();
    for requirement_id in candidate_ids {
        let Some(requirement) = requirement_by_id.get(requirement_id.as_str()) else {
            continue;
        };
        if !requirement_status_can_complete_from_work_items(requirement.status) {
            continue;
        }

        let subtree_ids =
            collect_requirement_subtree_ids_from_list(&requirements, requirement.id.as_str());
        let active_work_items = project_work_items
            .iter()
            .filter(|item| subtree_ids.contains(item.requirement_id.as_str()))
            .filter(|item| item.status != ProjectWorkItemStatus::Archived)
            .collect::<Vec<_>>();
        if active_work_items.is_empty() {
            continue;
        }
        if !active_work_items
            .iter()
            .all(|item| item.status == ProjectWorkItemStatus::Done)
        {
            continue;
        }

        if let Some(updated_requirement) = store
            .update_requirement(
                requirement.id.as_str(),
                UpdateRequirementRequest {
                    status: Some(RequirementStatus::Done),
                    ..UpdateRequirementRequest::default()
                },
            )
            .await
            .map_err(ExecutionSyncError::bad_request)?
        {
            updated_requirements.push(updated_requirement);
        }
    }

    Ok(updated_requirements)
}

pub(super) async fn recover_related_requirements_if_work_item_recovered(
    store: &AppStore,
    work_item: &ProjectWorkItemRecord,
) -> Result<Vec<RequirementRecord>, ExecutionSyncError> {
    if matches!(
        work_item.status,
        ProjectWorkItemStatus::Failed
            | ProjectWorkItemStatus::Blocked
            | ProjectWorkItemStatus::Archived
    ) {
        return Ok(Vec::new());
    }

    let requirements = store
        .list_requirements(&work_item.project_id, None, None)
        .await
        .map_err(ExecutionSyncError::bad_request)?;
    let requirement_by_id = requirements
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let project_work_items = store
        .list_work_items_by_project(&work_item.project_id, None, None, None)
        .await
        .map_err(ExecutionSyncError::bad_request)?;

    let mut candidate_ids = Vec::new();
    let mut seen = HashSet::new();
    let mut current_id = Some(work_item.requirement_id.as_str());
    while let Some(requirement_id) = current_id {
        if !seen.insert(requirement_id.to_string()) {
            break;
        }
        let Some(requirement) = requirement_by_id.get(requirement_id) else {
            break;
        };
        candidate_ids.push(requirement.id.clone());
        current_id = requirement.parent_requirement_id.as_deref();
    }

    let mut updated_requirements = Vec::new();
    for requirement_id in candidate_ids {
        let Some(requirement) = requirement_by_id.get(requirement_id.as_str()) else {
            continue;
        };
        if !matches!(
            requirement.status,
            RequirementStatus::Failed | RequirementStatus::Blocked
        ) {
            continue;
        }

        let subtree_ids =
            collect_requirement_subtree_ids_from_list(&requirements, requirement.id.as_str());
        let active_work_items = project_work_items
            .iter()
            .filter(|item| subtree_ids.contains(item.requirement_id.as_str()))
            .filter(|item| item.status != ProjectWorkItemStatus::Archived)
            .collect::<Vec<_>>();
        if active_work_items.is_empty()
            || active_work_items
                .iter()
                .any(|item| item.status == ProjectWorkItemStatus::Failed)
        {
            continue;
        }

        let next_status = if active_work_items
            .iter()
            .any(|item| item.status == ProjectWorkItemStatus::Blocked)
        {
            RequirementStatus::Blocked
        } else if active_work_items
            .iter()
            .all(|item| item.status == ProjectWorkItemStatus::Done)
        {
            RequirementStatus::Done
        } else {
            RequirementStatus::InProgress
        };
        if next_status == requirement.status {
            continue;
        }

        if let Some(updated_requirement) = store
            .update_requirement(
                requirement.id.as_str(),
                UpdateRequirementRequest {
                    status: Some(next_status),
                    ..UpdateRequirementRequest::default()
                },
            )
            .await
            .map_err(ExecutionSyncError::bad_request)?
        {
            updated_requirements.push(updated_requirement);
        }
    }

    Ok(updated_requirements)
}

fn requirement_status_can_complete_from_work_items(status: RequirementStatus) -> bool {
    matches!(
        status,
        RequirementStatus::Approved
            | RequirementStatus::InProgress
            | RequirementStatus::Blocked
            | RequirementStatus::Failed
    )
}

fn requirement_status_can_fail_from_work_items(status: RequirementStatus) -> bool {
    matches!(
        status,
        RequirementStatus::Reviewing
            | RequirementStatus::Approved
            | RequirementStatus::InProgress
            | RequirementStatus::Blocked
    )
}

fn requirement_status_can_block_from_work_items(status: RequirementStatus) -> bool {
    matches!(
        status,
        RequirementStatus::Reviewing | RequirementStatus::Approved | RequirementStatus::InProgress
    )
}

fn collect_requirement_subtree_ids_from_list(
    requirements: &[RequirementRecord],
    root_id: &str,
) -> HashSet<String> {
    let mut scope = HashSet::from([root_id.to_string()]);
    loop {
        let before = scope.len();
        for requirement in requirements {
            if requirement
                .parent_requirement_id
                .as_deref()
                .is_some_and(|parent_id| scope.contains(parent_id))
            {
                scope.insert(requirement.id.clone());
            }
        }
        if scope.len() == before {
            break;
        }
    }
    scope
}
