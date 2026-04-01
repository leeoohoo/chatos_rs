use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRunTarget {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub cwd: String,
    pub command: String,
    pub source: String,
    pub confidence: f64,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRunCatalog {
    pub project_id: String,
    pub user_id: Option<String>,
    pub status: String,
    pub default_target_id: Option<String>,
    pub targets: Vec<ProjectRunTarget>,
    pub error_message: Option<String>,
    pub analyzed_at: Option<String>,
    pub updated_at: String,
}
