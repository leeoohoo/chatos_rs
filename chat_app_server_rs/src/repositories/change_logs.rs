use serde::Serialize;

mod confirm;
mod conversation_meta;
mod listing;
mod path_support;
mod project_scope;

pub use self::confirm::confirm_change_logs_by_ids;
pub use self::listing::list_project_change_logs;
pub use self::project_scope::{list_unconfirmed_project_changes, summarize_project_changes};

#[derive(Debug, Clone, Serialize)]
pub struct ChangeLogItem {
    pub id: String,
    pub server_name: String,
    pub project_id: Option<String>,
    pub path: String,
    pub action: String,
    pub change_kind: String,
    pub bytes: i64,
    pub sha256: Option<String>,
    pub diff: Option<String>,
    pub conversation_id: Option<String>,
    pub run_id: Option<String>,
    pub confirmed: bool,
    pub confirmed_at: Option<String>,
    pub confirmed_by: Option<String>,
    pub created_at: String,
    pub conversation_title: Option<String>,
}

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
