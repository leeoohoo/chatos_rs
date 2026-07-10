// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_project_mcp_contract::args::{
    ProjectTaskStatus as McpProjectTaskStatus, RequirementStatus as McpRequirementStatus,
    RequirementType as McpRequirementType, UpdateProjectTaskPatch, UpdateRequirementPatch,
};

use crate::models::{
    ProjectWorkItemStatus, RequirementStatus, RequirementType, UpdateProjectWorkItemRequest,
    UpdateRequirementRequest,
};

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
