// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[derive(Debug, Clone)]
pub(crate) struct CreateLocalRequirementInput {
    pub(crate) project_id: String,
    pub(crate) owner_user_id: String,
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
    pub(crate) assignee_user_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UpdateLocalRequirementInput {
    pub(crate) parent_requirement_id: Option<String>,
    pub(crate) requirement_type: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) detail: Option<String>,
    pub(crate) business_value: Option<String>,
    pub(crate) acceptance_criteria: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) priority: Option<i64>,
    pub(crate) status: Option<String>,
    pub(crate) assignee_user_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CreateLocalWorkItemInput {
    pub(crate) requirement_id: String,
    pub(crate) owner_user_id: String,
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
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UpdateLocalWorkItemInput {
    pub(crate) requirement_id: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) priority: Option<i64>,
    pub(crate) assignee_user_id: Option<String>,
    pub(crate) estimate_points: Option<i64>,
    pub(crate) due_at: Option<String>,
    pub(crate) sort_order: Option<i64>,
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) is_planning_task: Option<bool>,
}

#[derive(Debug, Clone)]
pub(crate) struct UpsertLocalRequirementDocumentInput {
    pub(crate) document_id: Option<String>,
    pub(crate) requirement_id: String,
    pub(crate) owner_user_id: String,
    pub(crate) doc_type: String,
    pub(crate) title: String,
    pub(crate) format: String,
    pub(crate) content: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UpsertLocalProjectProfileInput {
    pub(crate) description: Option<String>,
    pub(crate) git_url: Option<String>,
    pub(crate) background: Option<String>,
    pub(crate) introduction: Option<String>,
}
