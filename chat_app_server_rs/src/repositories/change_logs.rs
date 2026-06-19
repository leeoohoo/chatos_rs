use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectChangeMark {
    pub path: String,
    pub relative_path: String,
    pub kind: String,
    pub last_change_id: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectChangeCounts {
    pub create: usize,
    pub edit: usize,
    pub delete: usize,
    pub total: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct ProjectChangeSummarySnapshot {
    pub file_marks: Vec<ProjectChangeMark>,
    pub deleted_marks: Vec<ProjectChangeMark>,
    pub counts: ProjectChangeCounts,
}
