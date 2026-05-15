use serde::Serialize;

mod conversation_meta;
mod path_support;
mod project_scope;

pub use self::project_scope::{list_unconfirmed_project_changes, summarize_project_changes};

#[derive(Debug, Clone)]
pub struct ProjectScopedChangeRecord {
    pub id: String,
    pub path: String,
    pub relative_path: String,
    pub kind: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectChangeMark {
    pub path: String,
    pub relative_path: String,
    pub kind: String,
    pub last_change_id: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectChangeCounts {
    pub create: usize,
    pub edit: usize,
    pub delete: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectChangeSummary {
    pub file_marks: Vec<ProjectChangeMark>,
    pub deleted_marks: Vec<ProjectChangeMark>,
    pub counts: ProjectChangeCounts,
}
