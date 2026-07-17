// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalProjectProfileRecord {
    pub(crate) project_id: String,
    pub(crate) description: Option<String>,
    pub(crate) git_url: Option<String>,
    pub(crate) background: Option<String>,
    pub(crate) introduction: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalRequirementRecord {
    pub(crate) id: String,
    pub(crate) project_id: String,
    pub(crate) parent_requirement_id: Option<String>,
    pub(crate) requirement_type: String,
    pub(crate) title: String,
    pub(crate) summary: Option<String>,
    pub(crate) detail: Option<String>,
    pub(crate) business_value: Option<String>,
    pub(crate) acceptance_criteria: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) priority: i64,
    pub(crate) status: String,
    pub(crate) creator_user_id: Option<String>,
    pub(crate) owner_user_id: Option<String>,
    pub(crate) assignee_user_id: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) archived_at: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct LocalWorkItemRow {
    pub(crate) id: String,
    pub(crate) project_id: String,
    pub(crate) requirement_id: String,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) status: String,
    pub(crate) priority: i64,
    pub(crate) assignee_user_id: Option<String>,
    pub(crate) estimate_points: Option<i64>,
    pub(crate) due_at: Option<String>,
    pub(crate) sort_order: i64,
    pub(crate) tags_json: String,
    pub(crate) is_planning_task: bool,
    pub(crate) creator_user_id: Option<String>,
    pub(crate) owner_user_id: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalWorkItemRecord {
    pub(crate) id: String,
    pub(crate) project_id: String,
    pub(crate) requirement_id: String,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) status: String,
    pub(crate) priority: i64,
    pub(crate) assignee_user_id: Option<String>,
    pub(crate) estimate_points: Option<i64>,
    pub(crate) due_at: Option<String>,
    pub(crate) sort_order: i64,
    pub(crate) tags: Vec<String>,
    pub(crate) is_planning_task: bool,
    pub(crate) creator_user_id: Option<String>,
    pub(crate) owner_user_id: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) archived_at: Option<String>,
}

impl From<LocalWorkItemRow> for LocalWorkItemRecord {
    fn from(row: LocalWorkItemRow) -> Self {
        Self {
            id: row.id,
            project_id: row.project_id,
            requirement_id: row.requirement_id,
            title: row.title,
            description: row.description,
            status: row.status,
            priority: row.priority,
            assignee_user_id: row.assignee_user_id,
            estimate_points: row.estimate_points,
            due_at: row.due_at,
            sort_order: row.sort_order,
            tags: serde_json::from_str(row.tags_json.as_str()).unwrap_or_default(),
            is_planning_task: row.is_planning_task,
            creator_user_id: row.creator_user_id,
            owner_user_id: row.owner_user_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
            archived_at: row.archived_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalRequirementDocumentRecord {
    pub(crate) id: String,
    pub(crate) requirement_id: String,
    pub(crate) doc_type: String,
    pub(crate) title: String,
    pub(crate) format: String,
    pub(crate) content: String,
    pub(crate) version: i64,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalRequirementDependencyRecord {
    pub(crate) requirement_id: String,
    pub(crate) prerequisite_requirement_id: String,
    pub(crate) relation_type: String,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub(crate) struct LocalWorkItemDependencyRecord {
    pub(crate) work_item_id: String,
    pub(crate) prerequisite_work_item_id: String,
    pub(crate) relation_type: String,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalDependencyGraphNode {
    pub(crate) id: String,
    pub(crate) node_type: String,
    pub(crate) label: String,
    pub(crate) status: String,
    pub(crate) parent_id: Option<String>,
    pub(crate) raw_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalDependencyGraphEdge {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) edge_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalDependencyGraph {
    pub(crate) root_id: Option<String>,
    pub(crate) nodes: Vec<LocalDependencyGraphNode>,
    pub(crate) edges: Vec<LocalDependencyGraphEdge>,
    pub(crate) blocked_by: Vec<LocalDependencyGraphNode>,
    pub(crate) ready: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct LocalProjectPlanSnapshot {
    pub(crate) project_id: String,
    pub(crate) requirements: Vec<LocalRequirementRecord>,
    pub(crate) work_items: Vec<LocalWorkItemRecord>,
    pub(crate) dependency_graph: LocalDependencyGraph,
}
